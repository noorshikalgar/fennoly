<script lang="ts">
	import { LogOut, Menu as MenuIcon, Search, Settings, X } from '@lucide/svelte';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { api } from '$lib/api';
	import { t } from '$lib/i18n/index.svelte';
	import { session } from '$lib/stores.svelte';
	import { ui } from '$lib/ui.svelte';
	import Avatar from './Avatar.svelte';
	import Menu from './Menu.svelte';

	let query = $state(page.url.pathname === '/search' ? (page.url.searchParams.get('q') ?? '') : '');
	let mobileSearch = $state(false);
	let menu = $state<{ x: number; y: number } | null>(null);
	let timer: ReturnType<typeof setTimeout>;

	$effect(() => {
		if (page.url.pathname !== '/search') query = '';
	});

	function search(immediate = false) {
		clearTimeout(timer);
		const run = () => {
			const q = query.trim();
			const onSearch = page.url.pathname === '/search';
			// reset: false keeps the cursor in the search box while results update.
			if (q) goto(`/search?q=${encodeURIComponent(q)}`, { replace: onSearch, reset: false });
			// Clearing the box clears the results too.
			else if (onSearch) goto('/search', { replace: true, reset: false });
		};
		if (immediate) run();
		else timer = setTimeout(run, 300);
	}

	async function signOut() {
		await api.logout().catch(() => {});
		session.me = null;
		goto('/login');
	}
</script>

<header class="sticky top-0 z-30 bg-bg/85 backdrop-blur border-b border-border lg:border-0 pt-[env(safe-area-inset-top)]">
	<div class="flex items-center gap-2 h-16 px-3 lg:px-8">
		<button class="icon-btn lg:hidden" aria-label="Menu" onclick={() => (ui.sidebarOpen = true)}>
			<MenuIcon size={22} />
		</button>
		<a href="/" class="lg:hidden flex items-center gap-2 font-bold text-lg mr-auto {mobileSearch ? 'hidden' : ''}">
			<img src="/icons/icon.svg" alt="" class="size-7" />Fennoly
		</a>

		<form
			class="relative flex-1 max-w-2xl {mobileSearch ? 'flex' : 'hidden lg:flex'}"
			role="search"
			onsubmit={(e) => {
				e.preventDefault();
				search(true);
			}}
		>
			<Search size={18} class="absolute left-4 top-1/2 -translate-y-1/2 text-muted pointer-events-none" />
			<input
				class="w-full h-11 rounded-2xl bg-surface-2 border border-transparent pl-11 pr-10 text-[0.95rem]
					focus:bg-surface focus:border-primary focus:outline-none focus:ring-4 focus:ring-primary/15 transition"
				type="search"
				placeholder={t('common.searchPlaceholder')}
				aria-label={t('common.search')}
				bind:value={query}
				oninput={() => search()}
			/>
			{#if mobileSearch}
				<button type="button" class="icon-btn absolute right-1 top-1/2 -translate-y-1/2" aria-label={t('common.close')} onclick={() => (mobileSearch = false)}>
					<X size={18} />
				</button>
			{/if}
		</form>

		<div class="flex items-center gap-1 lg:ml-auto">
			{#if !mobileSearch}
				<button class="icon-btn lg:hidden" aria-label={t('common.search')} onclick={() => (mobileSearch = true)}>
					<Search size={21} />
				</button>
			{/if}
			{#if session.me}
				<button
					class="rounded-full p-0.5 hover:ring-4 hover:ring-primary/15 transition"
					aria-label={session.me.display_name}
					onclick={(e) => {
						const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
						menu = { x: r.right - 220, y: r.bottom + 8 };
					}}
				>
					<Avatar name={session.me.display_name} color={session.me.avatar_color} size={36} />
				</button>
			{/if}
		</div>
	</div>
</header>

{#if menu}
	<Menu
		x={menu.x}
		y={menu.y}
		onclose={() => (menu = null)}
		items={[
			{ label: t('nav.settings'), icon: Settings, run: () => goto('/settings') },
			{ label: t('common.signOut'), icon: LogOut, run: signOut, separatorBefore: true }
		]}
	/>
{/if}
