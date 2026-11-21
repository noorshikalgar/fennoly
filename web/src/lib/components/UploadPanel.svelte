<script lang="ts">
	import { ChevronDown, ChevronUp, CircleAlert, CircleCheck, History, RotateCcw, WifiOff, X } from '@lucide/svelte';
	import { fly } from 'svelte/transition';
	import { formatBytes, formatDuration } from '$lib/format';
	import { t } from '$lib/i18n/index.svelte';
	import { toast } from '$lib/stores.svelte';
	import {
		cancel,
		clearFinished,
		dismissPending,
		resumable,
		resumePending,
		retry,
		uploadStats,
		uploads,
		type PendingUpload
	} from '$lib/uploads.svelte';
	import FileIcon from './FileIcon.svelte';

	let collapsed = $state(false);
	let now = $state(Date.now());
	let pending = $state<PendingUpload[]>([]);
	let resumeInput: HTMLInputElement;

	$effect(() => {
		const timer = setInterval(() => (now = Date.now()), 1000);
		return () => clearInterval(timer);
	});

	// Unfinished uploads from an earlier visit (refreshed as uploads change).
	$effect(() => {
		void uploads.items.length;
		void uploads.items.map((i) => i.status).join();
		pending = resumable();
	});

	const stats = $derived.by(() => {
		void now;
		return uploadStats();
	});
	const pctAll = $derived(stats.bytesTotal ? stats.bytesSent / stats.bytesTotal : 1);
	const eta = $derived(stats.speed > 1024 ? (stats.bytesTotal - stats.bytesSent) / stats.speed : Infinity);
	const title = $derived(
		stats.active
			? t('upload.progress', { done: stats.done, total: stats.total })
			: stats.failed
				? t('upload.someFailed', { n: stats.failed })
				: stats.done === 1
					? t('upload.oneComplete')
					: t('upload.complete', { n: stats.done })
	);
	const pct = (n: number) => new Intl.NumberFormat(undefined, { style: 'percent', maximumFractionDigits: 0 }).format(n);

	function picked() {
		const files = Array.from(resumeInput.files ?? []);
		resumeInput.value = '';
		if (files.length && resumePending(files) === 0) toast(t('upload.noMatch'), 'error');
	}
</script>

<input bind:this={resumeInput} type="file" multiple hidden onchange={picked} />

{#if uploads.open && uploads.items.length}
	<section
		class="fixed z-40 right-0 sm:right-6 bottom-[calc(4rem+env(safe-area-inset-bottom))] lg:bottom-6 w-full sm:w-96
			bg-surface border-t sm:border border-border sm:rounded-2xl shadow-2xl overflow-hidden"
		transition:fly={{ y: 24, duration: 200 }}
		aria-label={t('upload.title')}
	>
		<header class="flex items-center gap-2 pl-4 pr-2 h-14 bg-surface-2">
			<div class="flex-1 min-w-0">
				<div class="text-sm font-semibold truncate">{title}</div>
				{#if stats.active}
					<div class="text-xs text-muted tabular-nums truncate" aria-live="polite">
						{#if stats.waitingForNetwork && stats.speed < 1024}
							{t('upload.offline')}
						{:else}
							{formatBytes(stats.bytesSent)} / {formatBytes(stats.bytesTotal)}
							{#if stats.speed > 1024} · {t('upload.speed', { speed: formatBytes(stats.speed) })}{/if}
							{#if isFinite(eta)} · {t('upload.left', { time: formatDuration(eta) })}{/if}
						{/if}
					</div>
				{/if}
			</div>
			<button class="icon-btn" aria-label={collapsed ? 'Expand' : 'Collapse'} aria-expanded={!collapsed} onclick={() => (collapsed = !collapsed)}>
				{#if collapsed}<ChevronUp size={18} />{:else}<ChevronDown size={18} />{/if}
			</button>
			{#if !stats.active}
				<button class="icon-btn" aria-label={t('upload.clear')} onclick={clearFinished}><X size={18} /></button>
			{/if}
		</header>
		{#if stats.active}
			<div class="h-1 bg-surface-3" role="progressbar" aria-valuenow={Math.round(pctAll * 100)} aria-valuemin={0} aria-valuemax={100}>
				<div class="h-full transition-[width] duration-300 {stats.waitingForNetwork ? 'bg-warning' : 'bg-primary'}" style="width:{pctAll * 100}%"></div>
			</div>
		{/if}
		{#if stats.waitingForNetwork && !collapsed}
			<p class="flex items-start gap-2 px-4 py-2.5 text-xs bg-warning/10 text-warning border-b border-border">
				<WifiOff size={15} class="shrink-0 mt-px" />{t('upload.paused')}
			</p>
		{/if}
		{#if !collapsed}
			<ul class="max-h-[min(18rem,45dvh)] overflow-y-auto overscroll-contain">
				{#each uploads.items as item (item.id)}
					{@const done = item.size ? item.sent / item.size : item.status === 'done' ? 1 : 0}
					<li class="flex items-center gap-3 px-4 py-2.5 border-b border-border last:border-0 {item.status === 'cancelled' ? 'opacity-50' : ''}">
						<FileIcon node={{ kind: 'file', mime: null, name: item.name }} size={20} />
						<div class="flex-1 min-w-0">
							<div class="text-sm truncate" title={item.name}>{item.name}</div>
							{#if item.status === 'uploading' || item.status === 'retrying' || item.status === 'finishing'}
								<div class="mt-1.5 h-1.5 rounded-full bg-surface-3 overflow-hidden">
									<div class="h-full rounded-full transition-[width] duration-300 {item.status === 'retrying' || item.stalled ? 'bg-warning' : 'bg-primary'}" style="width:{done * 100}%"></div>
								</div>
								<div class="text-[0.7rem] text-muted mt-1 truncate tabular-nums">
									{#if item.status === 'retrying'}
										<span class="text-warning">{item.retryAt && item.retryAt > now ? t('upload.retryingIn', { s: Math.ceil((item.retryAt - now) / 1000) }) : t('upload.offline')}</span>
									{:else if item.status === 'finishing'}
										{t('upload.finishing')}
									{:else if item.stalled}
										<span class="text-warning">{t('upload.stalled')}</span>
									{:else}
										{formatBytes(item.sent)} / {formatBytes(item.size)}{#if item.resumed}{' · '}<span class="text-primary">{t('upload.resumed')}</span>{/if}
									{/if}
								</div>
							{:else if item.status === 'hashing'}
								<div class="text-xs text-muted tabular-nums">{t('upload.preparing', { pct: pct(item.hashed) })}</div>
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
							<button class="icon-btn size-8" aria-label={t('upload.retry')} title={t('upload.retry')} onclick={() => retry(item.id)}><RotateCcw size={16} /></button>
						{:else if item.status === 'retrying'}
							<button class="text-xs font-medium text-primary px-2 py-1 rounded-lg hover:bg-surface-2 shrink-0" onclick={() => retry(item.id)}>{t('upload.tryNow')}</button>
							<button class="icon-btn size-8" aria-label={t('upload.cancel')} onclick={() => cancel(item.id)}><X size={16} /></button>
						{:else if item.status !== 'cancelled'}
							<span class="text-xs tabular-nums text-muted w-10 text-right">{item.status === 'hashing' ? '' : pct(done)}</span>
							<button class="icon-btn size-8" aria-label={t('upload.cancel')} title={t('upload.cancel')} onclick={() => cancel(item.id)}><X size={16} /></button>
						{/if}
					</li>
				{/each}
			</ul>
		{/if}
	</section>
{:else if pending.length}
	<!-- Uploads interrupted by a closed tab or a reload: offer to continue them. -->
	<section
		class="fixed z-40 right-0 sm:right-6 bottom-[calc(4rem+env(safe-area-inset-bottom))] lg:bottom-6 w-full sm:w-96
			bg-surface border-t sm:border border-border sm:rounded-2xl shadow-2xl overflow-hidden"
		transition:fly={{ y: 24, duration: 200 }}
		aria-label={t('upload.unfinished')}
	>
		<div class="flex items-start gap-3 p-4">
			<span class="size-9 rounded-xl bg-warning/15 text-warning flex items-center justify-center shrink-0"><History size={18} /></span>
			<div class="flex-1 min-w-0">
				<div class="text-sm font-semibold">{t('upload.unfinished')}</div>
				<p class="text-xs text-muted mt-0.5">{t('upload.unfinishedHint')}</p>
				<ul class="mt-2 space-y-1">
					{#each pending.slice(0, 4) as p (p.name + p.size + p.parentId)}
						<li class="flex items-center gap-2 text-xs">
							<span class="truncate flex-1" title={p.name}>{p.name}</span>
							<span class="text-muted tabular-nums shrink-0">{formatBytes(p.size)}</span>
							<button class="icon-btn size-6 shrink-0" aria-label={t('upload.dismiss')} onclick={() => { dismissPending(p); pending = resumable(); }}><X size={13} /></button>
						</li>
					{/each}
				</ul>
				<button class="btn-primary h-9 mt-3" onclick={() => resumeInput.click()}>{t('upload.chooseFiles')}</button>
			</div>
		</div>
	</section>
{/if}
