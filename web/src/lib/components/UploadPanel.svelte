<script lang="ts">
	import { ChevronDown, ChevronUp, CircleAlert, CircleCheck, RotateCcw, X } from '@lucide/svelte';
	import { fly } from 'svelte/transition';
	import { formatBytes, formatDuration } from '$lib/format';
	import { t } from '$lib/i18n/index.svelte';
	import { cancel, clearFinished, retry, uploadStats, uploads } from '$lib/uploads.svelte';
	import FileIcon from './FileIcon.svelte';

	let collapsed = $state(false);
	let now = $state(Date.now());

	$effect(() => {
		const timer = setInterval(() => (now = Date.now()), 1000);
		return () => clearInterval(timer);
	});

	const stats = $derived.by(() => {
		void now;
		return uploadStats();
	});
	const pctAll = $derived(stats.bytesTotal ? stats.bytesSent / stats.bytesTotal : 1);
	const eta = $derived(stats.speed > 0 ? (stats.bytesTotal - stats.bytesSent) / stats.speed : Infinity);
	const title = $derived(
		stats.active
			? t('upload.progress', { done: stats.done, total: stats.total })
			: stats.failed
				? t('upload.someFailed', { n: stats.failed })
				: stats.done === 1
					? t('upload.oneComplete')
					: t('upload.complete', { n: stats.done })
	);
</script>

{#if uploads.open && uploads.items.length}
	<section
		class="fixed z-40 right-0 sm:right-6 bottom-[calc(4.5rem+env(safe-area-inset-bottom))] lg:bottom-6 w-full sm:w-96
			bg-surface border border-border sm:rounded-2xl shadow-2xl overflow-hidden"
		transition:fly={{ y: 24, duration: 200 }}
		aria-label={t('upload.title')}
	>
		<header class="flex items-center gap-2 pl-4 pr-2 h-14 bg-surface-2">
			<div class="flex-1 min-w-0">
				<div class="text-sm font-semibold truncate">{title}</div>
				{#if stats.active}
					<div class="text-xs text-muted tabular-nums">
						{t('upload.speed', { speed: formatBytes(stats.speed) })}
						{#if isFinite(eta)} · {t('upload.left', { time: formatDuration(eta) })}{/if}
					</div>
				{/if}
			</div>
			<button class="icon-btn" aria-label={collapsed ? 'Expand' : 'Collapse'} onclick={() => (collapsed = !collapsed)}>
				{#if collapsed}<ChevronUp size={18} />{:else}<ChevronDown size={18} />{/if}
			</button>
			{#if !stats.active}
				<button class="icon-btn" aria-label={t('upload.clear')} onclick={clearFinished}><X size={18} /></button>
			{/if}
		</header>
		{#if stats.active}
			<div class="h-1 bg-surface-3"><div class="h-full bg-primary transition-[width] duration-300" style="width:{pctAll * 100}%"></div></div>
		{/if}
		{#if !collapsed}
			<ul class="max-h-72 overflow-y-auto">
				{#each uploads.items as item (item.id)}
					{@const pct = item.size ? item.sent / item.size : 1}
					<li class="flex items-center gap-3 px-4 py-2.5 border-b border-border last:border-0 {item.status === 'cancelled' ? 'opacity-50' : ''}">
						<FileIcon node={{ kind: 'file', mime: null, name: item.name }} size={20} />
						<div class="flex-1 min-w-0">
							<div class="text-sm truncate">{item.name}</div>
							{#if item.status === 'uploading'}
								<div class="mt-1.5 h-1.5 rounded-full bg-surface-3 overflow-hidden">
									<div class="h-full bg-primary rounded-full transition-[width] duration-300" style="width:{pct * 100}%"></div>
								</div>
							{:else if item.status === 'error'}
								<div class="text-xs text-danger truncate" title={item.error}>{item.error ?? t('upload.failed')}</div>
							{:else}
								<div class="text-xs text-muted">{item.status === 'waiting' ? t('upload.waiting') : formatBytes(item.size)}</div>
							{/if}
						</div>
						{#if item.status === 'done'}
							<CircleCheck size={20} class="text-success shrink-0" />
						{:else if item.status === 'error'}
							<CircleAlert size={18} class="text-danger shrink-0" />
							<button class="icon-btn size-8" aria-label={t('upload.retry')} onclick={() => retry(item.id)}><RotateCcw size={16} /></button>
						{:else if item.status === 'uploading' || item.status === 'waiting'}
							<span class="text-xs tabular-nums text-muted w-9 text-right">{Math.round(pct * 100)}%</span>
							<button class="icon-btn size-8" aria-label={t('upload.cancel')} onclick={() => cancel(item.id)}><X size={16} /></button>
						{/if}
					</li>
				{/each}
			</ul>
		{/if}
	</section>
{/if}
