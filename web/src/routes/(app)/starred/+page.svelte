<script lang="ts">
	import { Star } from '@lucide/svelte';
	import { api } from '$lib/api';
	import EmptyState from '$lib/components/EmptyState.svelte';
	import ListPage from '$lib/components/ListPage.svelte';
	import { t } from '$lib/i18n/index.svelte';
</script>

<ListPage title={t('nav.starred')} fetch={api.starred}>
	{#snippet empty()}
		<EmptyState title={t('starred.empty')} body={t('starred.emptyBody')}>
			{#snippet icon()}<Star size={36} />{/snippet}
		</EmptyState>
	{/snippet}
</ListPage>
