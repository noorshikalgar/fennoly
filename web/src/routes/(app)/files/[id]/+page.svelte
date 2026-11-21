<script lang="ts">
	import { ChevronRight, Eye, FolderOpen, FolderPlus, Upload } from '@lucide/svelte';
	import { page } from '$app/state';
	import { ApiError, api } from '$lib/api';
	import { canWrite, newFolder, reportError } from '$lib/actions';
	import EmptyState from '$lib/components/EmptyState.svelte';
	import FileBrowser from '$lib/components/FileBrowser.svelte';
	import { t } from '$lib/i18n/index.svelte';
	import { changes, folderChanged } from '$lib/stores.svelte';
	import type { Crumb, FileNode } from '$lib/types';
	import { ui } from '$lib/ui.svelte';
	import { enqueue } from '$lib/uploads.svelte';

	const id = $derived(page.params.id!);
	let folder = $state<FileNode | null>(null);
	let crumbs = $state<Crumb[]>([]);
	let items = $state<FileNode[]>([]);
	let loading = $state(true);
	let missing = $state(false);
	let fileInput: HTMLInputElement;

	const writable = $derived(folder ? canWrite(folder) : false);

	function rootLabel(c: Crumb) {
		return c.space === 'family' ? t('space.family') : c.space === 'personal' ? t('space.myFiles') : c.name;
	}
	const title = $derived(crumbs.length ? (crumbs.at(-1)!.is_root ? rootLabel(crumbs.at(-1)!) : crumbs.at(-1)!.name) : '');

	async function load(folderId: string, quiet = false) {
		if (!quiet) loading = true;
		missing = false;
		try {
			const [info, kids] = await Promise.all([api.node(folderId), api.children(folderId)]);
			if (folderId !== id) return;
			if (info.node.kind !== 'folder') {
				missing = true;
				return;
			}
			folder = info.node;
			crumbs = info.crumbs;
			items = kids;
			ui.space = info.crumbs[0]?.space ?? null;
			ui.folder = { id: folderId, name: title, writable: canWrite(info.node) };
		} catch (e) {
			if (e instanceof ApiError && e.status === 404) missing = true;
			else reportError(e);
		} finally {
			loading = false;
		}
	}

	$effect(() => {
		items = [];
		load(id);
		return () => {
			ui.folder = null;
		};
	});

	// Reload when uploads or dialogs change this folder.
	let seen = changes.version;
	$effect(() => {
		const v = changes.version;
		if (v !== seen) {
			seen = v;
			// Any change can affect this folder (a move or a restore elsewhere), so refresh quietly.
			load(id, true);
		}
	});
</script>

<svelte:head><title>{title || t('nav.files')} · Fennoly</title></svelte:head>

<input bind:this={fileInput} type="file" multiple hidden onchange={() => {
	if (fileInput.files?.length) enqueue(Array.from(fileInput.files), id);
	fileInput.value = '';
}} />

{#if missing}
	<EmptyState title={t('errors.notFound')} body={t('files.notFound')}>
		{#snippet icon()}<FolderOpen size={36} />{/snippet}
	</EmptyState>
{:else}
	<div class="pt-3 lg:pt-1 flex items-center gap-3 flex-wrap">
		<nav class="flex items-center flex-wrap gap-0.5 min-w-0 flex-1" aria-label="breadcrumbs">
			{#each crumbs as c, i (c.id)}
				{#if i > 0}<ChevronRight size={18} class="text-muted shrink-0" />{/if}
				{#if i === crumbs.length - 1}
					<h1 class="text-xl sm:text-2xl font-bold tracking-tight px-1 truncate">{c.is_root ? rootLabel(c) : c.name}</h1>
				{:else}
					<a href="/files/{c.id}" class="text-lg sm:text-xl text-muted hover:text-text px-1.5 py-0.5 rounded-lg hover:bg-surface-2 truncate max-w-48">
						{c.is_root ? rootLabel(c) : c.name}
					</a>
				{/if}
			{/each}
		</nav>
		{#if folder && !writable}
			<span class="inline-flex items-center gap-1.5 text-xs font-medium text-muted bg-surface-2 rounded-full px-3 py-1.5"><Eye size={14} />{t('files.readOnly')}</span>
		{/if}
		{#if writable}
			<div class="hidden sm:flex gap-2">
				<button class="btn-outline" onclick={async () => (await newFolder(id)) && folderChanged(id)}>
					<FolderPlus size={18} />{t('common.newFolder')}
				</button>
			</div>
		{/if}
	</div>

	<FileBrowser
		{items}
		{loading}
		dropTarget={writable ? { id, name: title } : null}
		onchange={() => load(id, true)}
	>
		{#snippet empty()}
			<EmptyState title={t('files.emptyTitle')} body={writable ? t('files.emptyBody') : undefined}>
				{#snippet icon()}<FolderOpen size={36} />{/snippet}
				{#if writable}
					<button class="btn-primary" onclick={() => fileInput.click()}><Upload size={18} />{t('files.uploadFiles')}</button>
					<button class="btn-outline" onclick={async () => (await newFolder(id)) && folderChanged(id)}><FolderPlus size={18} />{t('common.newFolder')}</button>
				{/if}
			</EmptyState>
		{/snippet}
	</FileBrowser>
{/if}
