// File actions shared by the browser, menus, viewer and selection bar.

import { goto } from '$app/navigation';
import { ApiError, api, contentUrl, zipUrl } from './api';
import { category, previewable } from './format';
import { t } from './i18n/index.svelte';
import { folderChanged, toast } from './stores.svelte';
import type { FileNode } from './types';
import { confirmDialog, promptDialog, ui } from './ui.svelte';

export function reportError(e: unknown) {
	if (e instanceof ApiError) toast(e.message || t('errors.generic'), 'error');
	else if (e instanceof TypeError) toast(t('errors.network'), 'error');
	else toast(t('errors.generic'), 'error');
	console.error(e);
}

export const canWrite = (n: FileNode) => n.permission === 'owner' || n.permission === 'editor';
export const canShare = (n: FileNode) => n.permission === 'owner' && n.space_kind !== 'family';
/** Owners can link their items; in the family space every editor can. */
export const canLink = (n: FileNode) => n.permission === 'owner' || (n.space_kind === 'family' && canWrite(n));

/** Copies text, falling back to a hidden textarea where the Clipboard API is
 * unavailable (plain http on a home network is not a "secure context"). */
export async function copyText(text: string): Promise<boolean> {
	try {
		await navigator.clipboard.writeText(text);
		return true;
	} catch {
		const area = document.createElement('textarea');
		area.value = text;
		area.style.position = 'fixed';
		area.style.opacity = '0';
		document.body.appendChild(area);
		area.select();
		const ok = document.execCommand('copy');
		area.remove();
		return ok;
	}
}
export const canCompress = (n: FileNode) =>
	n.kind === 'file' && canWrite(n) && (n.mime === 'application/pdf' || (category(n) === 'image' && n.mime !== 'image/svg+xml'));

export function openNode(node: FileNode, siblings: FileNode[] = [node]) {
	if (node.kind === 'folder') {
		goto(`/files/${node.id}`);
		return;
	}
	if (previewable(node)) {
		const files = siblings.filter((n) => n.kind === 'file');
		ui.viewer = { items: files, index: Math.max(0, files.findIndex((n) => n.id === node.id)) };
	} else {
		download([node]);
	}
}

function navigate(href: string) {
	const a = document.createElement('a');
	a.href = href;
	a.rel = 'noopener';
	document.body.appendChild(a);
	a.click();
	a.remove();
}

export function download(nodes: FileNode[]) {
	if (nodes.length === 1 && nodes[0].kind === 'file') navigate(contentUrl(nodes[0].id, true));
	else if (nodes.length) navigate(zipUrl(nodes.map((n) => n.id)));
}

export async function rename(node: FileNode): Promise<boolean> {
	const name = await promptDialog({
		title: t('files.renameTitle'),
		label: t('files.newName'),
		value: node.name,
		confirmLabel: t('common.rename'),
		selectStem: node.kind === 'file'
	});
	if (!name || name === node.name) return false;
	try {
		await api.rename(node.id, name);
		return true;
	} catch (e) {
		reportError(e);
		return false;
	}
}

export async function newFolder(parentId: string): Promise<FileNode | null> {
	const name = await promptDialog({
		title: t('files.newFolderTitle'),
		label: t('files.folderName'),
		value: t('files.untitledFolder'),
		confirmLabel: t('common.create')
	});
	if (!name) return null;
	try {
		return await api.createFolder(parentId, name);
	} catch (e) {
		reportError(e);
		return null;
	}
}

/** Moves items to the trash, with an Undo button in the toast. */
export async function trash(nodes: FileNode[], after: () => void) {
	const done: FileNode[] = [];
	for (const n of nodes) {
		try {
			await api.trash(n.id);
			done.push(n);
		} catch (e) {
			reportError(e);
		}
	}
	after();
	if (done.length) {
		folderChanged('');
		toast(t('files.movedToTrash'), 'info', {
			label: t('common.undo'),
			run: async () => {
				for (const n of done) {
					try {
						await api.restore(n.id);
					} catch (e) {
						reportError(e);
					}
				}
				after();
			}
		});
	}
}

export async function toggleStar(node: FileNode) {
	try {
		await api.star(node.id, !node.starred);
		node.starred = !node.starred;
	} catch (e) {
		reportError(e);
	}
}

export async function purge(node: FileNode): Promise<boolean> {
	const ok = await confirmDialog({
		title: t('common.deleteForever'),
		message: t('trash.confirmPurge', { name: node.name }),
		confirmLabel: t('common.deleteForever'),
		danger: true
	});
	if (!ok) return false;
	try {
		await api.purge(node.id);
		return true;
	} catch (e) {
		reportError(e);
		return false;
	}
}
