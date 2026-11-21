<script lang="ts">
	// The file grid / list used by every folder and list page: selection,
	// context menus, keyboard support and drag-and-drop uploads.
	import {
		ArrowUpDown,
		Download,
		EllipsisVertical,
		FolderInput,
		Minimize2,
		Pencil,
		Share2,
		Star,
		StarOff,
		Trash,
		Users,
		X
	} from '@lucide/svelte';
	import type { Snippet } from 'svelte';
	import { fly } from 'svelte/transition';
	import { thumbUrl } from '$lib/api';
	import { canCompress, canShare, canWrite, download, openNode, rename, toggleStar, trash } from '$lib/actions';
	import { formatBytes, relativeTime } from '$lib/format';
	import { t } from '$lib/i18n/index.svelte';
	import type { FileNode } from '$lib/types';
	import { setView, ui } from '$lib/ui.svelte';
	import { enqueueTree, filesFromDrop } from '$lib/uploads.svelte';
	import FileIcon from './FileIcon.svelte';
	import Menu, { type MenuItem } from './Menu.svelte';

	let {
		items,
		loading = false,
		dropTarget = null,
		showLocation = false,
		sortable = true,
		onchange,
		empty
	}: {
		items: FileNode[];
		loading?: boolean;
		/** Folder that receives dropped files, when the user may write to it. */
		dropTarget?: { id: string; name: string } | null;
		showLocation?: boolean;
		sortable?: boolean;
		onchange: () => void;
		empty?: Snippet;
	} = $props();

	type SortKey = 'name' | 'modified' | 'size';
	let sortKey = $state<SortKey>('name');
	let sortDesc = $state(false);
	let selected = $state<Set<string>>(new Set());
	let anchor = $state<string | null>(null);
	let menu = $state<{ x: number; y: number; items: MenuItem[] } | null>(null);
	let dragging = $state(false);
	let dragDepth = 0;

	const coarse = typeof window !== 'undefined' && matchMedia('(pointer: coarse)').matches;

	const sorted = $derived.by(() => {
		if (!sortable) return items;
		const dir = sortDesc ? -1 : 1;
		const cmp = (a: FileNode, b: FileNode) => {
			if (a.kind !== b.kind) return a.kind === 'folder' ? -1 : 1;
			let r = 0;
			if (sortKey === 'modified') r = Date.parse(a.modified_at) - Date.parse(b.modified_at);
			else if (sortKey === 'size') r = a.size - b.size;
			if (r === 0) r = a.name.localeCompare(b.name, undefined, { numeric: true, sensitivity: 'base' });
			return r * dir;
		};
		return [...items].sort(cmp);
	});
	const folders = $derived(sorted.filter((n) => n.kind === 'folder'));
	const files = $derived(sorted.filter((n) => n.kind === 'file'));
	const selectedNodes = $derived(sorted.filter((n) => selected.has(n.id)));

	// Clear the selection when the list changes underneath it.
	$effect(() => {
		const ids = new Set(items.map((i) => i.id));
		const kept = [...selected].filter((id) => ids.has(id));
		if (kept.length !== selected.size) selected = new Set(kept);
	});

	function select(node: FileNode, e: MouseEvent | KeyboardEvent) {
		const next = new Set(selected);
		if (e.shiftKey && anchor) {
			const a = sorted.findIndex((n) => n.id === anchor);
			const b = sorted.findIndex((n) => n.id === node.id);
			const [from, to] = a < b ? [a, b] : [b, a];
			if (!(e.ctrlKey || e.metaKey)) next.clear();
			for (const n of sorted.slice(from, to + 1)) next.add(n.id);
		} else if (e.ctrlKey || e.metaKey) {
			if (next.has(node.id)) next.delete(node.id);
			else next.add(node.id);
			anchor = node.id;
		} else {
			next.clear();
			next.add(node.id);
			anchor = node.id;
		}
		selected = next;
	}

	function toggle(node: FileNode) {
		const next = new Set(selected);
		if (next.has(node.id)) next.delete(node.id);
		else next.add(node.id);
		selected = next;
	}

	function onItemClick(node: FileNode, e: MouseEvent) {
		if (coarse) {
			if (selected.size) toggle(node);
			else openNode(node, sorted);
		} else {
			select(node, e);
		}
	}

	let pressTimer: ReturnType<typeof setTimeout> | undefined;
	let longPressed = false;
	function onPointerDown(node: FileNode, e: PointerEvent) {
		if (e.pointerType !== 'touch') return;
		longPressed = false;
		pressTimer = setTimeout(() => {
			longPressed = true;
			toggle(node);
			navigator.vibrate?.(15);
		}, 450);
	}
	function cancelPress() {
		clearTimeout(pressTimer);
	}

	function actionsFor(nodes: FileNode[]): MenuItem[] {
		const one = nodes.length === 1 ? nodes[0] : null;
		const writable = nodes.every(canWrite);
		const list: MenuItem[] = [];
		if (one) list.push({ label: t('common.open'), run: () => openNode(one, sorted) });
		list.push({
			label: one?.kind === 'folder' || nodes.length > 1 ? t('files.downloadZip') : t('common.download'),
			icon: Download,
			run: () => download(nodes)
		});
		if (one && canShare(one)) list.push({ label: t('common.share'), icon: Share2, run: () => (ui.share = one) });
		if (one)
			list.push({
				label: one.starred ? t('common.unstar') : t('common.star'),
				icon: one.starred ? StarOff : Star,
				run: () => toggleStar(one)
			});
		if (one && canCompress(one))
			list.push({ label: t('tools.compress'), icon: Minimize2, run: () => (ui.compress = one) });
		if (writable) {
			if (one)
				list.push({
					label: t('common.rename'),
					icon: Pencil,
					separatorBefore: true,
					run: async () => (await rename(one)) && onchange()
				});
			list.push({ label: t('common.move'), icon: FolderInput, separatorBefore: !one, run: () => (ui.move = nodes) });
			list.push({ label: t('common.delete'), icon: Trash, danger: true, run: () => trash(nodes, onchange) });
		}
		return list;
	}

	function openMenu(node: FileNode, x: number, y: number) {
		if (!selected.has(node.id)) {
			selected = new Set([node.id]);
			anchor = node.id;
		}
		const nodes = sorted.filter((n) => selected.has(n.id));
		menu = { x, y, items: actionsFor(nodes) };
	}

	function onContext(node: FileNode, e: MouseEvent) {
		e.preventDefault();
		if (longPressed) return;
		openMenu(node, e.clientX, e.clientY);
	}

	function onMore(node: FileNode, e: MouseEvent) {
		e.stopPropagation();
		const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
		selected = new Set([node.id]);
		openMenu(node, r.left, r.bottom + 4);
	}

	function onKey(e: KeyboardEvent) {
		const target = e.target as HTMLElement;
		if (target.closest('input, textarea, [role=dialog], [role=menu]') || ui.viewer || menu) return;
		if (e.key === 'Escape' && selected.size) selected = new Set();
		else if ((e.key === 'a' || e.key === 'A') && (e.ctrlKey || e.metaKey)) {
			e.preventDefault();
			selected = new Set(sorted.map((n) => n.id));
		} else if (e.key === 'Delete' && selectedNodes.length && selectedNodes.every(canWrite)) {
			trash(selectedNodes, onchange);
		} else if (e.key === 'F2' && selectedNodes.length === 1 && canWrite(selectedNodes[0])) {
			rename(selectedNodes[0]).then((ok) => ok && onchange());
		} else if (e.key === 'Enter' && selectedNodes.length === 1) {
			openNode(selectedNodes[0], sorted);
		}
	}

	// ---- drag and drop upload --------------------------------------------
	function hasFiles(e: DragEvent) {
		return !!e.dataTransfer && Array.from(e.dataTransfer.types).includes('Files');
	}
	function onDragEnter(e: DragEvent) {
		if (!dropTarget || !hasFiles(e)) return;
		e.preventDefault();
		dragDepth++;
		dragging = true;
	}
	function onDragOver(e: DragEvent) {
		if (!dropTarget || !hasFiles(e)) return;
		e.preventDefault();
		e.dataTransfer!.dropEffect = 'copy';
	}
	function onDragLeave() {
		if (!dropTarget) return;
		dragDepth = Math.max(0, dragDepth - 1);
		if (dragDepth === 0) dragging = false;
	}
	async function onDrop(e: DragEvent) {
		if (!dropTarget || !hasFiles(e)) return;
		e.preventDefault();
		dragging = false;
		dragDepth = 0;
		const list = await filesFromDrop(e.dataTransfer!);
		if (list.length) await enqueueTree(list, dropTarget.id);
	}

	function setSort(key: SortKey) {
		if (sortKey === key) sortDesc = !sortDesc;
		else {
			sortKey = key;
			sortDesc = key !== 'name';
		}
	}

	function meta(n: FileNode) {
		const parts = [];
		if (n.kind === 'file') parts.push(formatBytes(n.size));
		parts.push(relativeTime(n.modified_at));
		return parts.join(' · ');
	}
</script>

<svelte:window onkeydown={onKey} />

<!-- Clicking empty space clears the selection; keyboard users use Escape (see onKey). -->
<!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
<div
	class="relative min-h-[60vh] pb-28 lg:pb-8"
	ondragenter={onDragEnter}
	ondragover={onDragOver}
	ondragleave={onDragLeave}
	ondrop={onDrop}
	onclick={(e) => {
		if (e.target === e.currentTarget) selected = new Set();
	}}
>
	<!-- Toolbar: selection actions, or sort + view toggle -->
	<div class="sticky top-0 z-10 -mx-4 px-4 lg:-mx-8 lg:px-8 py-2 bg-bg/90 backdrop-blur flex items-center gap-2 min-h-14">
		{#if selected.size}
			<div class="flex items-center gap-1 flex-1 min-w-0" in:fly={{ y: -6, duration: 120 }}>
				<button class="icon-btn" aria-label={t('common.clearSelection')} onclick={() => (selected = new Set())}>
					<X size={20} />
				</button>
				<span class="font-medium mr-2 whitespace-nowrap">{t('common.selected', { n: selected.size })}</span>
				<button class="icon-btn" title={t('common.download')} aria-label={t('common.download')} onclick={() => download(selectedNodes)}>
					<Download size={19} />
				</button>
				{#if selectedNodes.length === 1 && canShare(selectedNodes[0])}
					<button class="icon-btn" title={t('common.share')} aria-label={t('common.share')} onclick={() => (ui.share = selectedNodes[0])}>
						<Share2 size={19} />
					</button>
				{/if}
				{#if selectedNodes.every(canWrite)}
					<button class="icon-btn" title={t('common.move')} aria-label={t('common.move')} onclick={() => (ui.move = selectedNodes)}>
						<FolderInput size={19} />
					</button>
					<button class="icon-btn hover:text-danger" title={t('common.delete')} aria-label={t('common.delete')} onclick={() => trash(selectedNodes, onchange)}>
						<Trash size={19} />
					</button>
				{/if}
				<button
					class="icon-btn"
					title={t('common.moreActions')}
					aria-label={t('common.moreActions')}
					onclick={(e) => {
						const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
						menu = { x: r.left, y: r.bottom + 4, items: actionsFor(selectedNodes) };
					}}
				>
					<EllipsisVertical size={19} />
				</button>
			</div>
		{:else}
			<div class="flex-1"></div>
			{#if sortable && items.length > 1}
				<button
					class="btn-ghost h-9 px-3 text-muted"
					onclick={(e) => {
						const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
						menu = {
							x: r.left,
							y: r.bottom + 4,
							items: (['name', 'modified', 'size'] as SortKey[]).map((k) => ({
								label:
									(k === 'name' ? t('files.sortName') : k === 'modified' ? t('files.sortModified') : t('files.sortSize')) +
									(sortKey === k ? (sortDesc ? '  ↓' : '  ↑') : ''),
								run: () => setSort(k)
							}))
						};
					}}
				>
					<ArrowUpDown size={16} />
					{sortKey === 'name' ? t('files.sortName') : sortKey === 'modified' ? t('files.sortModified') : t('files.sortSize')}
				</button>
			{/if}
			<div class="flex rounded-xl bg-surface-2 p-0.5">
				<button
					class="h-8 px-2.5 rounded-[10px] text-sm {ui.view === 'grid' ? 'bg-surface shadow-sm text-text' : 'text-muted'}"
					aria-label={t('files.viewGrid')}
					aria-pressed={ui.view === 'grid'}
					onclick={() => setView('grid')}
				>
					<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="3" y="3" width="7" height="7" rx="1.5" /><rect x="14" y="3" width="7" height="7" rx="1.5" /><rect x="3" y="14" width="7" height="7" rx="1.5" /><rect x="14" y="14" width="7" height="7" rx="1.5" /></svg>
				</button>
				<button
					class="h-8 px-2.5 rounded-[10px] text-sm {ui.view === 'list' ? 'bg-surface shadow-sm text-text' : 'text-muted'}"
					aria-label={t('files.viewList')}
					aria-pressed={ui.view === 'list'}
					onclick={() => setView('list')}
				>
					<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M8 6h13M8 12h13M8 18h13M3 6h.01M3 12h.01M3 18h.01" stroke-linecap="round" /></svg>
				</button>
			</div>
		{/if}
	</div>

	{#if loading && !items.length}
		<div class="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 xl:grid-cols-6 gap-3 mt-2" aria-busy="true">
			{#each Array(8) as _, i (i)}
				<div class="rounded-2xl bg-surface-2 animate-pulse aspect-[4/3.6]"></div>
			{/each}
		</div>
	{:else if !items.length}
		{@render empty?.()}
	{:else if ui.view === 'grid'}
		{#if folders.length}
			<div class="grid grid-cols-1 min-[420px]:grid-cols-2 sm:grid-cols-3 md:grid-cols-4 xl:grid-cols-6 gap-3 mt-2">
				{#each folders as node (node.id)}
					{@const isSel = selected.has(node.id)}
					<div
						role="button"
						tabindex="0"
						aria-pressed={isSel}
						class="group flex items-center gap-3 h-14 pl-3.5 pr-1 rounded-2xl border cursor-pointer select-none transition-colors
							{isSel ? 'bg-primary-soft border-primary/40' : 'bg-surface border-border hover:bg-surface-2'}"
						onclick={(e) => onItemClick(node, e)}
						ondblclick={() => openNode(node, sorted)}
						oncontextmenu={(e) => onContext(node, e)}
						onpointerdown={(e) => onPointerDown(node, e)}
						onpointerup={cancelPress}
						onpointerleave={cancelPress}
						onkeydown={(e) => e.key === ' ' && (e.preventDefault(), select(node, e))}
					>
						<FileIcon {node} size={22} />
						<div class="flex-1 min-w-0">
							<div class="truncate font-medium text-[0.92rem]" title={node.name}>{node.name}</div>
							{#if showLocation && node.parent_name}
								<div class="truncate text-xs text-muted">{t('search.in', { folder: node.parent_name })}</div>
							{/if}
						</div>
						{#if node.starred}<Star size={14} class="text-warning shrink-0" fill="currentColor" />{/if}
						{#if node.permission === 'viewer'}<Users size={14} class="text-muted shrink-0" />{/if}
						<button
							class="icon-btn size-8 opacity-100 lg:opacity-0 group-hover:opacity-100 focus:opacity-100"
							aria-label={t('common.moreActions')}
							onclick={(e) => onMore(node, e)}
						>
							<EllipsisVertical size={17} />
						</button>
					</div>
				{/each}
			</div>
		{/if}
		{#if files.length}
			<div class="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 xl:grid-cols-6 gap-3 {folders.length ? 'mt-6' : 'mt-2'}">
				{#each files as node (node.id)}
					{@const isSel = selected.has(node.id)}
					<div
						role="button"
						tabindex="0"
						aria-pressed={isSel}
						class="group relative flex flex-col rounded-2xl border overflow-hidden cursor-pointer select-none transition-all
							{isSel ? 'border-primary ring-2 ring-primary/30 bg-primary-soft' : 'bg-surface border-border hover:shadow-card hover:-translate-y-px'}"
						onclick={(e) => onItemClick(node, e)}
						ondblclick={() => openNode(node, sorted)}
						oncontextmenu={(e) => onContext(node, e)}
						onpointerdown={(e) => onPointerDown(node, e)}
						onpointerup={cancelPress}
						onpointerleave={cancelPress}
						onkeydown={(e) => e.key === ' ' && (e.preventDefault(), select(node, e))}
					>
						<div class="aspect-[4/3] bg-surface-2 flex items-center justify-center overflow-hidden">
							{#if node.thumbnail}
								<img src={thumbUrl(node)} alt="" loading="lazy" draggable="false" class="size-full object-cover" />
							{:else}
								<FileIcon {node} size={44} />
							{/if}
						</div>
						<div class="flex items-start gap-1 p-2.5 pr-1">
							<div class="flex-1 min-w-0">
								<div class="truncate text-sm font-medium" title={node.name}>{node.name}</div>
								<div class="truncate text-xs text-muted mt-0.5">
									{showLocation && node.parent_name ? t('search.in', { folder: node.parent_name }) : meta(node)}
								</div>
							</div>
							<button
								class="icon-btn size-8 -mt-1 shrink-0 opacity-100 lg:opacity-0 group-hover:opacity-100 focus:opacity-100"
								aria-label={t('common.moreActions')}
								onclick={(e) => onMore(node, e)}
							>
								<EllipsisVertical size={17} />
							</button>
						</div>
						{#if node.starred}
							<span class="absolute top-2 right-2 size-6 rounded-full bg-surface/90 flex items-center justify-center shadow">
								<Star size={13} class="text-warning" fill="currentColor" />
							</span>
						{/if}
					</div>
				{/each}
			</div>
		{/if}
	{:else}
		<div class="mt-2 rounded-2xl border border-border bg-surface overflow-hidden" role="table">
			<div class="hidden sm:flex items-center gap-3 px-4 h-10 text-xs font-medium text-muted border-b border-border" role="row">
				<button class="flex-1 text-left hover:text-text" onclick={() => setSort('name')}>{t('common.name')}</button>
				<button class="w-36 text-left hover:text-text" onclick={() => setSort('modified')}>{t('common.modified')}</button>
				<button class="w-24 text-right hover:text-text" onclick={() => setSort('size')}>{t('common.size')}</button>
				<span class="w-8"></span>
			</div>
			{#each sorted as node (node.id)}
				{@const isSel = selected.has(node.id)}
				<div
					role="row"
					tabindex="0"
					aria-selected={isSel}
					class="group flex items-center gap-3 px-4 h-14 border-b border-border last:border-0 cursor-pointer select-none
						{isSel ? 'bg-primary-soft' : 'hover:bg-surface-2'}"
					onclick={(e) => onItemClick(node, e)}
					ondblclick={() => openNode(node, sorted)}
					oncontextmenu={(e) => onContext(node, e)}
					onpointerdown={(e) => onPointerDown(node, e)}
					onpointerup={cancelPress}
					onpointerleave={cancelPress}
					onkeydown={(e) => e.key === ' ' && (e.preventDefault(), select(node, e))}
				>
					<div class="flex-1 min-w-0 flex items-center gap-3">
						<span class="size-9 rounded-lg bg-surface-2 flex items-center justify-center overflow-hidden shrink-0">
							{#if node.thumbnail}
								<img src={thumbUrl(node)} alt="" loading="lazy" draggable="false" class="size-full object-cover" />
							{:else}
								<FileIcon {node} size={20} />
							{/if}
						</span>
						<div class="min-w-0">
							<div class="truncate text-sm font-medium" title={node.name}>{node.name}</div>
							<div class="truncate text-xs text-muted sm:hidden">{meta(node)}</div>
							{#if showLocation && node.parent_name}
								<div class="truncate text-xs text-muted hidden sm:block">{t('search.in', { folder: node.parent_name })}</div>
							{/if}
						</div>
						{#if node.starred}<Star size={13} class="text-warning shrink-0" fill="currentColor" />{/if}
					</div>
					<div class="hidden sm:block w-36 text-sm text-muted truncate">{relativeTime(node.modified_at)}</div>
					<div class="hidden sm:block w-24 text-sm text-muted text-right">{node.kind === 'file' ? formatBytes(node.size) : '—'}</div>
					<button class="icon-btn size-8" aria-label={t('common.moreActions')} onclick={(e) => onMore(node, e)}>
						<EllipsisVertical size={17} />
					</button>
				</div>
			{/each}
		</div>
	{/if}

	{#if dragging && dropTarget}
		<div
			class="pointer-events-none fixed inset-0 z-40 flex items-center justify-center bg-primary/10 backdrop-blur-[1px]"
			transition:fly={{ duration: 100 }}
		>
			<div class="rounded-3xl border-2 border-dashed border-primary bg-surface px-10 py-8 text-center shadow-2xl">
				<div class="text-primary font-semibold text-lg">{t('files.dropHere', { name: dropTarget.name })}</div>
			</div>
		</div>
	{/if}
</div>

{#if menu}
	<Menu x={menu.x} y={menu.y} items={menu.items} onclose={() => (menu = null)} />
{/if}
