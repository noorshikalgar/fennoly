<script lang="ts">
	import { watchInstall } from '$lib/install.svelte';
	import '../app.css';
	import '@fontsource-variable/inter';
	import '@fontsource/noto-sans-devanagari/400.css';
	import '@fontsource/noto-sans-devanagari/500.css';
	import '@fontsource/noto-sans-devanagari/600.css';
	import '@fontsource/noto-sans-devanagari/700.css';
	import { untrack } from 'svelte';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { ApiError, api } from '$lib/api';
	import Dialogs from '$lib/components/Dialogs.svelte';
	import Toasts from '$lib/components/Toasts.svelte';
	import { t } from '$lib/i18n/index.svelte';
	import { adoptPreferences, applyTheme, session, theme } from '$lib/stores.svelte';

	let { children } = $props();
	let ready = $state(false);
	let failed = $state(false);

	const PUBLIC = ['/login', '/setup'];
	/** Public link pages (/s/<token>) work without an account. */
	const isLinkPage = (path: string) => path.startsWith('/s/');

	async function boot() {
		failed = false;
		applyTheme(theme.choice);
		if (isLinkPage(page.url.pathname)) {
			ready = true;
			return;
		}
		try {
			const status = await api.setupStatus();
			if (status.needs_setup) {
				if (page.url.pathname !== '/setup') await goto('/setup', { replace: true });
				ready = true;
				return;
			}
			try {
				adoptPreferences(await api.me());
				if (PUBLIC.includes(page.url.pathname)) await goto('/', { replace: true });
			} catch (e) {
				if (!(e instanceof ApiError && e.status === 401)) throw e;
				session.me = null;
				if (!PUBLIC.includes(page.url.pathname) || page.url.pathname === '/setup') {
					const next = page.url.pathname === '/' ? '' : `?next=${encodeURIComponent(page.url.pathname + page.url.search)}`;
					await goto(`/login${next}`, { replace: true });
				}
			}
			ready = true;
		} catch {
			failed = true;
		}
	}

	// Boot once; untrack so theme or URL changes do not re-run it.
	$effect(() => {
		untrack(boot);
	});

	// Leaving the app area after sign-out.
	$effect(() => {
		const path = page.url.pathname;
		if (ready && !session.me && !PUBLIC.includes(path) && !isLinkPage(path)) goto('/login');
	});

	// The install prompt comes early; catch it for the Install buttons.
	watchInstall();

	// The service worker makes Fennoly installable and opens it without a
	// connection. Only built apps have one; the dev server does not.
	$effect(() => {
		if (!import.meta.env.DEV && 'serviceWorker' in navigator) {
			navigator.serviceWorker.register('/sw.js', { scope: '/' }).catch((e) => console.warn('service worker', e));
		}
	});
</script>

{#if ready}
	{@render children()}
{:else if failed}
	<div class="min-h-dvh flex flex-col items-center justify-center gap-4 p-6 text-center">
		<img src="/icons/icon.svg" alt="" class="size-14" />
		<p class="text-muted">{t('errors.network')}</p>
		<button class="btn-primary" onclick={boot}>{t('common.retry')}</button>
	</div>
{:else}
	<div class="min-h-dvh flex items-center justify-center">
		<img src="/icons/icon.svg" alt="Fennoly" class="size-14 animate-pulse" />
	</div>
{/if}

<Toasts />
<Dialogs />
