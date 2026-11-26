<script lang="ts">
	import { goto } from '$app/navigation';
	import { api } from '$lib/api';
	import { reportError } from '$lib/actions';
	import { ChevronLeft } from '@lucide/svelte';
	import AuthCard from '$lib/components/AuthCard.svelte';
	import LanguagePicker from '$lib/components/LanguagePicker.svelte';
	import { i18n, t } from '$lib/i18n/index.svelte';
	import { adoptPreferences } from '$lib/stores.svelte';

	/** The language comes first, so everything after it is in that language. */
	let step = $state<'language' | 'account'>('language');
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

{#if step === 'language'}
<AuthCard title="Choose your language · भाषा चुनें · भाषा निवडा" subtitle={t('setup.languageHint')} languages={false}>
	<LanguagePicker />
	<button class="btn-primary w-full h-12 text-base mt-5" onclick={() => (step = 'account')}>{t('tour.continue')}</button>
</AuthCard>
{:else}
<AuthCard title={t('setup.title')} subtitle={t('setup.subtitle')}>
	<button type="button" class="inline-flex items-center gap-1 text-sm text-primary font-medium -mt-2 mb-4" onclick={() => (step = 'language')}>
		<ChevronLeft size={16} />{t('setup.changeLanguage')}
	</button>
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
{/if}
