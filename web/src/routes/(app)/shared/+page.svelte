<script lang="ts">
	import { Share2 } from '@lucide/svelte';
	import { api } from '$lib/api';
	import EmptyState from '$lib/components/EmptyState.svelte';
	import ListPage from '$lib/components/ListPage.svelte';
	import { t } from '$lib/i18n/index.svelte';
	import type { FileNode } from '$lib/types';

	// Show who shared each item in the "location" line.
	const fetchShared = async (): Promise<FileNode[]> =>
		(await api.sharedWithMe()).map((s) => ({ ...s, parent_name: null }));
</script>

<ListPage title={t('nav.shared')} fetch={fetchShared} showLocation={false}>
	{#snippet empty()}
		<EmptyState title={t('shared.empty')} body={t('shared.emptyBody')}>
			{#snippet icon()}<Share2 size={36} />{/snippet}
		</EmptyState>
	{/snippet}
</ListPage>
