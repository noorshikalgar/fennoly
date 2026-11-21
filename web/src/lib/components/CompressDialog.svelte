<script lang="ts">
	// Compress with a live size preview: every slider change asks the server
	// for the real resulting size (debounced), then the user keeps a copy or
	// replaces the original.
	import { ArrowRight, LoaderCircle } from '@lucide/svelte';
	import { api } from '$lib/api';
	import { reportError } from '$lib/actions';
	import { formatBytes } from '$lib/format';
	import { t } from '$lib/i18n/index.svelte';
	import { folderChanged, toast } from '$lib/stores.svelte';
	import type { FileNode, ToolPreview } from '$lib/types';
	import Modal from './Modal.svelte';

	let { node, onclose }: { node: FileNode; onclose: () => void } = $props();

	const isPdf = $derived(node.mime === 'application/pdf');
	let available = $state(true);
	let quality = $state(70);
	let maxDim = $state<number | null>(null);
	let strength = $state(50);
	let preview = $state<ToolPreview | null>(null);
	let working = $state(false);
	let saving = $state(false);
	let requestId = 0;

	const DIMENSIONS: (number | null)[] = [null, 3840, 2560, 1920, 1280, 800];

	$effect(() => {
		if (isPdf) api.tools().then((c) => (available = c.pdf)).catch(() => {});
	});

	// Debounced live preview.
	$effect(() => {
		const params = isPdf
			? { node_id: node.id, tool: 'pdf' as const, strength }
			: { node_id: node.id, tool: 'image' as const, quality, max_dimension: maxDim };
		if (!available) return;
		const id = ++requestId;
		working = true;
		const timer = setTimeout(async () => {
			try {
				const result = await api.toolPreview(params);
				if (id === requestId) preview = result;
			} catch (e) {
				if (id === requestId) reportError(e);
			} finally {
				if (id === requestId) working = false;
			}
		}, isPdf ? 450 : 250);
		return () => clearTimeout(timer);
	});

	const saved = $derived(preview ? 1 - preview.result_size / preview.original_size : 0);
	const pct = $derived(new Intl.NumberFormat(undefined, { style: 'percent', maximumFractionDigits: 0 }).format(Math.max(0, saved)));

	async function apply(mode: 'copy' | 'replace') {
		if (!preview) return;
		saving = true;
		try {
			const result = await api.toolApply(node.id, preview.token, mode);
			if (node.parent_id) folderChanged(node.parent_id);
			toast(mode === 'copy' ? t('tools.savedAs', { name: result.name }) : t('tools.replaced'), 'success');
			onclose();
		} catch (e) {
			reportError(e);
		} finally {
			saving = false;
		}
	}

	const strengthLabel = $derived(strength < 35 ? t('tools.light') : strength < 70 ? t('tools.balanced') : t('tools.strong'));
</script>

<Modal title={t('tools.compressTitle', { name: node.name })} {onclose}>
	{#if !available}
		<p class="text-muted py-4">{t('tools.pdfUnavailable')}</p>
	{:else}
		<p class="text-sm text-muted mb-5">{isPdf ? t('tools.pdfHint') : t('tools.imageHint')}</p>

		<!-- Live before/after -->
		<div class="flex items-center gap-3 rounded-2xl bg-surface-2 p-4 mb-6">
			<div class="flex-1 text-center">
				<div class="text-xs text-muted uppercase tracking-wide">{t('tools.before')}</div>
				<div class="text-xl font-semibold mt-1 tabular-nums">{formatBytes(node.size)}</div>
			</div>
			<ArrowRight size={20} class="text-muted shrink-0" />
			<div class="flex-1 text-center">
				<div class="text-xs text-muted uppercase tracking-wide">{t('tools.after')}</div>
				<div class="text-xl font-semibold mt-1 tabular-nums flex items-center justify-center gap-2 {preview && saved > 0 ? 'text-success' : ''}">
					{#if working && !preview}
						<LoaderCircle size={20} class="animate-spin text-muted" />
					{:else if preview}
						<span class:opacity-50={working}>{formatBytes(preview.result_size)}</span>
						{#if working}<LoaderCircle size={16} class="animate-spin text-muted" />{/if}
					{/if}
				</div>
			</div>
		</div>
		<div class="text-center text-sm mb-6 min-h-5" aria-live="polite">
			{#if preview && !working}
				{#if saved > 0.005}
					<span class="inline-flex px-3 py-1 rounded-full bg-success/10 text-success font-medium">{t('tools.saves', { pct })}</span>
				{:else}
					<span class="text-muted">{t('tools.noSaving')}</span>
				{/if}
			{:else if working}
				<span class="text-muted">{t('tools.working')}</span>
			{/if}
		</div>

		{#if isPdf}
			<label class="label flex justify-between" for="strength">
				<span>{t('tools.strength')}</span><span class="text-primary">{strengthLabel}</span>
			</label>
			<input id="strength" type="range" min="0" max="100" step="5" bind:value={strength} class="w-full accent-[var(--primary)]" />
			<div class="flex justify-between text-xs text-muted mt-1">
				<span>{t('tools.light')}</span><span>{t('tools.strong')}</span>
			</div>
		{:else}
			<label class="label flex justify-between" for="quality">
				<span>{t('tools.quality')}</span><span class="text-primary tabular-nums">{quality}%</span>
			</label>
			<input id="quality" type="range" min="10" max="95" step="5" bind:value={quality} class="w-full accent-[var(--primary)]" />
			<div class="mt-5">
				<span class="label">{t('tools.maxSize')}</span>
				<div class="flex flex-wrap gap-2">
					{#each DIMENSIONS as d (d)}
						<button
							class="h-9 px-3 rounded-xl text-sm border transition-colors
								{maxDim === d ? 'bg-primary text-on-primary border-primary' : 'border-border hover:bg-surface-2'}"
							onclick={() => (maxDim = d)}>{d === null ? t('tools.originalSize') : `${d}px`}</button
						>
					{/each}
				</div>
			</div>
		{/if}
	{/if}
	{#snippet footer()}
		<button class="btn-ghost" onclick={onclose}>{t('common.cancel')}</button>
		{#if available}
			<button class="btn-outline" disabled={!preview || working || saving} onclick={() => apply('replace')}>{t('tools.replace')}</button>
			<button class="btn-primary" disabled={!preview || working || saving} onclick={() => apply('copy')}>{t('tools.saveCopy')}</button>
		{/if}
	{/snippet}
</Modal>
