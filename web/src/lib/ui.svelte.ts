// Global UI state: which dialog is open, the current folder, and promise-based
// confirm/prompt dialogs.

import type { FileNode } from './types';

export const ui = $state<{
	/** Folder shown in the browser, if any; the New/Upload buttons target it. */
	folder: { id: string; name: string; writable: boolean } | null;
	/** Space of the open folder, for highlighting navigation. */
	space: 'personal' | 'family' | 'shared' | null;
	share: FileNode | null;
	move: FileNode[] | null;
	compress: FileNode | null;
	viewer: { items: FileNode[]; index: number } | null;
	view: 'grid' | 'list';
	sidebarOpen: boolean;
}>({
	folder: null,
	space: null,
	share: null,
	move: null,
	compress: null,
	viewer: null,
	view: (() => {
		try {
			return (localStorage.getItem('fennoly-view') as 'grid' | 'list') || 'grid';
		} catch {
			return 'grid';
		}
	})(),
	sidebarOpen: false
});

export function setView(view: 'grid' | 'list') {
	ui.view = view;
	try {
		localStorage.setItem('fennoly-view', view);
	} catch {
		/* ignore */
	}
}

// ---- confirm / prompt ----------------------------------------------------

export interface DialogRequest {
	kind: 'confirm' | 'prompt';
	title: string;
	message?: string;
	label?: string;
	value?: string;
	confirmLabel: string;
	danger?: boolean;
	/** For prompts on file names: select the name without its extension. */
	selectStem?: boolean;
	inputType?: string;
	allowEmpty?: boolean;
	resolve: (value: string | boolean | null) => void;
}

export const dialog = $state<{ current: DialogRequest | null }>({ current: null });

export function confirmDialog(opts: {
	title: string;
	message?: string;
	confirmLabel: string;
	danger?: boolean;
}): Promise<boolean> {
	return new Promise((resolve) => {
		dialog.current = { kind: 'confirm', ...opts, resolve: (v) => resolve(v === true) };
	});
}

export function promptDialog(opts: {
	title: string;
	label?: string;
	value?: string;
	confirmLabel: string;
	selectStem?: boolean;
	inputType?: string;
	allowEmpty?: boolean;
}): Promise<string | null> {
	return new Promise((resolve) => {
		dialog.current = {
			kind: 'prompt',
			...opts,
			resolve: (v) => resolve(typeof v === 'string' ? v : null)
		};
	});
}
