<script lang="ts">
	import { ChevronLeft, ChevronRight, Download, ExternalLink, Link2, Minimize2, Share2, X } from '@lucide/svelte';
	import { fade } from 'svelte/transition';
	import { contentUrl } from '$lib/api';
	import { canCompress, canLink, canShare, download } from '$lib/actions';
	import { category, formatBytes, formatDateTime, previewable } from '$lib/format';
	import { t } from '$lib/i18n/index.svelte';
	import type { FileNode } from '$lib/types';
	import { ui } from '$lib/ui.svelte';
	import FileIcon from './FileIcon.svelte';

	let { items, index = $bindable(), onclose }: { items: FileNode[]; index: number; onclose: () => void } = $props();

	const node = $derived(items[index]);
	const kind = $derived(category(node));
	let text = $state<string | null>(null);
	let touchX = 0;

	$effect(() => {
		text = null;
		if (kind === 'text' && node.size < 2_000_000) {
			const id = node.id;
			fetch(contentUrl(id))
				.then((r) => r.text())
				.then((body) => {
					if (items[index]?.id === id) text = body;
				})
				.catch(() => {});
		}
	});

	function go(step: number) {
		const next = index + step;
		if (next >= 0 && next < items.length) index = next;
	}

	function onkeydown(e: KeyboardEvent) {
		if (ui.share || ui.compress || ui.link) return;
		if (e.key === 'Escape') onclose();
		else if (e.key === 'ArrowLeft') go(-1);
		else if (e.key === 'ArrowRight') go(1);
	}
</script>

<svelte:window {onkeydown} />

<div class="fixed inset-0 z-50 bg-black/95 text-white flex flex-col" transition:fade={{ duration: 150 }} role="dialog" aria-modal="true" aria-label={node.name}>
	<header class="flex items-center gap-2 px-3 sm:px-4 h-14 shrink-0 pt-[env(safe-area-inset-top)]">
		<button class="size-10 rounded-full hover:bg-white/10 flex items-center justify-center" aria-label={t('common.close')} onclick={onclose}>
			<X size={22} />
		</button>
		<FileIcon {node} size={20} />
		<div class="flex-1 min-w-0">
			<div class="truncate font-medium">{node.name}</div>
			<div class="truncate text-xs text-white/60">{formatBytes(node.size)} · {formatDateTime(node.modified_at)}</div>
		</div>
		{#if canCompress(node)}
			<button class="size-10 rounded-full hover:bg-white/10 flex items-center justify-center" title={t('tools.compress')} aria-label={t('tools.compress')} onclick={() => (ui.compress = node)}>
				<Minimize2 size={19} />
			</button>
		{/if}
		{#if canShare(node)}
			<button class="size-10 rounded-full hover:bg-white/10 flex items-center justify-center" title={t('common.share')} aria-label={t('common.share')} onclick={() => (ui.share = node)}>
				<Share2 size={19} />
			</button>
		{/if}
		{#if canLink(node)}
			<button class="size-10 rounded-full hover:bg-white/10 flex items-center justify-center" title={t('links.getLink')} aria-label={t('links.getLink')} onclick={() => (ui.link = node)}>
				<Link2 size={19} />
			</button>
		{/if}
		<a class="size-10 rounded-full hover:bg-white/10 hidden sm:flex items-center justify-center" href={contentUrl(node.id)} target="_blank" rel="noopener" title={t('viewer.openTab')} aria-label={t('viewer.openTab')}>
			<ExternalLink size={19} />
		</a>
		<button class="size-10 rounded-full hover:bg-white/10 flex items-center justify-center" title={t('common.download')} aria-label={t('common.download')} onclick={() => download([node])}>
			<Download size={19} />
		</button>
	</header>

	<div
		class="relative flex-1 min-h-0 flex items-center justify-center px-2 sm:px-16 pb-4"
		role="presentation"
		ontouchstart={(e) => (touchX = e.touches[0].clientX)}
		ontouchend={(e) => {
			const dx = e.changedTouches[0].clientX - touchX;
			if (Math.abs(dx) > 60 && kind !== 'pdf') go(dx < 0 ? 1 : -1);
		}}
	>
		{#key node.id}
			{#if !previewable(node)}
				<div class="text-center">
					<FileIcon {node} size={64} />
					<p class="mt-4 text-white/70">{t('viewer.noPreview')}</p>
					<button class="btn mt-5 bg-white text-black hover:bg-white/90" onclick={() => download([node])}>
						<Download size={18} />{t('common.download')}
					</button>
				</div>
			{:else if kind === 'image'}
				<img src={contentUrl(node.id)} alt={node.name} class="max-w-full max-h-full object-contain select-none" in:fade={{ duration: 120 }} />
			{:else if kind === 'pdf'}
				<iframe src={contentUrl(node.id)} title={node.name} class="w-full h-full max-w-5xl rounded-lg bg-white"></iframe>
			{:else if kind === 'video'}
				<!-- svelte-ignore a11y_media_has_caption -->
				<video src={contentUrl(node.id)} controls autoplay playsinline class="max-w-full max-h-full rounded-lg"></video>
			{:else if kind === 'audio'}
				<div class="w-full max-w-md text-center">
					<FileIcon {node} size={72} />
					<audio src={contentUrl(node.id)} controls autoplay class="w-full mt-6"></audio>
				</div>
			{:else if kind === 'text'}
				<pre class="w-full h-full max-w-4xl overflow-auto rounded-xl bg-white/5 p-5 text-sm leading-relaxed whitespace-pre-wrap break-words font-mono">{text ?? t('common.loading')}</pre>
			{/if}
		{/key}

		{#if index > 0}
			<button class="absolute left-2 sm:left-4 top-1/2 -translate-y-1/2 size-11 rounded-full bg-white/10 hover:bg-white/20 hidden sm:flex items-center justify-center" aria-label={t('viewer.previous')} onclick={() => go(-1)}>
				<ChevronLeft size={24} />
			</button>
		{/if}
		{#if index < items.length - 1}
			<button class="absolute right-2 sm:right-4 top-1/2 -translate-y-1/2 size-11 rounded-full bg-white/10 hover:bg-white/20 hidden sm:flex items-center justify-center" aria-label={t('viewer.next')} onclick={() => go(1)}>
				<ChevronRight size={24} />
			</button>
		{/if}
	</div>
	{#if items.length > 1}
		<div class="text-center text-xs text-white/50 pb-[max(0.75rem,env(safe-area-inset-bottom))]">{index + 1} / {items.length}</div>
	{/if}
</div>
