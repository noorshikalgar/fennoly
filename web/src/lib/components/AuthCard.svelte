<script lang="ts">
	import type { Snippet } from 'svelte';
	import { LANGUAGES, i18n, setLocale, t } from '$lib/i18n/index.svelte';

	let {
		title,
		subtitle,
		languages = true,
		children
	}: { title: string; subtitle?: string; /** The small language switch (off where the page asks for it). */ languages?: boolean; children: Snippet } = $props();
</script>

<div class="min-h-dvh flex flex-col bg-bg">
	<div class="absolute inset-x-0 top-0 h-72 bg-gradient-to-b from-primary-soft to-transparent pointer-events-none"></div>
	<div class="relative flex justify-end p-4 gap-1 min-h-17 {languages ? '' : 'invisible'}">
		{#each LANGUAGES as lang (lang.code)}
			<button
				class="h-9 px-3 rounded-xl text-sm {i18n.locale === lang.code ? 'bg-surface shadow-sm font-semibold text-primary' : 'text-muted hover:bg-surface/60'}"
				onclick={() => setLocale(lang.code)}>{lang.label}</button
			>
		{/each}
	</div>
	<main class="relative flex-1 flex items-start sm:items-center justify-center px-4 pb-10">
		<div class="w-full max-w-md">
			<div class="flex flex-col items-center mb-8 text-center">
				<img src="/icons/icon.svg" alt="" class="size-16 mb-4 drop-shadow-lg" />
				<div class="text-sm font-medium text-muted"><span translate="no">Fennoly</span> · {t('app.tagline')}</div>
			</div>
			<div class="card p-6 sm:p-8">
				<h1 class="text-2xl font-bold tracking-tight">{title}</h1>
				{#if subtitle}<p class="text-muted mt-1.5 leading-relaxed">{subtitle}</p>{/if}
				<div class="mt-6">{@render children()}</div>
			</div>
		</div>
	</main>
</div>
