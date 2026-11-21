<script lang="ts">
	import { Search } from '@lucide/svelte';
	import { page } from '$app/state';
	import { api } from '$lib/api';
	import EmptyState from '$lib/components/EmptyState.svelte';
	import ListPage from '$lib/components/ListPage.svelte';
	import { t } from '$lib/i18n/index.svelte';

	const q = $derived(page.url.searchParams.get('q') ?? '');
	const fetch = $derived(() => api.search(q));
</script>

{#key q}
	<ListPage title={q ? t('search.results', { q }) : t('common.search')} {fetch}>
		{#snippet empty()}
			<EmptyState title={q ? t('search.none', { q }) : t('search.hint')}>
				{#snippet icon()}<Search size={36} />{/snippet}
			</EmptyState>
		{/snippet}
	</ListPage>
{/key}
