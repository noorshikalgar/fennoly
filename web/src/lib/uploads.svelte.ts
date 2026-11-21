// Upload manager for Fennoly's chunked upload protocol (the server side is in
// crates/fennoly-server/src/routes/chunked.rs).
//
// For each file:
//   1. hash it chunk by chunk in a Web Worker (BLAKE3),
//   2. send the plan; the server answers with the chunks it still needs
//      (everything for a new file, only the rest for an interrupted one),
//   3. send those chunks several at a time — the server checks every chunk
//      against its hash before accepting it,
//   4. ask the server to finish.
//
// Network trouble is expected, not exceptional: a failed chunk goes back in
// the queue, the file pauses (1 s, 2 s, 4 s … up to 30 s) and continues on its
// own when the connection returns, however long that takes. Nothing is lost
// or duplicated, because the server only counts verified chunks.

import { ApiError, api } from './api';
import { folderChanged } from './stores.svelte';
import type { HashMessage, HashRequest } from './upload/hash.worker';
import { backoff, chunkRange, chunkSizeFor, isTransient } from './upload/plan';

export type UploadStatus =
	| 'waiting'
	| 'hashing'
	| 'uploading'
	| 'retrying'
	| 'finishing'
	| 'done'
	| 'error'
	| 'cancelled';

export interface UploadItem {
	id: number;
	name: string;
	size: number;
	/** Bytes the server has confirmed, plus bytes in flight. */
	sent: number;
	status: UploadStatus;
	error?: string;
	parentId: string;
	/** 0–1 while preparing (hashing). */
	hashed: number;
	/** When the next attempt happens (ms since epoch) while status is 'retrying'. */
	retryAt?: number;
	/** True when an earlier, interrupted attempt was continued. */
	resumed: boolean;
	/** Uploading, but no bytes have moved for a while (connection slow or lost). */
	stalled: boolean;
}

const ACTIVE_STATUSES: UploadStatus[] = ['waiting', 'hashing', 'uploading', 'retrying', 'finishing'];
const MAX_ACTIVE_FILES = 3;
const MAX_INFLIGHT = 6; // chunk requests across all files
const MAX_INFLIGHT_PER_FILE = 4;
const STALL_MS = 20_000; // abort a chunk that has moved no bytes for this long
const STALLED_AFTER_MS = 8_000; // show "checking the connection" after this long without progress
const PROBE_EVERY_MS = 4_000; // how often to check the connection while it looks down
const MAX_CHECKSUM_RETRIES = 3;

export const uploads = $state<{ items: UploadItem[]; open: boolean; speed: number }>({
	items: [],
	open: false,
	speed: 0
});

interface Job {
	file: File;
	chunkSize: number;
	hashes?: string[];
	serverId?: string;
	/** Chunk indices still to send. */
	queue: number[];
	inflight: Map<number, { xhr: XMLHttpRequest; loaded: number }>;
	/** Bytes the server has accepted. */
	confirmed: number;
	/** Consecutive transient failures (drives the pause length). */
	failures: number;
	checksumFailures: Map<number, number>;
	pausedUntil: number;
	finishing: boolean;
	planning: boolean;
	/** Last time any byte of this file moved (ms since epoch). */
	lastProgress: number;
	/** Set while dead requests are being restarted: re-queue them without a pause. */
	restarting: boolean;
}

const jobs = new Map<number, Job>();
let nextId = 1;
/** The last connection check failed: start no new requests until one succeeds. */
let netDown = false;
let worker: Worker | null = null;
let wakeTimer: ReturnType<typeof setTimeout> | undefined;

function item(id: number) {
	return uploads.items.find((i) => i.id === id);
}

function isLive(id: number) {
	const it = item(id);
	return !!it && it.status !== 'cancelled' && it.status !== 'error' && it.status !== 'done';
}

// ---- hashing ---------------------------------------------------------------

function hasher(): Worker {
	if (!worker) {
		worker = new Worker(new URL('./upload/hash.worker.ts', import.meta.url), { type: 'module' });
		worker.onmessage = (e: MessageEvent<HashMessage>) => {
			const msg = e.data;
			const it = item(msg.job);
			const job = jobs.get(msg.job);
			if (!it || !job || it.status !== 'hashing') return;
			if (msg.type === 'progress') {
				it.hashed = msg.total ? msg.done / msg.total : 1;
			} else if (msg.type === 'error') {
				fail(msg.job, msg.message);
			} else {
				job.hashes = msg.hashes;
				it.hashed = 1;
				plan(msg.job);
			}
		};
	}
	return worker;
}

// ---- server calls ----------------------------------------------------------

async function plan(id: number) {
	const it = item(id);
	const job = jobs.get(id);
	if (!it || !job?.hashes || job.planning) return;
	job.planning = true;
	try {
		const res = await api.chunkedPlan({
			parent_id: it.parentId,
			filename: it.name,
			size: it.size,
			chunk_size: job.chunkSize,
			chunk_hashes: job.hashes
		});
		job.planning = false;
		if (!isLive(id)) return;
		job.serverId = res.id;
		job.queue = [...res.missing];
		const missingBytes = res.missing.reduce((sum, i) => {
			const [a, b] = chunkRange(i, it.size, job.chunkSize);
			return sum + (b - a);
		}, 0);
		job.confirmed = it.size - missingBytes;
		it.resumed = res.resumed && job.confirmed > 0;
		it.sent = job.confirmed;
		it.status = 'uploading';
		it.retryAt = undefined;
		it.stalled = false;
		job.failures = 0;
		job.pausedUntil = 0;
		job.lastProgress = Date.now();
		remember(it);
		if (!job.queue.length) finish(id);
		else pump();
	} catch (e) {
		job.planning = false;
		laterOrFail(id, e, () => plan(id));
	}
}

async function finish(id: number) {
	const it = item(id);
	const job = jobs.get(id);
	if (!it || !job?.serverId || job.finishing || !isLive(id)) return;
	job.finishing = true;
	it.status = 'finishing';
	try {
		await api.chunkedFinish(job.serverId);
		it.status = 'done';
		it.sent = it.size;
		jobs.delete(id);
		forget(it);
		folderChanged(it.parentId);
		pump();
	} catch (e) {
		job.finishing = false;
		if (e instanceof ApiError && e.status === 409) {
			// The server is still missing chunks: ask which, and send them.
			try {
				const status = await api.chunkedStatus(job.serverId);
				job.queue = [...status.missing];
				it.status = 'uploading';
				pump();
				return;
			} catch (e2) {
				return laterOrFail(id, e2, () => finish(id));
			}
		}
		if (e instanceof ApiError && e.status === 404) {
			it.status = 'uploading';
			return plan(id);
		}
		laterOrFail(id, e, () => finish(id));
	}
}

// ---- chunks ----------------------------------------------------------------

function inflightTotal() {
	let n = 0;
	for (const job of jobs.values()) n += job.inflight.size;
	return n;
}

function errorMessage(xhr: XMLHttpRequest): string {
	try {
		return JSON.parse(xhr.responseText).message ?? `HTTP ${xhr.status}`;
	} catch {
		return `HTTP ${xhr.status}`;
	}
}

function sendChunk(id: number, index: number) {
	const it = item(id)!;
	const job = jobs.get(id)!;
	const serverId = job.serverId;
	const [start, end] = chunkRange(index, it.size, job.chunkSize);
	const xhr = new XMLHttpRequest();
	const slot = { xhr, loaded: 0 };
	job.inflight.set(index, slot);

	let stall: ReturnType<typeof setTimeout> | undefined;
	const armStall = () => {
		clearTimeout(stall);
		stall = setTimeout(() => xhr.abort(), STALL_MS);
	};
	let settled = false;
	const settle = () => {
		if (settled) return false;
		settled = true;
		clearTimeout(stall);
		job.inflight.delete(index);
		updateSent(id);
		return true;
	};

	xhr.open('PUT', `/api/chunked/${serverId}/chunks/${index}`);
	job.lastProgress = Math.max(job.lastProgress, Date.now() - 1);
	xhr.upload.onprogress = (e) => {
		if (e.loaded > slot.loaded) job.lastProgress = Date.now();
		slot.loaded = e.loaded;
		updateSent(id);
		armStall();
	};
	xhr.onload = () => {
		if (!settle() || !isLive(id) || job.serverId !== serverId) return;
		if (xhr.status >= 200 && xhr.status < 300) {
			job.confirmed += end - start;
			job.failures = 0;
			job.lastProgress = Date.now();
			it.stalled = false;
			if (it.status === 'retrying') {
				it.status = 'uploading';
				it.retryAt = undefined;
			}
			updateSent(id);
			if (!job.queue.length && !job.inflight.size) finish(id);
			else pump();
			return;
		}
		const message = errorMessage(xhr);
		if (xhr.status === 400 && /checksum|cut short|interrupted/.test(message)) {
			// The bytes were damaged or cut off on the way: send this chunk again.
			const n = (job.checksumFailures.get(index) ?? 0) + 1;
			job.checksumFailures.set(index, n);
			if (n > MAX_CHECKSUM_RETRIES) return fail(id, message);
			job.queue.unshift(index);
			return pump();
		}
		if (xhr.status === 404) {
			// The server no longer knows this upload (e.g. cleaned up): plan again.
			abortAll(job);
			job.queue = [];
			return plan(id);
		}
		if (xhr.status === 409) return finish(id);
		if (isTransient(xhr.status)) return networkTrouble(id, index);
		fail(id, message);
	};
	// No response at all: offline, connection reset, network switch, stall.
	xhr.onerror = () => {
		if (settle()) networkTrouble(id, index);
	};
	xhr.onabort = () => {
		if (settle()) networkTrouble(id, index);
	};
	armStall();
	xhr.send(job.file.slice(start, end));
}

/** A chunk failed for network reasons: put it back and pause the file. */
function networkTrouble(id: number, index: number) {
	const it = item(id);
	const job = jobs.get(id);
	if (!it || !job || !isLive(id)) return;
	if (!job.queue.includes(index)) job.queue.unshift(index);
	if (job.restarting) return; // the network is fine; the caller sends it again right away
	// Only count one failure per pause, however many chunks were in flight.
	if (job.pausedUntil > Date.now()) return;
	job.failures++;
	const wait = backoff(job.failures);
	job.pausedUntil = Date.now() + wait;
	it.status = 'retrying';
	it.stalled = false;
	it.retryAt = job.pausedUntil;
	schedule(wait);
}

/** Retries `again` after a pause for network errors; fails the file otherwise. */
function laterOrFail(id: number, e: unknown, again: () => void) {
	const it = item(id);
	const job = jobs.get(id);
	if (!it || !job || !isLive(id)) return;
	const status = e instanceof ApiError ? e.status : 0; // 0 = no response (fetch TypeError)
	if (!isTransient(status)) return fail(id, e instanceof ApiError ? e.message : String(e));
	job.failures++;
	const wait = backoff(job.failures);
	it.status = 'retrying';
	it.retryAt = Date.now() + wait;
	setTimeout(() => {
		if (item(id)?.status !== 'retrying') return;
		it.retryAt = undefined;
		it.status = job.serverId ? 'uploading' : 'hashing';
		again();
	}, wait);
}

function updateSent(id: number) {
	const it = item(id);
	const job = jobs.get(id);
	if (!it || !job) return;
	let flying = 0;
	for (const s of job.inflight.values()) flying += s.loaded;
	it.sent = Math.min(it.size, job.confirmed + flying);
}

function abortAll(job: Job) {
	const list = [...job.inflight.values()];
	job.inflight.clear();
	for (const { xhr } of list) xhr.abort();
}

function fail(id: number, message: string) {
	const it = item(id);
	const job = jobs.get(id);
	if (!it) return;
	it.status = 'error';
	it.error = message;
	it.retryAt = undefined;
	if (job) abortAll(job);
	pump();
}

// ---- scheduler -------------------------------------------------------------

function schedule(ms: number) {
	clearTimeout(wakeTimer);
	wakeTimer = setTimeout(pump, Math.max(0, ms));
}

/** Starts preparing waiting files and fills free chunk slots, round-robin. */
function pump() {
	const now = Date.now();
	const busy = uploads.items.filter((i) => ['hashing', 'uploading', 'retrying', 'finishing'].includes(i.status)).length;
	const start = uploads.items.filter((i) => i.status === 'waiting').slice(0, Math.max(0, MAX_ACTIVE_FILES - busy));
	for (const it of start) {
		const job = jobs.get(it.id)!;
		it.status = 'hashing';
		if (job.hashes) plan(it.id);
		else hasher().postMessage({ job: it.id, file: job.file, chunkSize: job.chunkSize } satisfies HashRequest);
	}

	let nextWake = Infinity;
	let progressed = true;
	while (progressed && inflightTotal() < MAX_INFLIGHT) {
		progressed = false;
		for (const it of uploads.items) {
			const job = jobs.get(it.id);
			if (!job?.serverId || job.finishing || job.planning || !job.queue.length) continue;
			if (it.status !== 'uploading' && it.status !== 'retrying') continue;
			if (job.pausedUntil > now) {
				nextWake = Math.min(nextWake, job.pausedUntil);
				continue;
			}
			if (netDown) {
				nextWake = Math.min(nextWake, now + PROBE_EVERY_MS);
				continue;
			}
			if (!navigator.onLine) {
				// The device says it's offline; look again shortly (or on 'online').
				nextWake = Math.min(nextWake, now + 5000);
				continue;
			}
			if (job.inflight.size >= MAX_INFLIGHT_PER_FILE || inflightTotal() >= MAX_INFLIGHT) continue;
			if (it.status === 'retrying') {
				it.status = 'uploading';
				it.retryAt = undefined;
			}
			sendChunk(it.id, job.queue.shift()!);
			progressed = true;
		}
	}
	if (isFinite(nextWake)) schedule(nextWake - now);
}

if (typeof window !== 'undefined') {
	// Continue right away when the connection comes back.
	window.addEventListener('online', () => {
		for (const job of jobs.values()) job.pausedUntil = 0;
		pump();
	});
	// Warn before closing the tab mid-upload (the upload can be resumed later).
	window.addEventListener('beforeunload', (e) => {
		if (uploads.items.some((i) => ACTIVE_STATUSES.includes(i.status))) e.preventDefault();
	});
	// Every second: smoothed overall speed, and spot files whose bytes stopped moving.
	let last = 0;
	setInterval(() => {
		const total = uploads.items.reduce((s, i) => s + i.sent, 0);
		const delta = Math.max(0, total - last);
		last = total;
		uploads.speed = uploads.items.some((i) => i.status === 'uploading') ? uploads.speed * 0.6 + delta * 0.4 : 0;
		const now = Date.now();
		for (const it of uploads.items) {
			const job = jobs.get(it.id);
			const stalled = !!job && it.status === 'uploading' && job.inflight.size > 0 && now - job.lastProgress > STALLED_AFTER_MS;
			if (it.stalled !== stalled) it.stalled = stalled;
		}
		if (uploads.items.some((i) => i.stalled || i.status === 'retrying')) watchConnection();
	}, 1000);
}

// ---- connection watch --------------------------------------------------------
// A dead connection rarely produces an error: requests just hang. So while a
// file is stalled or pausing, ask the server a tiny question every few seconds.
// No answer: drop the hung requests now (instead of waiting for the watchdog).
// An answer: the network is back, so stop pausing and continue immediately.

let watching = false;

async function reachable(): Promise<boolean> {
	const ctrl = new AbortController();
	const t = setTimeout(() => ctrl.abort(), 3000);
	try {
		const res = await fetch('/api/health', { cache: 'no-store', signal: ctrl.signal });
		return res.ok;
	} catch {
		return false;
	} finally {
		clearTimeout(t);
	}
}

async function watchConnection() {
	if (watching) return;
	watching = true;
	try {
		while (netDown || uploads.items.some((i) => i.stalled || i.status === 'retrying')) {
			const up = await reachable();
			const wasDown = netDown;
			netDown = !up;
			for (const it of uploads.items) {
				const job = jobs.get(it.id);
				if (!job) continue;
				if (up && it.status === 'retrying' && job.pausedUntil > Date.now()) {
					// The network is back: stop pausing.
					job.pausedUntil = 0;
					job.failures = Math.min(job.failures, 1);
				}
				if (it.stalled && job.inflight.size) {
					if (up) {
						// The server answers but these requests moved nothing: they are dead
						// (opened just as the network dropped). Restart them without a pause.
						job.restarting = true;
						abortAll(job);
						job.restarting = false;
						job.lastProgress = Date.now();
						it.stalled = false;
					} else {
						abortAll(job); // their handlers queue the chunks again and pause
					}
				}
			}
			if (up) pump();
			if (up && wasDown) continue; // re-check soon after recovering
			await new Promise((r) => setTimeout(r, PROBE_EVERY_MS));
		}
	} finally {
		watching = false;
	}
}

// ---- public API ------------------------------------------------------------

export function uploadStats() {
	const items = uploads.items;
	const active = items.filter((i) => ACTIVE_STATUSES.includes(i.status));
	return {
		active: active.length,
		total: items.filter((i) => i.status !== 'cancelled').length,
		done: items.filter((i) => i.status === 'done').length,
		failed: items.filter((i) => i.status === 'error').length,
		waitingForNetwork: items.some((i) => i.status === 'retrying' || i.stalled),
		bytesTotal: active.reduce((s, i) => s + i.size, 0),
		bytesSent: active.reduce((s, i) => s + i.sent, 0),
		speed: uploads.speed
	};
}

/** Queues files for upload into a folder. */
export function enqueue(list: File[], parentId: string) {
	for (const file of list) {
		const id = nextId++;
		jobs.set(id, {
			file,
			chunkSize: chunkSizeFor(file.size),
			queue: [],
			inflight: new Map(),
			confirmed: 0,
			failures: 0,
			checksumFailures: new Map(),
			pausedUntil: 0,
			finishing: false,
			planning: false,
			lastProgress: 0,
			restarting: false
		});
		uploads.items.push({
			id,
			name: file.name,
			size: file.size,
			sent: 0,
			status: 'waiting',
			parentId,
			hashed: 0,
			resumed: false,
			stalled: false
		});
	}
	if (list.length) uploads.open = true;
	pump();
}

export function cancel(id: number) {
	const it = item(id);
	if (!it) return;
	const job = jobs.get(id);
	it.status = 'cancelled';
	if (job) {
		abortAll(job);
		worker?.postMessage({ cancel: id });
		if (job.serverId) api.chunkedCancel(job.serverId).catch(() => {});
		jobs.delete(id);
	}
	forget(it);
	pump();
}

/** Retries a failed file, or skips the pause of one waiting for the network. */
export function retry(id: number) {
	const it = item(id);
	const job = jobs.get(id);
	if (!it || !job) return;
	job.failures = 0;
	job.pausedUntil = 0;
	job.checksumFailures.clear();
	it.retryAt = undefined;
	if (it.status === 'error') {
		it.error = undefined;
		it.status = 'waiting'; // re-plans (and re-syncs the missing chunks) when its turn comes
		job.serverId = undefined;
		job.queue = [];
	} else if (it.status === 'retrying') {
		it.status = job.serverId ? 'uploading' : 'hashing';
		if (!job.serverId && job.hashes) plan(id);
	}
	pump();
}

export function clearFinished() {
	uploads.items = uploads.items.filter((i) => ACTIVE_STATUSES.includes(i.status));
	if (!uploads.items.length) uploads.open = false;
}

// ---- interrupted uploads (survive a closed tab) ---------------------------

export interface PendingUpload {
	name: string;
	size: number;
	parentId: string;
	at: number;
}

const PENDING_KEY = 'fennoly-pending-uploads';

export function pendingUploads(): PendingUpload[] {
	try {
		const list = JSON.parse(localStorage.getItem(PENDING_KEY) ?? '[]') as PendingUpload[];
		// The server keeps partial uploads for 3 days.
		return list.filter((p) => Date.now() - p.at < 3 * 86400_000);
	} catch {
		return [];
	}
}

function savePending(list: PendingUpload[]) {
	try {
		localStorage.setItem(PENDING_KEY, JSON.stringify(list));
	} catch {
		/* storage unavailable */
	}
}

const same = (a: { name: string; size: number; parentId: string }, b: { name: string; size: number; parentId: string }) =>
	a.name === b.name && a.size === b.size && a.parentId === b.parentId;

function remember(it: UploadItem) {
	const list = pendingUploads().filter((p) => !same(p, it));
	list.push({ name: it.name, size: it.size, parentId: it.parentId, at: Date.now() });
	savePending(list);
}

function forget(it: { name: string; size: number; parentId: string }) {
	savePending(pendingUploads().filter((p) => !same(p, it)));
}

/** Interrupted uploads from an earlier visit that are not running now. */
export function resumable(): PendingUpload[] {
	return pendingUploads().filter((p) => !uploads.items.some((i) => same(i, p) && ACTIVE_STATUSES.includes(i.status)));
}

export function dismissPending(p: PendingUpload) {
	forget(p);
}

/** Re-adds picked files to continue interrupted uploads, each into its original folder. */
export function resumePending(files: File[]): number {
	const pending = pendingUploads();
	let matched = 0;
	for (const file of files) {
		const match = pending.find((p) => p.name === file.name && p.size === file.size);
		if (match) {
			enqueue([file], match.parentId);
			matched++;
		}
	}
	return matched;
}

// ---- folders ---------------------------------------------------------------

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

/** Reads files (and folders) from a drop event. Must start synchronously in the drop handler. */
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
