import { api } from './api';
import { setLocale } from './i18n/index.svelte';
import type { BackupStatus, Me, ThemeChoice } from './types';

// ---- Session -------------------------------------------------------------

export const session = $state<{ me: Me | null }>({ me: null });

/** Stores the signed-in user and applies their language and theme. */
export function adoptPreferences(me: Me) {
	session.me = me;
	setLocale(me.locale);
	applyTheme(me.theme);
}

// ---- Theme ---------------------------------------------------------------

const media = typeof window !== 'undefined' ? matchMedia('(prefers-color-scheme: dark)') : null;

export const theme = $state<{ choice: ThemeChoice }>({
	choice: ((): ThemeChoice => {
		try {
			return (localStorage.getItem('fennoly-theme') as ThemeChoice) || 'system';
		} catch {
			return 'system';
		}
	})()
});

export function applyTheme(choice: ThemeChoice) {
	theme.choice = choice;
	try {
		localStorage.setItem('fennoly-theme', choice);
	} catch {
		/* ignore */
	}
	const resolved = choice === 'system' ? (media?.matches ? 'dark' : 'light') : choice;
	document.documentElement.dataset.theme = resolved;
	const meta = document.querySelector('meta[name="theme-color"]');
	meta?.setAttribute('content', getComputedStyle(document.documentElement).getPropertyValue('--bg').trim());
}

media?.addEventListener('change', () => {
	if (theme.choice === 'system') applyTheme('system');
});

// ---- Toasts --------------------------------------------------------------

export interface Toast {
	id: number;
	message: string;
	kind: 'info' | 'success' | 'error';
	action?: { label: string; run: () => void };
}

export const toasts = $state<Toast[]>([]);
let nextToast = 1;

export function toast(message: string, kind: Toast['kind'] = 'info', action?: Toast['action']) {
	const id = nextToast++;
	toasts.push({ id, message, kind, action });
	setTimeout(() => dismissToast(id), action ? 7000 : 4000);
}

export function dismissToast(id: number) {
	const i = toasts.findIndex((t) => t.id === id);
	if (i >= 0) toasts.splice(i, 1);
}

// ---- Folder refresh signal ----------------------------------------------

/** Bumped when something changes the contents of a folder (uploads, tools). */
export const changes = $state({ version: 0, folder: '' as string });

export function folderChanged(folderId: string) {
	changes.folder = folderId;
	changes.version++;
}

// ---- Backup health (admins) ------------------------------------------------

class BackupHealth {
	value = $state<BackupStatus | null>(null);

	/** Reloads the summary behind the "backups need attention" banner. */
	async refresh() {
		if (session.me?.role !== 'admin') {
			this.value = null;
			return;
		}
		try {
			this.value = await api.backupStatus();
		} catch {
			/* keep the last known state */
		}
	}
}

export const backupStatus = new BackupHealth();
