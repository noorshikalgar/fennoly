<script lang="ts">
	// Public link page: no account needed. Handles password-protected,
	// expired and deleted links, a single file, or a browsable folder.
	import { ChevronRight, Download, Eye, KeyRound, LinkIcon, Timer, X } from '@lucide/svelte';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { ApiError, api, publicContentUrl, publicThumbUrl, publicZipUrl } from '$lib/api';
	import FileIcon from '$lib/components/FileIcon.svelte';
	import Logo from '$lib/components/Logo.svelte';
	import { category, formatBytes, formatDate, relativeTime } from '$lib/format';
	import { LANGUAGES, i18n, setLocale, t } from '$lib/i18n/index.svelte';
	import type { PublicFolder, PublicInfo, PublicNode } from '$lib/types';

	const token = $derived(page.params.token!);
	const folderParam = $derived(page.url.searchParams.get('f'));

	let phase = $state<'loading' | 'password' | 'expired' | 'missing' | 'ready'>('loading');
	let info = $state<PublicInfo | null>(null);
	let folder = $state<PublicFolder | null>(null);
	let password = $state('');
	let error = $state('');
	let preview = $state<PublicNode | null>(null);
	let text = $state<string | null>(null);

	async function load() {
		error = '';
		try {
			info = await api.publicInfo(token);
			phase = 'ready';
		} catch (e) {
			if (e instanceof ApiError && e.code === 'password_required') phase = 'password';
			else if (e instanceof ApiError && e.code === 'expired') phase = 'expired';
			else phase = 'missing';
		}
	}

	$effect(() => {
		void token;
		load();
	});

	// Load the folder being browsed (the shared folder, or one inside it).
	$effect(() => {
		if (phase !== 'ready' || !info || info.root.kind !== 'folder') return;
		const id = folderParam ?? info.root.id;
		api.publicFolder(token, id)
			.then((f) => (folder = f))
			.catch(() => (phase = 'missing'));
	});

	$effect(() => {
		text = null;
		const node = preview ?? (info?.root.kind === 'file' ? info.root : null);
		if (node && category(node) === 'text' && node.size < 2_000_000) {
			fetch(publicContentUrl(token, node.id))
				.then((r) => r.text())
				.then((body) => (text = body))
				.catch(() => {});
		}
	});

	async function unlock(e: SubmitEvent) {
		e.preventDefault();
		error = '';
		try {
			await api.publicUnlock(token, password);
			password = '';
			await load();
		} catch (err) {
			error = err instanceof ApiError && err.status === 429 ? err.message : t('public.wrongPassword');
		}
	}

	function openFolder(id: string | null) {
		const url = new URL(page.url.href);
		if (id && id !== info?.root.id) url.searchParams.set('f', id);
		else url.searchParams.delete('f');
		goto(url.pathname + url.search, { reset: false });
	}

	function openItem(n: PublicNode) {
		if (n.kind === 'folder') openFolder(n.id);
		else preview = n;
	}

	const files = $derived(folder?.children.filter((c) => c.kind === 'file') ?? []);
	function step(dir: number) {
		if (!preview) return;
		const i = files.findIndex((f) => f.id === preview!.id) + dir;
		if (i >= 0 && i < files.length) preview = files[i];
	}
</script>

<svelte:head>
	<title>{info ? `${info.root.name} · Fennoly` : 'Fennoly'}</title>
	<meta name="robots" content="noindex" />
</svelte:head>

<svelte:window
	onkeydown={(e) => {
		if (!preview) return;
		if (e.key === 'Escape') preview = null;
		else if (e.key === 'ArrowLeft') step(-1);
		else if (e.key === 'ArrowRight') step(1);
	}}
/>

{#snippet previewBody(node: PublicNode)}
	{@const kind = category(node)}
	{#if kind === 'image'}
		<img src={publicContentUrl(token, node.id)} alt={node.name} class="max-w-full max-h-full object-contain mx-auto" />
	{:else if kind === 'pdf'}
		<iframe src={publicContentUrl(token, node.id)} title={node.name} class="w-full h-full rounded-xl bg-white"></iframe>
	{:else if kind === 'video'}
		<!-- svelte-ignore a11y_media_has_caption -->
		<video src={publicContentUrl(token, node.id)} controls playsinline class="max-w-full max-h-full mx-auto rounded-xl"></video>
	{:else if kind === 'audio'}
		<audio src={publicContentUrl(token, node.id)} controls class="w-full max-w-md mx-auto"></audio>
	{:else if kind === 'text'}
		<pre class="w-full h-full overflow-auto rounded-xl bg-surface-2 p-4 text-sm whitespace-pre-wrap break-words font-mono">{text ?? t('common.loading')}</pre>
	{:else}
		<div class="text-center py-10">
			<FileIcon {node} size={64} />
			<p class="text-muted mt-4">{t('viewer.noPreview')}</p>
		</div>
	{/if}
{/snippet}

<div class="min-h-dvh flex flex-col bg-bg">
	<header class="flex items-center gap-2 px-4 sm:px-6 h-16 border-b border-border bg-surface/80 backdrop-blur pt-[env(safe-area-inset-top)]">
		<a href="/" aria-label="Fennoly"><Logo size={28} wordmark /></a>
		<div class="ml-auto flex gap-1">
			{#each LANGUAGES as lang (lang.code)}
				<button
					class="h-8 px-2.5 rounded-lg text-sm {i18n.locale === lang.code ? 'bg-primary-soft text-primary font-semibold' : 'text-muted hover:bg-surface-2'}"
					onclick={() => setLocale(lang.code)}>{lang.label}</button
				>
			{/each}
		</div>
	</header>

	<main class="flex-1 w-full max-w-5xl mx-auto px-4 sm:px-6 py-6 sm:py-10">
		{#if phase === 'loading'}
			<div class="flex justify-center py-24"><Logo size={48} class="animate-pulse" /></div>
		{:else if phase === 'password'}
			<form class="card max-w-sm mx-auto p-6 sm:p-8 mt-6 text-center" onsubmit={unlock}>
				<span class="inline-flex size-14 rounded-2xl bg-primary-soft text-primary items-center justify-center mb-4"><KeyRound size={26} /></span>
				<h1 class="text-xl font-bold">{t('public.passwordTitle')}</h1>
				<p class="text-muted text-sm mt-1.5 mb-5">{t('public.passwordBody')}</p>
				<!-- svelte-ignore a11y_autofocus -->
				<input class="input text-center" type="password" bind:value={password} autocomplete="off" autofocus required aria-label={t('login.password')} />
				{#if error}<p class="text-sm text-danger mt-2" role="alert">{error}</p>{/if}
				<button class="btn-primary w-full h-11 mt-4">{t('public.open')}</button>
			</form>
		{:else if phase === 'expired' || phase === 'missing'}
			<div class="card max-w-md mx-auto p-8 mt-6 text-center">
				<span class="inline-flex size-14 rounded-2xl bg-surface-2 text-muted items-center justify-center mb-4">
					{#if phase === 'expired'}<Timer size={26} />{:else}<LinkIcon size={26} />{/if}
				</span>
				<h1 class="text-xl font-bold">{phase === 'expired' ? t('public.expiredTitle') : t('public.notFoundTitle')}</h1>
				<p class="text-muted text-sm mt-2 leading-relaxed">{t('public.notFoundBody')}</p>
			</div>
		{:else if info}
			<!-- Heading -->
			<div class="flex flex-wrap items-start gap-3 mb-5">
				<div class="flex-1 min-w-0">
					<h1 class="text-xl sm:text-2xl font-bold tracking-tight break-words">{info.root.name}</h1>
					<p class="text-sm text-muted mt-1 flex flex-wrap gap-x-3 gap-y-1">
						{#if info.shared_by}<span>{t('public.sharedBy', { name: info.shared_by })}</span>{/if}
						{#if info.expires_at}<span class="inline-flex items-center gap-1"><Timer size={14} />{t('links.expiresOn', { date: formatDate(info.expires_at) })}</span>{/if}
					</p>
				</div>
				{#if info.allow_download}
					{#if info.root.kind === 'file'}
						<a class="btn-primary" href={publicContentUrl(token, info.root.id, true)}><Download size={18} />{t('common.download')}</a>
					{:else if folder}
						<a class="btn-primary" href={publicZipUrl(token, [folder.node.id])}><Download size={18} />{t('public.downloadAll')}</a>
					{/if}
				{:else}
					<span class="inline-flex items-center gap-1.5 text-xs font-medium text-muted bg-surface-2 rounded-full px-3 py-1.5"><Eye size={14} />{t('public.viewOnlyNote')}</span>
				{/if}
			</div>

			{#if info.root.kind === 'file'}
				<div class="card p-3 sm:p-4 h-[70dvh] flex items-center justify-center overflow-hidden">
					{@render previewBody(info.root)}
				</div>
			{:else if folder}
				{#if folder.crumbs.length > 1}
					<nav class="flex items-center flex-wrap gap-0.5 text-sm mb-3" aria-label="breadcrumbs">
						{#each folder.crumbs as c, i (c.id)}
							{#if i > 0}<ChevronRight size={15} class="text-muted" />{/if}
							<button class="px-1.5 py-0.5 rounded-lg hover:bg-surface-2 {i === folder.crumbs.length - 1 ? 'font-semibold' : 'text-muted'}" onclick={() => openFolder(c.id)}>{c.name}</button>
						{/each}
					</nav>
				{/if}
				<ul class="card overflow-hidden">
					{#each folder.children as n (n.id)}
						<li class="border-b border-border last:border-0">
							<div class="flex items-center gap-3 px-3 sm:px-4 h-16 hover:bg-surface-2">
								<button class="flex-1 min-w-0 flex items-center gap-3 text-left h-full" onclick={() => openItem(n)}>
									<span class="size-10 rounded-xl bg-surface-2 flex items-center justify-center overflow-hidden shrink-0">
										{#if n.thumbnail}<img src={publicThumbUrl(token, n.id)} alt="" loading="lazy" class="size-full object-cover" />{:else}<FileIcon node={n} size={20} />{/if}
									</span>
									<span class="min-w-0">
										<span class="block truncate text-sm font-medium">{n.name}</span>
										<span class="block truncate text-xs text-muted">{n.kind === 'file' ? `${formatBytes(n.size)} · ` : ''}{relativeTime(n.modified_at)}</span>
									</span>
								</button>
								{#if info.allow_download}
									<a class="icon-btn" href={n.kind === 'file' ? publicContentUrl(token, n.id, true) : publicZipUrl(token, [n.id])} aria-label={t('common.download')} title={t('common.download')}><Download size={18} /></a>
								{/if}
							</div>
						</li>
					{:else}
						<li class="p-10 text-center text-muted">{t('files.emptyTitle')}</li>
					{/each}
				</ul>
			{/if}
		{/if}
	</main>

	<footer class="text-center text-xs text-muted py-6 px-4">{t('public.poweredBy')}</footer>
</div>

{#if preview}
	<div class="fixed inset-0 z-50 bg-black/95 text-white flex flex-col" role="dialog" aria-modal="true" aria-label={preview.name}>
		<header class="flex items-center gap-2 px-3 h-14 shrink-0 pt-[env(safe-area-inset-top)]">
			<button class="size-10 rounded-full hover:bg-white/10 flex items-center justify-center" aria-label={t('common.close')} onclick={() => (preview = null)}><X size={22} /></button>
			<div class="flex-1 min-w-0">
				<div class="truncate font-medium">{preview.name}</div>
				<div class="text-xs text-white/60">{formatBytes(preview.size)}</div>
			</div>
			{#if info?.allow_download}
				<a class="size-10 rounded-full hover:bg-white/10 flex items-center justify-center" href={publicContentUrl(token, preview.id, true)} aria-label={t('common.download')}><Download size={19} /></a>
			{/if}
		</header>
		<div class="flex-1 min-h-0 flex items-center justify-center p-2 sm:p-6">
			{@render previewBody(preview)}
		</div>
	</div>
{/if}
