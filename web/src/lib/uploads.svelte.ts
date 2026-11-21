// Upload queue. Big files go up as several parallel parts (tus concatenation),
// small files in one request. Every upload resumes after a dropped connection.

import { Upload } from 'tus-js-client';
import { ApiError, api } from './api';
import { folderChanged } from './stores.svelte';

export type UploadStatus = 'waiting' | 'uploading' | 'done' | 'error' | 'cancelled';

export interface UploadItem {
	id: number;
	name: string;
	size: number;
	sent: number;
	status: UploadStatus;
	error?: string;
	parentId: string;
	startedAt?: number;
}

const MAX_FILES_AT_ONCE = 3;
const PARALLEL_THRESHOLD = 16 * 1024 * 1024; // split files larger than 16 MB
const CHUNK = 8 * 1024 * 1024;

export const uploads = $state<{ items: UploadItem[]; open: boolean }>({ items: [], open: false });

const files = new Map<number, File>();
const running = new Map<number, Upload>();
let nextId = 1;

export function uploadStats() {
	const items = uploads.items;
	const active = items.filter((i) => i.status === 'uploading' || i.status === 'waiting');
	const total = items.filter((i) => i.status !== 'cancelled').length;
	const done = items.filter((i) => i.status === 'done').length;
	const failed = items.filter((i) => i.status === 'error').length;
	const bytesTotal = active.reduce((s, i) => s + i.size, 0);
	const bytesSent = active.reduce((s, i) => s + i.sent, 0);
	const uploading = items.filter((i) => i.status === 'uploading' && i.startedAt);
	const speed = uploading.reduce((s, i) => s + i.sent / Math.max(0.5, (Date.now() - i.startedAt!) / 1000), 0);
	return { active: active.length, total, done, failed, bytesTotal, bytesSent, speed };
}

function pump() {
	const busy = uploads.items.filter((i) => i.status === 'uploading').length;
	const free = MAX_FILES_AT_ONCE - busy;
	const waiting = uploads.items.filter((i) => i.status === 'waiting').slice(0, Math.max(0, free));
	for (const item of waiting) start(item);
}

function find(id: number) {
	return uploads.items.find((i) => i.id === id);
}

function start(item: UploadItem) {
	const file = files.get(item.id);
	if (!file) return;
	item.status = 'uploading';
	item.startedAt = Date.now();
	item.sent = 0;
	const parallel = file.size > PARALLEL_THRESHOLD ? Math.min(4, Math.ceil(file.size / CHUNK)) : 1;
	const upload = new Upload(file, {
		endpoint: '/api/uploads',
		retryDelays: [0, 1000, 3000, 5000, 10000],
		chunkSize: parallel > 1 ? CHUNK : Infinity,
		parallelUploads: parallel,
		metadata: { filename: item.name, parentId: item.parentId, filetype: file.type },
		storeFingerprintForResuming: true,
		removeFingerprintOnSuccess: true,
		onProgress: (sent) => {
			const it = find(item.id);
			if (it) it.sent = sent;
		},
		onSuccess: () => {
			const it = find(item.id);
			if (it) {
				it.status = 'done';
				it.sent = it.size;
			}
			running.delete(item.id);
			files.delete(item.id);
			folderChanged(item.parentId);
			pump();
		},
		onError: (err) => {
			const it = find(item.id);
			if (it && it.status !== 'cancelled') {
				it.status = 'error';
				const body = (err as { originalResponse?: { getBody(): string } }).originalResponse?.getBody?.();
				let message = err.message;
				try {
					message = body ? JSON.parse(body).message ?? message : message;
				} catch {
					/* not JSON */
				}
				it.error = message;
			}
			running.delete(item.id);
			pump();
		}
	});
	running.set(item.id, upload);
	upload.findPreviousUploads().then((previous) => {
		if (previous.length) upload.resumeFromPreviousUpload(previous[0]);
		upload.start();
	});
}

/** Queues files for upload into a folder. */
export function enqueue(list: File[], parentId: string) {
	for (const file of list) {
		const id = nextId++;
		files.set(id, file);
		uploads.items.push({ id, name: file.name, size: file.size, sent: 0, status: 'waiting', parentId });
	}
	if (list.length) uploads.open = true;
	pump();
}

export function cancel(id: number) {
	const item = find(id);
	if (!item) return;
	item.status = 'cancelled';
	running.get(id)?.abort(true);
	running.delete(id);
	files.delete(id);
	pump();
}

export function retry(id: number) {
	const item = find(id);
	if (item && item.status === 'error') {
		item.status = 'waiting';
		item.error = undefined;
		pump();
	}
}

export function clearFinished() {
	uploads.items = uploads.items.filter((i) => i.status === 'uploading' || i.status === 'waiting');
	if (!uploads.items.length) uploads.open = false;
}

// ---- Folders -------------------------------------------------------------

export interface PathedFile {
	file: File;
	/** Folder path relative to the drop target, e.g. "Trip/Day 1"; empty for top level. */
	dir: string;
}

/** Creates (or finds) a folder path under `parentId` and returns the deepest folder's id. */
async function ensureFolder(parentId: string, dir: string, cache: Map<string, string>): Promise<string> {
	if (!dir) return parentId;
	if (cache.has(dir)) return cache.get(dir)!;
	const slash = dir.lastIndexOf('/');
	const parentDir = slash >= 0 ? dir.slice(0, slash) : '';
	const name = slash >= 0 ? dir.slice(slash + 1) : dir;
	const under = await ensureFolder(parentId, parentDir, cache);
	let id: string;
	try {
		id = (await api.createFolder(under, name)).id;
	} catch (e) {
		if (!(e instanceof ApiError && e.status === 409)) throw e;
		const existing = (await api.children(under)).find(
			(c) => c.kind === 'folder' && c.name.toLowerCase() === name.toLowerCase()
		);
		if (!existing) throw e;
		id = existing.id;
	}
	cache.set(dir, id);
	return id;
}

/** Uploads files that may come from a folder, recreating the folder structure. */
export async function enqueueTree(list: PathedFile[], parentId: string) {
	const cache = new Map<string, string>();
	const byFolder = new Map<string, File[]>();
	for (const { file, dir } of list) {
		const folder = await ensureFolder(parentId, dir, cache);
		if (!byFolder.has(folder)) byFolder.set(folder, []);
		byFolder.get(folder)!.push(file);
	}
	if (cache.size) folderChanged(parentId);
	for (const [folder, group] of byFolder) enqueue(group, folder);
}

/** Reads files (and folders) from a drop event. */
export async function filesFromDrop(dt: DataTransfer): Promise<PathedFile[]> {
	const entries = Array.from(dt.items)
		.map((i) => i.webkitGetAsEntry?.())
		.filter((e): e is FileSystemEntry => !!e);
	if (!entries.length) return Array.from(dt.files).map((file) => ({ file, dir: '' }));
	const out: PathedFile[] = [];
	async function walk(entry: FileSystemEntry, dir: string) {
		if (entry.isFile) {
			const file = await new Promise<File>((res, rej) => (entry as FileSystemFileEntry).file(res, rej));
			out.push({ file, dir });
		} else if (entry.isDirectory) {
			const reader = (entry as FileSystemDirectoryEntry).createReader();
			const path = dir ? `${dir}/${entry.name}` : entry.name;
			// readEntries returns at most ~100 entries per call.
			for (;;) {
				const batch = await new Promise<FileSystemEntry[]>((res, rej) => reader.readEntries(res, rej));
				if (!batch.length) break;
				for (const child of batch) await walk(child, path);
			}
		}
	}
	for (const e of entries) await walk(e, '');
	return out;
}

/** Files picked with <input webkitdirectory>. */
export function filesFromFolderInput(list: FileList): PathedFile[] {
	return Array.from(list).map((file) => {
		const rel = (file as File & { webkitRelativePath?: string }).webkitRelativePath || file.name;
		const dir = rel.split('/').slice(0, -1).join('/');
		return { file, dir };
	});
}
