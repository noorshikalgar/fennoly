<script lang="ts">
	// Everything I have shared: with which people (and their permission) and
	// through which public links, with quick manage and stop-sharing actions.
	import { Copy, Download, Eye, KeyRound, Link2, Share2, Timer, Users, X } from '@lucide/svelte';
	import { api, linkUrl, thumbUrl } from '$lib/api';
	import { canLink, canShare, copyText, openNode, reportError } from '$lib/actions';
	import Avatar from '$lib/components/Avatar.svelte';
	import EmptyState from '$lib/components/EmptyState.svelte';
	import FileIcon from '$lib/components/FileIcon.svelte';
	import SharedTabs from '$lib/components/SharedTabs.svelte';
	import { formatDate, relativeTime } from '$lib/format';
	import { t } from '$lib/i18n/index.svelte';
	import { changes, toast } from '$lib/stores.svelte';
	import type { SharedByMeItem } from '$lib/types';
	import { confirmDialog, ui } from '$lib/ui.svelte';

	let items = $state<SharedByMeItem[]>([]);
	let loading = $state(true);

	async function load() {
		try {
			items = await api.sharedByMe();
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

	// Reload after changes elsewhere and whenever a share or link dialog closes.
	const dialogOpen = $derived(!!ui.share || !!ui.link);
	$effect(() => {
		void changes.version;
		if (!dialogOpen) load();
	});

	async function stop(item: SharedByMeItem) {
		const ok = await confirmDialog({
			title: t('sharedByMe.stop'),
			message: t('sharedByMe.stopConfirm', { name: item.node.name }),
			confirmLabel: t('sharedByMe.stop'),
			danger: true
		});
		if (!ok) return;
		try {
			await api.stopSharing(item.node.id);
			toast(t('sharedByMe.stopped'), 'success');
			load();
		} catch (e) {
			reportError(e);
		}
	}

	async function copy(token: string) {
		if (await copyText(linkUrl(token))) toast(t('links.copied'), 'success');
	}
</script>

<svelte:head><title>{t('nav.sharedByMe')} · Fennoly</title></svelte:head>

<SharedTabs />
<h1 class="text-xl sm:text-2xl font-bold tracking-tight pt-3">{t('nav.sharedByMe')}</h1>

{#if !loading && !items.length}
	<EmptyState title={t('sharedByMe.empty')} body={t('sharedByMe.emptyBody')}>
		{#snippet icon()}<Share2 size={36} />{/snippet}
	</EmptyState>
{:else}
	<ul class="grid grid-cols-1 gap-3 mt-4 pb-24 lg:pb-8">
		{#each items as item (item.node.id)}
			{@const n = item.node}
			<li class="card p-4">
				<div class="flex items-start gap-3">
					<button class="size-12 rounded-xl bg-surface-2 flex items-center justify-center overflow-hidden shrink-0" onclick={() => openNode(n)} aria-label={n.name}>
						{#if n.thumbnail}<img src={thumbUrl(n)} alt="" class="size-full object-cover" />{:else}<FileIcon node={n} size={24} />{/if}
					</button>
					<div class="flex-1 min-w-0">
						<button class="block max-w-full truncate font-semibold text-left hover:underline" onclick={() => openNode(n)}>{n.name}</button>
						<div class="text-xs text-muted truncate">
							{n.space_kind === 'family' ? t('space.family') : t('space.myFiles')}{n.parent_name && !n.parent_is_root ? ` › ${n.parent_name}` : ''}
							· {t('sharedByMe.sharedOn', { when: relativeTime(item.last_shared_at) })}
						</div>
					</div>
					<button class="icon-btn hover:text-danger shrink-0" title={t('sharedByMe.stop')} aria-label={t('sharedByMe.stop')} onclick={() => stop(item)}>
						<X size={18} />
					</button>
				</div>

				{#if item.shares.length}
					<div class="mt-3 pt-3 border-t border-border">
						<div class="flex items-center justify-between mb-2">
							<span class="text-xs font-medium text-muted uppercase tracking-wide flex items-center gap-1.5"><Users size={14} />{t('sharedByMe.people')}</span>
							{#if canShare(n)}<button class="text-xs font-medium text-primary hover:underline" onclick={() => (ui.share = n)}>{t('sharedByMe.manage')}</button>{/if}
						</div>
						<div class="flex flex-wrap gap-2">
							{#each item.shares as s (s.id)}
								<span class="inline-flex items-center gap-2 rounded-full bg-surface-2 pl-1 pr-3 py-1 text-sm">
									<Avatar name={s.display_name} color={s.avatar_color} size={24} />
									<span class="font-medium">{s.display_name}</span>
									<span class="text-xs text-muted">{s.permission === 'editor' ? t('share.canEdit') : t('share.canView')}</span>
								</span>
							{/each}
						</div>
					</div>
				{/if}

				{#if item.links.length}
					<div class="mt-3 pt-3 border-t border-border">
						<div class="flex items-center justify-between mb-2">
							<span class="text-xs font-medium text-muted uppercase tracking-wide flex items-center gap-1.5"><Link2 size={14} />{t('links.active')}</span>
							{#if canLink(n)}<button class="text-xs font-medium text-primary hover:underline" onclick={() => (ui.link = n)}>{t('sharedByMe.manage')}</button>{/if}
						</div>
						<ul class="space-y-2">
							{#each item.links as l (l.id)}
								<li class="flex flex-wrap items-center gap-x-2 gap-y-1.5 text-xs {l.expired ? 'opacity-60' : ''}">
									<span class="inline-flex items-center gap-1 px-2 py-1 rounded-full {l.allow_download ? 'bg-primary-soft text-primary' : 'bg-surface-2 text-muted'}">
										{#if l.allow_download}<Download size={12} />{t('links.canDownload')}{:else}<Eye size={12} />{t('links.viewOnly')}{/if}
									</span>
									{#if l.has_password}<span class="inline-flex items-center gap-1 px-2 py-1 rounded-full bg-surface-2 text-muted"><KeyRound size={12} />{t('links.protected')}</span>{/if}
									<span class="inline-flex items-center gap-1 px-2 py-1 rounded-full bg-surface-2 {l.expired ? 'text-danger' : 'text-muted'}">
										<Timer size={12} />{l.expired ? t('links.expired') : l.expires_at ? t('links.expiresOn', { date: formatDate(l.expires_at) }) : t('links.noExpiry')}
									</span>
									<span class="text-muted">{t('links.stats', { views: l.view_count, downloads: l.download_count })}</span>
									{#if !l.expired}
										<button class="ml-auto inline-flex items-center gap-1 px-2.5 py-1 rounded-full border border-border hover:bg-surface-2 font-medium" onclick={() => copy(l.token)}>
											<Copy size={12} />{t('links.copy')}
										</button>
									{/if}
								</li>
							{/each}
						</ul>
					</div>
				{/if}
			</li>
		{/each}
	</ul>
{/if}
