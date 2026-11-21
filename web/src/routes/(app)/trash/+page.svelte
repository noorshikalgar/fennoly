<script lang="ts">
	import { RotateCcw, Trash, X } from '@lucide/svelte';
	import { api } from '$lib/api';
	import { purge, reportError } from '$lib/actions';
	import EmptyState from '$lib/components/EmptyState.svelte';
	import FileIcon from '$lib/components/FileIcon.svelte';
	import { formatBytes, relativeTime } from '$lib/format';
	import { t } from '$lib/i18n/index.svelte';
	import { folderChanged, toast } from '$lib/stores.svelte';
	import type { FileNode } from '$lib/types';
	import { confirmDialog, ui } from '$lib/ui.svelte';

	let items = $state<FileNode[]>([]);
	let loading = $state(true);

	async function load() {
		try {
			items = await api.trashList();
		} catch (e) {
			reportError(e);
		} finally {
			loading = false;
		}
	}

	$effect(() => {
		ui.folder = null;
		ui.space = null;
		load();
	});

	async function restore(n: FileNode) {
		try {
			await api.restore(n.id);
			toast(t('trash.restored', { name: n.name }), 'success');
			folderChanged('');
			load();
		} catch (e) {
			reportError(e);
		}
	}

	async function remove(n: FileNode) {
		if (await purge(n)) load();
	}

	async function emptyAll() {
		const ok = await confirmDialog({
			title: t('trash.emptyAll'),
			message: t('trash.confirmEmpty'),
			confirmLabel: t('common.deleteForever'),
			danger: true
		});
		if (!ok) return;
		try {
			await api.emptyTrash();
			load();
		} catch (e) {
			reportError(e);
		}
	}
</script>

<svelte:head><title>{t('trash.title')} · Fennoly</title></svelte:head>

<div class="flex items-center gap-3 pt-3 lg:pt-1 mb-1">
	<h1 class="text-xl sm:text-2xl font-bold tracking-tight flex-1">{t('trash.title')}</h1>
	{#if items.length}
		<button class="btn-ghost text-danger" onclick={emptyAll}><Trash size={18} />{t('trash.emptyAll')}</button>
	{/if}
</div>
<p class="text-sm text-muted mb-4">{t('trash.emptyBody')}</p>

{#if !loading && !items.length}
	<EmptyState title={t('trash.empty')}>
		{#snippet icon()}<Trash size={36} />{/snippet}
	</EmptyState>
{:else}
	<ul class="rounded-2xl border border-border bg-surface overflow-hidden">
		{#each items as n (n.id)}
			<li class="flex items-center gap-3 px-4 h-16 border-b border-border last:border-0">
				<span class="size-10 rounded-xl bg-surface-2 flex items-center justify-center shrink-0"><FileIcon node={n} size={20} /></span>
				<div class="flex-1 min-w-0">
					<div class="truncate font-medium text-sm">{n.name}</div>
					<div class="truncate text-xs text-muted">
						{n.trashed_at ? t('trash.deleted', { when: relativeTime(n.trashed_at) }) : ''}{n.kind === 'file' ? ` · ${formatBytes(n.size)}` : ''}
					</div>
				</div>
				<button class="btn-ghost h-9 px-3" onclick={() => restore(n)}><RotateCcw size={16} /><span class="hidden sm:inline">{t('common.restore')}</span></button>
				<button class="icon-btn hover:text-danger" title={t('common.deleteForever')} aria-label={t('common.deleteForever')} onclick={() => remove(n)}><X size={18} /></button>
			</li>
		{/each}
	</ul>
{/if}
