<script lang="ts">
	import { ArchiveRestore, ChevronLeft, ChevronRight, Folder, History, LoaderCircle } from '@lucide/svelte';
	import { api } from '$lib/api';
	import { reportError } from '$lib/actions';
	import FileIcon from '$lib/components/FileIcon.svelte';
	import Modal from '$lib/components/Modal.svelte';
	import { formatBytes, formatDateTime, relativeTime } from '$lib/format';
	import { t } from '$lib/i18n/index.svelte';
	import type { BackupEntry, BackupSnapshot, BackupTarget } from '$lib/types';
	import { confirmDialog } from '$lib/ui.svelte';

	let {
		target,
		onclose,
		onrestore
	}: { target: BackupTarget; onclose: () => void; onrestore: (runId: string) => void } = $props();

	let snapshots = $state<BackupSnapshot[] | null>(null);
	let loadError = $state<string | null>(null);
	let snap = $state<BackupSnapshot | null>(null);
	/** Folders opened inside the snapshot: [{rel, name}] from the top. */
	let trail = $state<{ rel: string; name: string }[]>([]);
	let entries = $state<BackupEntry[] | null>(null);
	let selected = $state<Record<string, BackupEntry>>({});
	let restoring = $state(false);
	let token = 0;

	/** Enough of a type for the right icon (snapshots do not record MIME types). */
	function guessMime(name: string): string | null {
		const ext = name.split('.').pop()?.toLowerCase() ?? '';
		if (ext === 'pdf') return 'application/pdf';
		if (['jpg', 'jpeg', 'png', 'gif', 'webp', 'heic', 'avif', 'bmp'].includes(ext)) return 'image/' + ext;
		if (['mp4', 'mov', 'mkv', 'webm'].includes(ext)) return 'video/' + ext;
		if (['mp3', 'm4a', 'wav', 'ogg', 'flac'].includes(ext)) return 'audio/' + ext;
		return null;
	}

	const here = $derived(trail.length ? trail[trail.length - 1].rel : '');
	const count = $derived(Object.keys(selected).length);

	$effect(() => {
		api
			.backupSnapshots(target.id)
			.then((s) => (snapshots = s))
			.catch((e) => {
				loadError = e?.message ?? t('errors.generic');
				snapshots = [];
			});
	});

	async function open(path: string) {
		if (!snap) return;
		const mine = ++token;
		entries = null;
		try {
			const list = await api.backupBrowse(target.id, snap.id, path);
			if (mine === token) entries = list;
		} catch (e) {
			if (mine === token) entries = [];
			reportError(e);
		}
	}

	function pick(s: BackupSnapshot) {
		snap = s;
		trail = [];
		selected = {};
		open('');
	}

	function enter(e: BackupEntry) {
		trail = [...trail, { rel: e.rel, name: e.label ?? e.name }];
		open(e.rel);
	}

	function up(to: number) {
		trail = trail.slice(0, to);
		open(here);
	}

	function toggle(e: BackupEntry) {
		if (selected[e.rel]) delete selected[e.rel];
		else selected[e.rel] = e;
	}

	async function restore() {
		if (!snap || count === 0) return;
		const ok = await confirmDialog({
			title: t('backup.restoreTitle'),
			message: t('backup.restoreConfirm', { n: count }),
			confirmLabel: t('common.restore')
		});
		if (!ok) return;
		restoring = true;
		try {
			const { run_id } = await api.backupRestore(target.id, snap.id, Object.keys(selected));
			onrestore(run_id);
		} catch (e) {
			reportError(e);
		} finally {
			restoring = false;
		}
	}
</script>

<Modal title={snap ? t('backup.snapshotOf', { when: formatDateTime(snap.time) }) : t('backup.snapshotsTitle', { name: target.name })} size="lg" {onclose}>
	{#if !snap}
		{#if snapshots === null}
			<p class="text-sm text-muted py-8 text-center flex items-center justify-center gap-2">
				<LoaderCircle size={16} class="animate-spin" />{t('backup.readingBackups')}
			</p>
		{:else if loadError}
			<p class="text-sm text-danger py-6 text-center">{loadError}</p>
		{:else if snapshots.length === 0}
			<p class="text-sm text-muted py-6 text-center">{t('backup.noSnapshots')}</p>
		{:else}
			<p class="text-sm text-muted mb-3">{t('backup.pickSnapshot')}</p>
			<ul class="grid grid-cols-1 gap-1">
				{#each snapshots as s (s.id)}
					<li>
						<button type="button" class="w-full flex items-center gap-3 rounded-xl px-3 py-2.5 hover:bg-surface-2 text-left" onclick={() => pick(s)}>
							<History size={18} class="text-muted shrink-0" />
							<span class="flex-1 min-w-0">
								<span class="block text-sm font-medium">{formatDateTime(s.time)}</span>
								<span class="block text-xs text-muted">{relativeTime(s.time)} · {s.short_id}</span>
							</span>
							<ChevronRight size={18} class="text-muted" />
						</button>
					</li>
				{/each}
			</ul>
		{/if}
	{:else}
		<nav class="flex items-center gap-1 flex-wrap text-sm mb-3 -ml-1" aria-label={t('backup.where')}>
			<button type="button" class="btn-ghost h-8 px-2" onclick={() => (snap = null)} aria-label={t('common.back')}>
				<ChevronLeft size={16} />
			</button>
			<button type="button" class="px-1.5 rounded-lg hover:bg-surface-2 {trail.length ? 'text-muted' : 'font-semibold'}" onclick={() => up(0)}>
				{t('backup.everything')}
			</button>
			{#each trail as c, i (c.rel)}
				<ChevronRight size={14} class="text-muted shrink-0" />
				<button
					type="button"
					class="px-1.5 rounded-lg hover:bg-surface-2 truncate max-w-[12rem] {i === trail.length - 1 ? 'font-semibold' : 'text-muted'}"
					onclick={() => up(i + 1)}>{c.name}</button
				>
			{/each}
		</nav>

		{#if entries === null}
			<p class="text-sm text-muted py-8 text-center flex items-center justify-center gap-2">
				<LoaderCircle size={16} class="animate-spin" />{t('common.loading')}
			</p>
		{:else if entries.length === 0}
			<p class="text-sm text-muted py-6 text-center">{t('backup.emptyFolder')}</p>
		{:else}
			<ul class="grid grid-cols-1 border border-border rounded-2xl divide-y divide-border overflow-hidden">
				{#each entries as e (e.rel)}
					<li class="flex items-center gap-2 pr-2 {selected[e.rel] ? 'bg-primary-soft/50' : ''}">
						<label class="pl-3 py-3 cursor-pointer">
							<input
								type="checkbox"
								class="size-[18px] accent-[var(--primary)]"
								checked={!!selected[e.rel]}
								onchange={() => toggle(e)}
								aria-label={e.label ?? e.name}
							/>
						</label>
						{#if e.is_dir}
							<button type="button" class="flex-1 min-w-0 flex items-center gap-3 py-2.5 text-left" onclick={() => enter(e)}>
								<Folder size={20} class="text-primary shrink-0" />
								<span class="truncate text-sm">{e.label ?? e.name}</span>
								<ChevronRight size={16} class="text-muted ml-auto shrink-0" />
							</button>
						{:else}
							<div class="flex-1 min-w-0 flex items-center gap-3 py-2.5">
								<FileIcon node={{ kind: 'file', mime: guessMime(e.name), name: e.name }} size={20} />
								<span class="truncate text-sm flex-1">{e.name}</span>
								{#if e.size !== null}<span class="text-xs text-muted tabular-nums shrink-0">{formatBytes(e.size)}</span>{/if}
							</div>
						{/if}
					</li>
				{/each}
			</ul>
		{/if}
		<p class="text-xs text-muted mt-3">{t('backup.restoreHint')}</p>
	{/if}

	{#snippet footer()}
		<button class="btn-ghost" onclick={onclose}>{t('common.close')}</button>
		{#if snap}
			<button class="btn-primary" disabled={count === 0 || restoring} onclick={restore}>
				<ArchiveRestore size={17} />{count ? t('backup.restoreN', { n: count }) : t('common.restore')}
			</button>
		{/if}
	{/snippet}
</Modal>
