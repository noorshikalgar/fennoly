<script lang="ts">
	import { ChevronRight, HardDrive, Star, Users } from '@lucide/svelte';
	import { api, thumbUrl } from '$lib/api';
	import { openNode, reportError } from '$lib/actions';
	import FileIcon from '$lib/components/FileIcon.svelte';
	import GettingStarted from '$lib/components/GettingStarted.svelte';
	import { formatBytes, relativeTime } from '$lib/format';
	import { t } from '$lib/i18n/index.svelte';
	import { changes, session } from '$lib/stores.svelte';
	import type { FileNode } from '$lib/types';
	import { ui } from '$lib/ui.svelte';

	let recent = $state<FileNode[]>([]);
	let starred = $state<FileNode[]>([]);
	let loaded = $state(false);

	const me = $derived(session.me!);
	const greeting = $derived.by(() => {
		const h = new Date().getHours();
		const key = h < 12 ? 'home.morning' : h < 17 ? 'home.afternoon' : 'home.evening';
		return t(key, { name: me.display_name.split(' ')[0] });
	});

	async function load() {
		try {
			[recent, starred] = await Promise.all([api.recent(), api.starred()]);
			session.me = await api.me();
		} catch (e) {
			reportError(e);
		} finally {
			loaded = true;
		}
	}

	$effect(() => {
		ui.folder = null;
		ui.space = null;
	});

	$effect(() => {
		void changes.version;
		load();
	});
</script>

<svelte:head><title>{t('nav.home')} · Fennoly</title></svelte:head>

<section class="pt-4 lg:pt-2">
	<h1 class="text-2xl sm:text-3xl font-bold tracking-tight">{greeting}</h1>
	<p class="text-muted mt-1">{t('home.subtitle')}</p>
</section>

{#if me.role === 'admin' && !me.checklist_hidden}<GettingStarted />{/if}

<section class="grid sm:grid-cols-2 gap-3 mt-6">
	<a href="/files/{me.home_id}" class="card p-4 flex items-center gap-4 hover:-translate-y-0.5 transition-transform">
		<span class="size-12 rounded-2xl bg-primary-soft text-primary flex items-center justify-center"><HardDrive size={24} /></span>
		<div class="flex-1 min-w-0">
			<div class="font-semibold">{t('home.openMine')}</div>
			<div class="text-sm text-muted">{t('home.openMineHint')}</div>
		</div>
		<ChevronRight size={20} class="text-muted" />
	</a>
	<a href="/files/{me.family_id}" class="card p-4 flex items-center gap-4 hover:-translate-y-0.5 transition-transform">
		<span class="size-12 rounded-2xl bg-[#fdebd6] text-[#d9620f] flex items-center justify-center"><Users size={24} /></span>
		<div class="flex-1 min-w-0">
			<div class="font-semibold">{t('home.openFamily')}</div>
			<div class="text-sm text-muted">{t('home.openFamilyHint')}</div>
		</div>
		<ChevronRight size={20} class="text-muted" />
	</a>
</section>

{#if starred.length}
	<section class="mt-8">
		<div class="flex items-center justify-between mb-3">
			<h2 class="font-semibold text-lg flex items-center gap-2"><Star size={18} class="text-warning" fill="currentColor" />{t('home.starred')}</h2>
			<a href="/starred" class="text-sm text-primary font-medium">{t('home.seeAll')}</a>
		</div>
		<div class="flex gap-3 overflow-x-auto pb-2 -mx-4 px-4 lg:mx-0 lg:px-0 snap-x">
			{#each starred.slice(0, 12) as node (node.id)}
				<button class="snap-start shrink-0 w-44 card p-3 text-left flex items-center gap-3 hover:bg-surface-2" onclick={() => openNode(node, starred)}>
					<FileIcon {node} size={24} />
					<span class="truncate text-sm font-medium">{node.name}</span>
				</button>
			{/each}
		</div>
	</section>
{/if}

<section class="mt-8">
	<div class="flex items-center justify-between mb-3">
		<h2 class="font-semibold text-lg">{t('home.recent')}</h2>
		{#if recent.length}<a href="/recent" class="text-sm text-primary font-medium">{t('home.seeAll')}</a>{/if}
	</div>
	{#if loaded && !recent.length}
		<div class="card p-8 text-center text-muted">{t('home.noRecent')}</div>
	{:else}
		<div class="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 xl:grid-cols-5 2xl:grid-cols-6 gap-3">
			{#each recent.slice(0, 12) as node (node.id)}
				<button class="card overflow-hidden text-left hover:-translate-y-0.5 transition-transform" onclick={() => openNode(node, recent)}>
					<div class="aspect-[4/3] bg-surface-2 flex items-center justify-center overflow-hidden">
						{#if node.thumbnail}
							<img src={thumbUrl(node)} alt="" loading="lazy" class="size-full object-cover" />
						{:else}
							<FileIcon {node} size={40} />
						{/if}
					</div>
					<div class="p-2.5">
						<div class="truncate text-sm font-medium">{node.name}</div>
						<div class="truncate text-xs text-muted mt-0.5">{formatBytes(node.size)} · {relativeTime(node.modified_at)}</div>
					</div>
				</button>
			{:else}
				{#each Array(6) as _, i (i)}<div class="rounded-2xl bg-surface-2 animate-pulse aspect-[4/3.6]"></div>{/each}
			{/each}
		</div>
	{/if}
</section>
