<script lang="ts">
	import { Clock, HardDrive, House, Settings, Share2, ShieldCheck, Star, Trash, Users } from '@lucide/svelte';
	import { page } from '$app/state';
	import { formatBytes } from '$lib/format';
	import { t } from '$lib/i18n/index.svelte';
	import { session } from '$lib/stores.svelte';
	import { ui } from '$lib/ui.svelte';
	import NewMenu from './NewMenu.svelte';

	let { space }: { space: string | null } = $props();

	const me = $derived(session.me!);
	const path = $derived(page.url.pathname);

	const links = $derived([
		{ href: '/', label: t('nav.home'), icon: House, active: path === '/' },
		{ href: `/files/${me.home_id}`, label: t('nav.myFiles'), icon: HardDrive, active: path.startsWith('/files') && space === 'personal' },
		{ href: `/files/${me.family_id}`, label: t('nav.family'), icon: Users, active: path.startsWith('/files') && space === 'family' },
		{ href: '/shared', label: t('nav.shared'), icon: Share2, active: path === '/shared' || (path.startsWith('/files') && space === 'shared') },
		{ href: '/recent', label: t('nav.recent'), icon: Clock, active: path === '/recent' },
		{ href: '/starred', label: t('nav.starred'), icon: Star, active: path === '/starred' },
		{ href: '/trash', label: t('nav.trash'), icon: Trash, active: path === '/trash' }
	]);

	const quotaPct = $derived(me.quota_bytes ? Math.min(1, me.used_bytes / me.quota_bytes) : 0);
</script>

<nav class="flex flex-col h-full w-64 px-3 py-4 gap-1" aria-label="Main">
	<a href="/" class="flex items-center gap-2.5 px-3 mb-4" onclick={() => (ui.sidebarOpen = false)}>
		<img src="/icons/icon.svg" alt="" class="size-8" />
		<span class="text-xl font-bold tracking-tight">Fennoly</span>
	</a>
	<div class="px-1 mb-4"><NewMenu /></div>

	{#each links as link (link.href)}
		<a
			href={link.href}
			onclick={() => (ui.sidebarOpen = false)}
			class="flex items-center gap-3 px-3 h-10 rounded-xl text-[0.93rem] transition-colors
				{link.active ? 'bg-primary-soft text-primary font-semibold' : 'text-text hover:bg-surface-2'}"
			aria-current={link.active ? 'page' : undefined}
		>
			<link.icon size={19} />
			{link.label}
		</a>
	{/each}

	<div class="flex-1"></div>

	{#if me.role === 'admin'}
		<a
			href="/admin"
			onclick={() => (ui.sidebarOpen = false)}
			class="flex items-center gap-3 px-3 h-10 rounded-xl text-[0.93rem] {path === '/admin' ? 'bg-primary-soft text-primary font-semibold' : 'hover:bg-surface-2'}"
		>
			<ShieldCheck size={19} />{t('nav.admin')}
		</a>
	{/if}
	<a
		href="/settings"
		onclick={() => (ui.sidebarOpen = false)}
		class="flex items-center gap-3 px-3 h-10 rounded-xl text-[0.93rem] {path === '/settings' ? 'bg-primary-soft text-primary font-semibold' : 'hover:bg-surface-2'}"
	>
		<Settings size={19} />{t('nav.settings')}
	</a>

	<div class="mx-3 mt-4 pt-4 border-t border-border">
		<div class="text-xs text-muted mb-2">{t('home.storage')}</div>
		{#if me.quota_bytes}
			<div class="h-1.5 rounded-full bg-surface-3 overflow-hidden mb-1.5">
				<div class="h-full rounded-full {quotaPct > 0.9 ? 'bg-danger' : 'bg-primary'}" style="width:{quotaPct * 100}%"></div>
			</div>
			<div class="text-xs text-muted">{t('home.usedOf', { used: formatBytes(me.used_bytes), total: formatBytes(me.quota_bytes) })}</div>
		{:else}
			<div class="text-xs text-muted">{t('home.used', { used: formatBytes(me.used_bytes) })}</div>
		{/if}
	</div>
</nav>
