<script lang="ts">
	import { HardDrive, House, Share2, Users } from '@lucide/svelte';
	import { page } from '$app/state';
	import { t } from '$lib/i18n/index.svelte';
	import { session } from '$lib/stores.svelte';
	import NewMenu from './NewMenu.svelte';

	let { space }: { space: string | null } = $props();
	const me = $derived(session.me!);
	const path = $derived(page.url.pathname);

	const tabs = $derived([
		{ href: '/', label: t('nav.home'), icon: House, active: path === '/' },
		{ href: `/files/${me.home_id}`, label: t('nav.files'), icon: HardDrive, active: path.startsWith('/files') && space === 'personal' },
		{ href: `/files/${me.family_id}`, label: t('nav.family'), icon: Users, active: path.startsWith('/files') && space === 'family' },
		{ href: '/shared', label: t('nav.sharedShort'), icon: Share2, active: path === '/shared' || (path.startsWith('/files') && space === 'shared') }
	]);
</script>

<div class="lg:hidden fixed right-4 z-30 bottom-[calc(4.75rem+env(safe-area-inset-bottom))]">
	<NewMenu variant="fab" />
</div>

<nav
	class="lg:hidden fixed bottom-0 inset-x-0 z-30 bg-surface/95 backdrop-blur border-t border-border
		grid grid-cols-4 pb-[env(safe-area-inset-bottom)]"
	aria-label="Main"
>
	{#each tabs as tab (tab.href)}
		<a
			href={tab.href}
			class="flex flex-col items-center justify-center gap-1 h-16 text-[0.7rem] {tab.active ? 'text-primary font-semibold' : 'text-muted'}"
			aria-current={tab.active ? 'page' : undefined}
		>
			<span class="px-4 py-1 rounded-full {tab.active ? 'bg-primary-soft' : ''}"><tab.icon size={21} /></span>
			<span class="truncate max-w-full px-1">{tab.label}</span>
		</a>
	{/each}
</nav>
