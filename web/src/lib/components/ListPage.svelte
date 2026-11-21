<script lang="ts">
	// A page that shows a flat list of items (recent, starred, search, shared).
	import type { Snippet } from 'svelte';
	import { reportError } from '$lib/actions';
	import { t } from '$lib/i18n/index.svelte';
	import { changes } from '$lib/stores.svelte';
	import type { FileNode } from '$lib/types';
	import { ui } from '$lib/ui.svelte';
	import FileBrowser from './FileBrowser.svelte';

	let {
		title,
		fetch,
		empty,
		sortable = true,
		showLocation = true
	}: {
		title: string;
		fetch: () => Promise<FileNode[]>;
		empty: Snippet;
		sortable?: boolean;
		showLocation?: boolean;
	} = $props();

	let items = $state<FileNode[]>([]);
	let loading = $state(true);

	async function load() {
		try {
			items = await fetch();
		} catch (e) {
			reportError(e);
		} finally {
			loading = false;
		}
	}

	$effect(() => {
		ui.folder = null;
		ui.space = null;
	});

	$effect(() => {
		void changes.version;
		void fetch;
		load();
	});
</script>

<svelte:head><title>{title} · Fennoly</title></svelte:head>

<h1 class="text-xl sm:text-2xl font-bold tracking-tight pt-3 lg:pt-1">{title}</h1>
<FileBrowser {items} {loading} {sortable} {showLocation} onchange={load} {empty} />
{#if loading && !items.length}<span class="sr-only">{t('common.loading')}</span>{/if}
