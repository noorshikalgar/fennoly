import { INTL_LOCALE, i18n } from './i18n/index.svelte';
import type { FileNode } from './types';

const UNITS = ['B', 'KB', 'MB', 'GB', 'TB'];

export function formatBytes(bytes: number): string {
	let n = bytes;
	let u = 0;
	while (n >= 1024 && u < UNITS.length - 1) {
		n /= 1024;
		u++;
	}
	const digits = u === 0 || n >= 100 ? 0 : 1;
	const num = new Intl.NumberFormat(INTL_LOCALE[i18n.locale], { maximumFractionDigits: digits }).format(n);
	return `${num} ${UNITS[u]}`;
}

export function formatDate(iso: string): string {
	return new Intl.DateTimeFormat(INTL_LOCALE[i18n.locale], { dateStyle: 'medium' }).format(new Date(iso));
}

export function formatDateTime(iso: string): string {
	return new Intl.DateTimeFormat(INTL_LOCALE[i18n.locale], { dateStyle: 'medium', timeStyle: 'short' }).format(
		new Date(iso)
	);
}

/** "2 hours ago", "yesterday", or a date for anything older than a week.
 * `short` gives compact forms ("2 hr. ago") for tight spaces such as cards. */
export function relativeTime(iso: string, style: 'long' | 'short' = 'long'): string {
	const diff = (Date.parse(iso) - Date.now()) / 1000;
	const rtf = new Intl.RelativeTimeFormat(INTL_LOCALE[i18n.locale], { numeric: 'auto', style });
	const abs = Math.abs(diff);
	// Small clock differences between server and device should read as "now".
	if (abs < 45) return rtf.format(0, 'second');
	if (abs < 60) return rtf.format(Math.round(diff), 'second');
	if (abs < 3600) return rtf.format(Math.round(diff / 60), 'minute');
	if (abs < 86400) return rtf.format(Math.round(diff / 3600), 'hour');
	if (abs < 7 * 86400) return rtf.format(Math.round(diff / 86400), 'day');
	return formatDate(iso);
}

export function formatDuration(seconds: number): string {
	if (!isFinite(seconds) || seconds < 0) return '';
	const s = Math.round(seconds);
	if (s < 60) return `${s}s`;
	const m = Math.floor(s / 60);
	if (m < 60) return `${m}m ${s % 60}s`;
	return `${Math.floor(m / 60)}h ${m % 60}m`;
}

export type FileCategory =
	| 'folder'
	| 'image'
	| 'pdf'
	| 'video'
	| 'audio'
	| 'doc'
	| 'sheet'
	| 'slides'
	| 'archive'
	| 'text'
	| 'other';

export function category(node: Pick<FileNode, 'kind' | 'mime' | 'name'>): FileCategory {
	if (node.kind === 'folder') return 'folder';
	const mime = node.mime ?? '';
	const ext = node.name.split('.').pop()?.toLowerCase() ?? '';
	if (mime.startsWith('image/')) return 'image';
	if (mime === 'application/pdf') return 'pdf';
	if (mime.startsWith('video/')) return 'video';
	if (mime.startsWith('audio/')) return 'audio';
	if (['doc', 'docx', 'odt', 'rtf'].includes(ext)) return 'doc';
	if (['xls', 'xlsx', 'ods', 'csv'].includes(ext)) return 'sheet';
	if (['ppt', 'pptx', 'odp'].includes(ext)) return 'slides';
	if (['zip', 'rar', '7z', 'gz', 'tar'].includes(ext)) return 'archive';
	if (mime.startsWith('text/') || ['md', 'json', 'txt', 'log'].includes(ext)) return 'text';
	return 'other';
}

/** Can the built-in viewer show this file? */
export function previewable(node: FileNode): boolean {
	const c = category(node);
	return ['image', 'pdf', 'video', 'audio', 'text'].includes(c) && node.mime !== 'image/svg+xml';
}
