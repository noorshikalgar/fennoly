<script lang="ts">
	import { page } from '$app/state';
	import { t } from '$lib/i18n/index.svelte';

	const tabs = $derived([
		{ href: '/shared', label: t('shared.tabWithMe') },
		{ href: '/shared-by-me', label: t('shared.tabByMe') }
	]);
</script>

<div class="inline-flex rounded-2xl bg-surface-2 p-1 mt-3 lg:mt-1" role="tablist">
	{#each tabs as tab (tab.href)}
		{@const active = page.url.pathname === tab.href}
		<a
			href={tab.href}
			role="tab"
			aria-selected={active}
			class="px-4 h-9 inline-flex items-center rounded-xl text-sm font-medium transition-colors
				{active ? 'bg-surface text-text shadow-sm' : 'text-muted hover:text-text'}">{tab.label}</a
		>
	{/each}
</div>
