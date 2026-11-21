<script lang="ts">
	import { Clock } from '@lucide/svelte';
	import { api } from '$lib/api';
	import EmptyState from '$lib/components/EmptyState.svelte';
	import ListPage from '$lib/components/ListPage.svelte';
	import { t } from '$lib/i18n/index.svelte';
</script>

<ListPage title={t('nav.recent')} fetch={api.recent} sortable={false}>
	{#snippet empty()}
		<EmptyState title={t('recent.empty')} body={t('home.noRecent')}>
			{#snippet icon()}<Clock size={36} />{/snippet}
		</EmptyState>
	{/snippet}
</ListPage>
