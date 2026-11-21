<script lang="ts">
	import { Copy, Download, Eye, KeyRound, Link2, Plus, Timer, Trash } from '@lucide/svelte';
	import { api, linkUrl } from '$lib/api';
	import { canLink, copyText, reportError } from '$lib/actions';
	import { formatDate, relativeTime } from '$lib/format';
	import { t } from '$lib/i18n/index.svelte';
	import { toast } from '$lib/stores.svelte';
	import type { FileNode, ShareLink } from '$lib/types';
	import { confirmDialog } from '$lib/ui.svelte';
	import Modal from './Modal.svelte';

	let { node, onclose }: { node: FileNode; onclose: () => void } = $props();

	let links = $state<ShareLink[]>([]);
	let loading = $state(true);
	let creating = $state(false);
	let showForm = $state(false);

	// New-link form
	let allowDownload = $state(true);
	let password = $state('');
	let expiry = $state<'never' | '1' | '7' | '30' | 'custom'>('never');
	let customDate = $state('');

	const allowed = $derived(canLink(node));
	const today = new Date().toISOString().slice(0, 10);

	$effect(() => {
		if (!allowed) {
			loading = false;
			return;
		}
		api.links(node.id)
			.then((l) => {
				links = l;
				showForm = l.length === 0;
			})
			.catch(reportError)
			.finally(() => (loading = false));
	});

	function expiresAt(): string | null {
		if (expiry === 'never') return null;
		if (expiry === 'custom') {
			if (!customDate) return null;
			// End of the chosen day, local time.
			return new Date(`${customDate}T23:59:59`).toISOString();
		}
		return new Date(Date.now() + Number(expiry) * 86400_000).toISOString();
	}

	async function create(e: SubmitEvent) {
		e.preventDefault();
		creating = true;
		try {
			const link = await api.createLink(node.id, {
				allow_download: allowDownload,
				password: password || undefined,
				expires_at: expiresAt()
			});
			links = [link, ...links];
			showForm = false;
			password = '';
			expiry = 'never';
			if (await copyText(linkUrl(link.token))) toast(t('links.copied'), 'success');
		} catch (err) {
			reportError(err);
		} finally {
			creating = false;
		}
	}

	async function copy(link: ShareLink) {
		if (await copyText(linkUrl(link.token))) toast(t('links.copied'), 'success');
	}

	async function toggleDownload(link: ShareLink) {
		try {
			const updated = await api.updateLink(link.id, { allow_download: !link.allow_download });
			links = links.map((l) => (l.id === link.id ? updated : l));
			toast(t('links.saved'), 'success');
		} catch (e) {
			reportError(e);
		}
	}

	async function removePassword(link: ShareLink) {
		try {
			const updated = await api.updateLink(link.id, { password: '' });
			links = links.map((l) => (l.id === link.id ? updated : l));
			toast(t('links.saved'), 'success');
		} catch (e) {
			reportError(e);
		}
	}

	async function remove(link: ShareLink) {
		const ok = await confirmDialog({ title: t('links.delete'), confirmLabel: t('links.delete'), danger: true });
		if (!ok) return;
		try {
			await api.deleteLink(link.id);
			links = links.filter((l) => l.id !== link.id);
			toast(t('links.deleted'), 'success');
		} catch (e) {
			reportError(e);
		}
	}

	const expiryOptions = $derived([
		{ id: 'never', label: t('links.never') },
		{ id: '1', label: t('links.day1') },
		{ id: '7', label: t('links.day7') },
		{ id: '30', label: t('links.day30') },
		{ id: 'custom', label: t('links.custom') }
	] as const);
</script>

<Modal title={t('links.title', { name: node.name })} {onclose}>
	{#if !allowed}
		<p class="text-muted py-2">{t('links.notAllowed')}</p>
	{:else if loading}
		<div class="py-8 text-center text-muted">{t('common.loading')}</div>
	{:else}
		<p class="text-sm text-muted mb-4 flex gap-2"><Link2 size={16} class="shrink-0 mt-0.5" />{t('links.anyone')}</p>

		{#if links.length}
			<ul class="space-y-3 mb-4">
				{#each links as link (link.id)}
					<li class="rounded-2xl border border-border p-3 {link.expired ? 'opacity-60' : ''}">
						<div class="flex gap-2">
							<input class="input h-10 text-sm font-mono" readonly value={linkUrl(link.token)} onfocus={(e) => (e.currentTarget as HTMLInputElement).select()} aria-label={t('links.copy')} />
							<button class="btn-primary h-10 px-3 shrink-0" onclick={() => copy(link)} disabled={link.expired}>
								<Copy size={16} /><span class="hidden sm:inline">{t('links.copy')}</span>
							</button>
						</div>
						<div class="flex flex-wrap items-center gap-1.5 mt-2.5 text-xs">
							<button
								class="inline-flex items-center gap-1 px-2.5 py-1 rounded-full border transition-colors
									{link.allow_download ? 'border-primary/30 bg-primary-soft text-primary' : 'border-border text-muted hover:bg-surface-2'}"
								onclick={() => toggleDownload(link)}
								title={t('links.allowDownload')}
							>
								{#if link.allow_download}<Download size={13} />{t('links.canDownload')}{:else}<Eye size={13} />{t('links.viewOnly')}{/if}
							</button>
							{#if link.has_password}
								<button class="inline-flex items-center gap-1 px-2.5 py-1 rounded-full border border-border text-muted hover:bg-surface-2" title={t('links.removePassword')} onclick={() => removePassword(link)}>
									<KeyRound size={13} />{t('links.protected')} ×
								</button>
							{/if}
							<span class="inline-flex items-center gap-1 px-2.5 py-1 rounded-full border border-border {link.expired ? 'text-danger border-danger/30' : 'text-muted'}">
								<Timer size={13} />
								{link.expired ? t('links.expired') : link.expires_at ? t('links.expiresOn', { date: formatDate(link.expires_at) }) : t('links.noExpiry')}
							</span>
							<span class="text-muted ml-auto">{t('links.stats', { views: link.view_count, downloads: link.download_count })}</span>
							<button class="icon-btn size-8 hover:text-danger" title={t('links.delete')} aria-label={t('links.delete')} onclick={() => remove(link)}>
								<Trash size={16} />
							</button>
						</div>
						<div class="text-[0.7rem] text-muted mt-1">{relativeTime(link.created_at)}</div>
					</li>
				{/each}
			</ul>
		{/if}

		{#if showForm}
			<form id="new-link" class="rounded-2xl bg-surface-2 p-4 space-y-4" onsubmit={create}>
				<label class="flex items-start gap-3 cursor-pointer">
					<button
						type="button"
						role="switch"
						aria-checked={allowDownload}
						aria-label={t('links.allowDownload')}
						class="mt-0.5 relative inline-flex h-6 w-11 shrink-0 rounded-full transition-colors {allowDownload ? 'bg-primary' : 'bg-surface-3'}"
						onclick={() => (allowDownload = !allowDownload)}
					>
						<span class="absolute top-0.5 left-0.5 size-5 rounded-full bg-white shadow transition-transform {allowDownload ? 'translate-x-5' : ''}"></span>
					</button>
					<span>
						<span class="block text-sm font-medium">{t('links.allowDownload')}</span>
						<span class="block text-xs text-muted">{t('links.allowDownloadHint')}</span>
					</span>
				</label>

				<div>
					<label class="label" for="link-pass">{t('links.password')}</label>
					<input id="link-pass" class="input" type="password" autocomplete="new-password" minlength="4" bind:value={password} />
					<p class="text-xs text-muted mt-1.5">{t('links.passwordHint')}</p>
				</div>

				<div>
					<span class="label">{t('links.expires')}</span>
					<div class="flex flex-wrap gap-2">
						{#each expiryOptions as opt (opt.id)}
							<button
								type="button"
								class="h-9 px-3 rounded-xl text-sm border transition-colors
									{expiry === opt.id ? 'bg-primary text-on-primary border-primary' : 'border-border bg-surface hover:bg-surface-3'}"
								onclick={() => (expiry = opt.id)}>{opt.label}</button
							>
						{/each}
					</div>
					{#if expiry === 'custom'}
						<input class="input mt-2 w-auto" type="date" min={today} bind:value={customDate} required aria-label={t('links.custom')} />
					{/if}
				</div>
			</form>
		{/if}
	{/if}
	{#snippet footer()}
		{#if allowed && !loading}
			{#if showForm}
				{#if links.length}<button class="btn-ghost" onclick={() => (showForm = false)}>{t('common.cancel')}</button>{/if}
				<button class="btn-primary" form="new-link" disabled={creating}><Link2 size={17} />{t('links.create')}</button>
			{:else}
				<button class="btn-outline mr-auto" onclick={() => (showForm = true)}><Plus size={17} />{t('links.new')}</button>
				<button class="btn-primary" onclick={onclose}>{t('common.close')}</button>
			{/if}
		{:else}
			<button class="btn-primary" onclick={onclose}>{t('common.close')}</button>
		{/if}
	{/snippet}
</Modal>
