<script lang="ts">
	import { Search, Users } from '@lucide/svelte';
	import { api } from '$lib/api';
	import { reportError } from '$lib/actions';
	import { t } from '$lib/i18n/index.svelte';
	import { session, toast } from '$lib/stores.svelte';
	import type { FileNode, Share, UserSummary } from '$lib/types';
	import Avatar from './Avatar.svelte';
	import Modal from './Modal.svelte';

	let { node, onclose }: { node: FileNode; onclose: () => void } = $props();

	let users = $state<UserSummary[]>([]);
	let shares = $state<Share[]>([]);
	let loading = $state(true);
	let query = $state('');
	let familyItem = $state(false);

	$effect(() => {
		Promise.all([api.users(), api.shares(node.id), api.node(node.id)])
			.then(([u, s, info]) => {
				users = u.filter((x) => x.id !== session.me?.id);
				shares = s;
				familyItem = info.crumbs[0]?.space === 'family';
			})
			.catch(reportError)
			.finally(() => (loading = false));
	});

	const visible = $derived(
		users.filter((u) => {
			const q = query.trim().toLowerCase();
			return !q || u.display_name.toLowerCase().includes(q) || u.username.includes(q);
		})
	);

	function current(userId: string) {
		return shares.find((s) => s.user_id === userId)?.permission ?? 'none';
	}

	async function change(user: UserSummary, value: string) {
		try {
			const existing = shares.find((s) => s.user_id === user.id);
			if (value === 'none') {
				if (existing) {
					await api.removeShare(existing.id);
					shares = shares.filter((s) => s.id !== existing.id);
				}
			} else {
				shares = await api.share(node.id, user.id, value);
			}
			toast(t('share.updated'), 'success');
		} catch (e) {
			reportError(e);
		}
	}
</script>

<Modal title={t('share.title', { name: node.name })} {onclose}>
	{#if loading}
		<div class="py-10 text-center text-muted">{t('common.loading')}</div>
	{:else if familyItem}
		<p class="text-muted leading-relaxed py-2">{t('share.familyNote')}</p>
	{:else if node.permission !== 'owner'}
		<p class="text-muted leading-relaxed py-2">{t('share.notOwner')}</p>
	{:else if !users.length}
		<div class="flex flex-col items-center text-center gap-3 py-8">
			<span class="size-14 rounded-2xl bg-primary-soft text-primary flex items-center justify-center"><Users size={26} /></span>
			<p class="text-muted max-w-xs leading-relaxed">{t('share.noUsers')}</p>
		</div>
	{:else}
		{#if users.length > 5}
			<div class="relative mb-3">
				<Search size={17} class="absolute left-3.5 top-1/2 -translate-y-1/2 text-muted" />
				<input class="input pl-10" placeholder={t('share.search')} bind:value={query} />
			</div>
		{/if}
		<div class="text-xs font-medium text-muted uppercase tracking-wide mb-1">{t('share.people')}</div>
		<ul class="-mx-2">
			{#each visible as user (user.id)}
				<li class="flex items-center gap-3 px-2 py-2.5 rounded-xl hover:bg-surface-2">
					<Avatar name={user.display_name} color={user.avatar_color} size={38} />
					<div class="flex-1 min-w-0">
						<div class="font-medium truncate">{user.display_name}</div>
						<div class="text-xs text-muted truncate">@{user.username}</div>
					</div>
					<select
						class="h-9 rounded-xl border border-border bg-surface px-2.5 text-sm focus:border-primary focus:outline-none
							{current(user.id) !== 'none' ? 'text-primary font-medium' : 'text-muted'}"
						value={current(user.id)}
						onchange={(e) => change(user, (e.currentTarget as HTMLSelectElement).value)}
						aria-label={user.display_name}
					>
						<option value="none">{t('share.noAccess')}</option>
						<option value="viewer">{t('share.canView')}</option>
						<option value="editor">{t('share.canEdit')}</option>
					</select>
				</li>
			{/each}
		</ul>
	{/if}
	{#snippet footer()}
		<button class="btn-primary" onclick={onclose}>{t('common.close')}</button>
	{/snippet}
</Modal>
