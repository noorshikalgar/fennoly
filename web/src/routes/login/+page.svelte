<script lang="ts">
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { ApiError, api } from '$lib/api';
	import AuthCard from '$lib/components/AuthCard.svelte';
	import { t } from '$lib/i18n/index.svelte';
	import { adoptPreferences } from '$lib/stores.svelte';

	let username = $state('');
	let password = $state('');
	let busy = $state(false);
	let error = $state('');

	async function submit(e: SubmitEvent) {
		e.preventDefault();
		busy = true;
		error = '';
		try {
			adoptPreferences(await api.login(username, password));
			const next = page.url.searchParams.get('next');
			await goto(next && next.startsWith('/') && !next.startsWith('//') ? next : '/', { replace: true });
		} catch (err) {
			error = err instanceof ApiError ? err.message : t('errors.network');
		} finally {
			busy = false;
		}
	}
</script>

<svelte:head><title>{t('login.title')} · Fennoly</title></svelte:head>

<AuthCard title={t('login.title')}>
	<form class="space-y-4" onsubmit={submit}>
		<div>
			<label class="label" for="username">{t('login.username')}</label>
			<input id="username" class="input" bind:value={username} autocomplete="username" autocapitalize="none" required />
		</div>
		<div>
			<label class="label" for="password">{t('login.password')}</label>
			<input id="password" class="input" type="password" bind:value={password} autocomplete="current-password" required />
		</div>
		{#if error}
			<p class="text-sm text-danger bg-danger-soft rounded-xl px-3.5 py-2.5" role="alert">{error}</p>
		{/if}
		<button class="btn-primary w-full h-12 text-base" disabled={busy}>{t('login.submit')}</button>
		<p class="text-xs text-muted text-center leading-relaxed pt-2">{t('login.forgot')}</p>
	</form>
</AuthCard>
