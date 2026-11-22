<script lang="ts">
	import { fade, fly } from 'svelte/transition';
	import BackupBanner from '$lib/components/BackupBanner.svelte';
	import BottomNav from '$lib/components/BottomNav.svelte';
	import CompressDialog from '$lib/components/CompressDialog.svelte';
	import LinkDialog from '$lib/components/LinkDialog.svelte';
	import MoveDialog from '$lib/components/MoveDialog.svelte';
	import ShareDialog from '$lib/components/ShareDialog.svelte';
	import Sidebar from '$lib/components/Sidebar.svelte';
	import TopBar from '$lib/components/TopBar.svelte';
	import UploadPanel from '$lib/components/UploadPanel.svelte';
	import Viewer from '$lib/components/Viewer.svelte';
	import { api } from '$lib/api';
	import { changes, session } from '$lib/stores.svelte';
	import { ui } from '$lib/ui.svelte';

	let { children } = $props();

	// Keep the storage meter current after uploads, deletes and tools.
	let seen = changes.version;
	$effect(() => {
		const v = changes.version;
		if (v === seen) return;
		seen = v;
		const timer = setTimeout(() => {
			api.me().then((me) => (session.me = me)).catch(() => {});
		}, 800);
		return () => clearTimeout(timer);
	});
</script>

{#if session.me}
	<div class="min-h-dvh lg:flex">
		<aside class="hidden lg:block sticky top-0 h-dvh shrink-0 overflow-y-auto">
			<Sidebar space={ui.space} />
		</aside>

		{#if ui.sidebarOpen}
			<div class="lg:hidden fixed inset-0 z-50">
				<button class="absolute inset-0 bg-[var(--overlay)] cursor-default" aria-label="Close menu" onclick={() => (ui.sidebarOpen = false)} transition:fade={{ duration: 150 }}></button>
				<div class="absolute inset-y-0 left-0 bg-surface shadow-2xl overflow-y-auto pt-[env(safe-area-inset-top)]" transition:fly={{ x: -280, duration: 200 }}>
					<Sidebar space={ui.space} />
				</div>
			</div>
		{/if}

		<div class="flex-1 min-w-0 flex flex-col">
			<TopBar />
			<main class="flex-1 px-4 lg:px-8 pb-24 lg:pb-10 w-full max-w-[1600px]">
				<BackupBanner />
				{@render children()}
			</main>
		</div>
	</div>

	<BottomNav space={ui.space} />
	<UploadPanel />

	{#if ui.viewer}
		<Viewer items={ui.viewer.items} bind:index={ui.viewer.index} onclose={() => (ui.viewer = null)} />
	{/if}
	{#if ui.share}
		<ShareDialog node={ui.share} onclose={() => (ui.share = null)} />
	{/if}
	{#if ui.link}
		<LinkDialog node={ui.link} onclose={() => (ui.link = null)} />
	{/if}
	{#if ui.move}
		<MoveDialog nodes={ui.move} onclose={() => (ui.move = null)} />
	{/if}
	{#if ui.compress}
		<CompressDialog node={ui.compress} onclose={() => (ui.compress = null)} />
	{/if}
{/if}
