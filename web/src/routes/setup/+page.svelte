<script lang="ts">
	import { goto } from '$app/navigation';
	import { api } from '$lib/api';
	import { reportError } from '$lib/actions';
	import AuthCard from '$lib/components/AuthCard.svelte';
	import { i18n, t } from '$lib/i18n/index.svelte';
	import { adoptPreferences } from '$lib/stores.svelte';

	let displayName = $state('');
	let username = $state('');
	let password = $state('');
	let busy = $state(false);
	let touchedUsername = false;

	// Suggest a username from the name until the user edits it.
	$effect(() => {
		if (!touchedUsername) {
			username = displayName
				.toLowerCase()
				.normalize('NFKD')
				.replace(/[^a-z0-9]+/g, '')
				.slice(0, 32);
		}
	});

	async function submit(e: SubmitEvent) {
		e.preventDefault();
		busy = true;
		try {
			const me = await api.setup({ username, display_name: displayName, password, locale: i18n.locale });
			adoptPreferences(me);
			await goto('/', { replace: true });
		} catch (err) {
			reportError(err);
		} finally {
			busy = false;
		}
	}
</script>

<svelte:head><title>{t('setup.title')} · Fennoly</title></svelte:head>

<AuthCard title={t('setup.title')} subtitle={t('setup.subtitle')}>
	<form class="space-y-4" onsubmit={submit}>
		<div>
			<label class="label" for="name">{t('setup.displayName')}</label>
			<input id="name" class="input" bind:value={displayName} autocomplete="name" required maxlength="64" />
		</div>
		<div>
			<label class="label" for="username">{t('setup.username')}</label>
			<input
				id="username"
				class="input"
				bind:value={username}
				oninput={() => (touchedUsername = true)}
				autocomplete="username"
				autocapitalize="none"
				pattern="[a-zA-Z0-9][a-zA-Z0-9._\-]{'{'}1,31{'}'}"
				required
			/>
			<p class="text-xs text-muted mt-1.5">{t('setup.usernameHint')}</p>
		</div>
		<div>
			<label class="label" for="password">{t('setup.password')}</label>
			<input id="password" class="input" type="password" bind:value={password} autocomplete="new-password" minlength="8" required />
			<p class="text-xs text-muted mt-1.5">{t('setup.passwordHint')}</p>
		</div>
		<button class="btn-primary w-full h-12 text-base mt-2" disabled={busy}>{t('setup.submit')}</button>
	</form>
</AuthCard>
