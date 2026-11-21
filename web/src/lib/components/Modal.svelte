<script lang="ts">
	import { X } from '@lucide/svelte';
	import type { Snippet } from 'svelte';
	import { fade, fly } from 'svelte/transition';
	import { t } from '$lib/i18n/index.svelte';

	let {
		title,
		onclose,
		children,
		footer,
		size = 'md'
	}: {
		title: string;
		onclose: () => void;
		children: Snippet;
		footer?: Snippet;
		size?: 'sm' | 'md' | 'lg' | 'full';
	} = $props();

	let panel: HTMLDivElement;

	const widths = { sm: 'sm:max-w-sm', md: 'sm:max-w-lg', lg: 'sm:max-w-2xl', full: 'sm:max-w-5xl' };

	$effect(() => {
		const previous = document.activeElement as HTMLElement | null;
		const first = panel.querySelector<HTMLElement>('[autofocus], input, select, textarea, button:not([data-close])');
		first?.focus();
		const onKey = (e: KeyboardEvent) => {
			if (e.key === 'Escape') {
				e.stopPropagation();
				onclose();
			}
		};
		window.addEventListener('keydown', onKey);
		return () => {
			window.removeEventListener('keydown', onKey);
			previous?.focus?.();
		};
	});
</script>

<div class="fixed inset-0 z-50 flex items-end sm:items-center justify-center sm:p-4" role="presentation">
	<button
		type="button"
		class="absolute inset-0 bg-[var(--overlay)] backdrop-blur-[2px] cursor-default"
		aria-label={t('common.close')}
		onclick={onclose}
		transition:fade={{ duration: 150 }}
	></button>
	<div
		bind:this={panel}
		role="dialog"
		aria-modal="true"
		aria-label={title}
		class="relative w-full {widths[size]} max-h-[92dvh] flex flex-col bg-surface text-text
			rounded-t-3xl sm:rounded-2xl shadow-2xl border border-border"
		transition:fly={{ y: 24, duration: 200 }}
	>
		<div class="flex items-center gap-3 px-5 pt-5 pb-3">
			<h2 class="text-lg font-semibold flex-1 min-w-0 truncate">{title}</h2>
			<button type="button" class="icon-btn -mr-2" data-close aria-label={t('common.close')} onclick={onclose}>
				<X size={20} />
			</button>
		</div>
		<div class="px-5 pb-4 overflow-y-auto">
			{@render children()}
		</div>
		{#if footer}
			<!-- On phones, footer buttons share the row equally for easy tapping. -->
			<div class="flex flex-wrap justify-end gap-2 px-5 py-4 border-t border-border pb-[max(1rem,env(safe-area-inset-bottom))]
				max-sm:[&>*]:flex-1 max-sm:[&>*]:min-w-[8rem]">
				{@render footer()}
			</div>
		{/if}
	</div>
</div>
