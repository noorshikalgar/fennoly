<script lang="ts">
	import { ChevronRight, FolderPlus, House, Users } from '@lucide/svelte';
	import { api } from '$lib/api';
	import { canWrite, newFolder, reportError } from '$lib/actions';
	import { t } from '$lib/i18n/index.svelte';
	import { folderChanged, session, toast } from '$lib/stores.svelte';
	import type { Crumb, FileNode } from '$lib/types';
	import FileIcon from './FileIcon.svelte';
	import Modal from './Modal.svelte';

	let { nodes, onclose }: { nodes: FileNode[]; onclose: () => void } = $props();

	let folderId = $state<string | null>(null);
	let crumbs = $state<Crumb[]>([]);
	let folders = $state<FileNode[]>([]);
	let writable = $state(false);
	let busy = $state(false);

	const moving = $derived(new Set(nodes.map((n) => n.id)));
	const sourceParent = $derived(nodes[0]?.parent_id);

	async function load(id: string) {
		try {
			const [info, kids] = await Promise.all([api.node(id), api.children(id)]);
			folderId = id;
			crumbs = info.crumbs;
			writable = canWrite(info.node);
			folders = kids.filter((k) => k.kind === 'folder' && !moving.has(k.id));
		} catch (e) {
			reportError(e);
		}
	}

	$effect(() => {
		const start = sourceParent ?? session.me?.home_id;
		if (start) load(start);
	});

	function rootLabel(c: Crumb) {
		return c.space === 'family' ? t('space.family') : c.space === 'shared' ? c.name : t('space.myFiles');
	}

	async function create() {
		if (!folderId) return;
		const f = await newFolder(folderId);
		if (f) load(folderId);
	}

	async function moveHere() {
		if (!folderId) return;
		busy = true;
		const target = crumbs.at(-1);
		for (const n of nodes) {
			try {
				await api.move(n.id, folderId);
			} catch (e) {
				reportError(e);
			}
		}
		busy = false;
		if (sourceParent) folderChanged(sourceParent);
		folderChanged(folderId);
		toast(t('files.movedTo', { name: target ? (target.is_root ? rootLabel(target) : target.name) : '' }), 'success');
		onclose();
	}
</script>

<Modal
	title={nodes.length === 1 ? t('files.moveTitle', { name: nodes[0].name }) : t('files.moveMany', { n: nodes.length })}
	{onclose}
>
	<div class="flex gap-2 mb-3">
		<button class="btn-outline h-9 px-3" onclick={() => session.me?.home_id && load(session.me.home_id)}>
			<House size={16} />{t('space.myFiles')}
		</button>
		<button class="btn-outline h-9 px-3" onclick={() => session.me?.family_id && load(session.me.family_id)}>
			<Users size={16} />{t('space.family')}
		</button>
	</div>
	<nav class="flex items-center flex-wrap gap-1 text-sm mb-2 min-h-8" aria-label="breadcrumbs">
		{#each crumbs as c, i (c.id)}
			{#if i > 0}<ChevronRight size={14} class="text-muted" />{/if}
			<button
				class="px-1.5 py-0.5 rounded-lg hover:bg-surface-2 {i === crumbs.length - 1 ? 'font-semibold' : 'text-muted'}"
				onclick={() => load(c.id)}>{c.is_root ? rootLabel(c) : c.name}</button
			>
		{/each}
	</nav>
	<div class="rounded-2xl border border-border h-72 overflow-y-auto">
		{#each folders as f (f.id)}
			<button class="w-full flex items-center gap-3 px-4 h-12 hover:bg-surface-2 text-left border-b border-border last:border-0" onclick={() => load(f.id)}>
				<FileIcon node={f} size={20} />
				<span class="flex-1 truncate">{f.name}</span>
				<ChevronRight size={16} class="text-muted" />
			</button>
		{:else}
			<div class="h-full flex items-center justify-center text-muted text-sm">{t('files.emptyTitle')}</div>
		{/each}
	</div>
	{#snippet footer()}
		{#if writable}
			<button class="btn-ghost mr-auto" onclick={create}><FolderPlus size={17} />{t('common.newFolder')}</button>
		{/if}
		<button class="btn-ghost" onclick={onclose}>{t('common.cancel')}</button>
		<button class="btn-primary" disabled={!writable || busy || folderId === sourceParent} onclick={moveHere}>
			{t('files.moveHere')}
		</button>
	{/snippet}
</Modal>
