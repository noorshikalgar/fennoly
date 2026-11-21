<script lang="ts">
	import { Check, Monitor, Moon, Smartphone, Sun, Sunrise } from '@lucide/svelte';
	import { api } from '$lib/api';
	import { reportError } from '$lib/actions';
	import Avatar from '$lib/components/Avatar.svelte';
	import { LANGUAGES, setLocale, t } from '$lib/i18n/index.svelte';
	import { applyTheme, session, theme, toast } from '$lib/stores.svelte';
	import type { Locale, ThemeChoice } from '$lib/types';
	import { ui } from '$lib/ui.svelte';

	const me = $derived(session.me!);
	let name = $state(session.me?.display_name ?? '');
	let current = $state('');
	let next = $state('');
	let version = $state('');
	let installEvent = $state<(Event & { prompt: () => Promise<void> }) | null>(null);
	const standalone = typeof window !== 'undefined' && matchMedia('(display-mode: standalone)').matches;
	const ios = typeof navigator !== 'undefined' && /iphone|ipad|ipod/i.test(navigator.userAgent);

	$effect(() => {
		ui.folder = null;
		ui.space = null;
		api.setupStatus().then((s) => (version = s.version)).catch(() => {});
		const onPrompt = (e: Event) => {
			e.preventDefault();
			installEvent = e as typeof installEvent;
		};
		window.addEventListener('beforeinstallprompt', onPrompt);
		return () => window.removeEventListener('beforeinstallprompt', onPrompt);
	});

	const themes: { id: ThemeChoice; label: () => string; icon: typeof Sun; swatch: string }[] = [
		{ id: 'system', label: () => t('settings.themeSystem'), icon: Monitor, swatch: 'linear-gradient(135deg,#f6f6fa 50%,#16161d 50%)' },
		{ id: 'light', label: () => t('settings.themeLight'), icon: Sun, swatch: '#f6f6fa' },
		{ id: 'dark', label: () => t('settings.themeDark'), icon: Moon, swatch: '#16161d' },
		{ id: 'saffron', label: () => t('settings.themeSaffron'), icon: Sunrise, swatch: '#fdebd6' }
	];

	async function save(patch: Parameters<typeof api.updateMe>[0]) {
		try {
			session.me = await api.updateMe(patch);
			toast(t('settings.saved'), 'success');
		} catch (e) {
			reportError(e);
		}
	}

	function pickTheme(id: ThemeChoice) {
		applyTheme(id);
		save({ theme: id });
	}

	function pickLanguage(code: Locale) {
		setLocale(code);
		save({ locale: code });
	}

	async function changePassword(e: SubmitEvent) {
		e.preventDefault();
		try {
			await api.changePassword(current, next);
			current = next = '';
			toast(t('settings.passwordChanged'), 'success');
		} catch (err) {
			reportError(err);
		}
	}
</script>

<svelte:head><title>{t('settings.title')} · Fennoly</title></svelte:head>

<div class="max-w-2xl pt-3 lg:pt-1 space-y-6">
	<h1 class="text-xl sm:text-2xl font-bold tracking-tight">{t('settings.title')}</h1>

	<section class="card p-5 sm:p-6">
		<h2 class="font-semibold mb-4">{t('settings.profile')}</h2>
		<div class="flex items-center gap-4 mb-5">
			<Avatar name={me.display_name} color={me.avatar_color} size={56} />
			<div>
				<div class="font-semibold text-lg">{me.display_name}</div>
				<div class="text-sm text-muted">@{me.username} · {me.role === 'admin' ? t('admin.roleAdmin') : t('admin.roleMember')}</div>
			</div>
		</div>
		<form
			class="flex gap-2 items-end"
			onsubmit={(e) => {
				e.preventDefault();
				save({ display_name: name });
			}}
		>
			<div class="flex-1">
				<label class="label" for="dn">{t('settings.displayName')}</label>
				<input id="dn" class="input" bind:value={name} maxlength="64" required />
			</div>
			<button class="btn-primary h-11" disabled={name.trim() === me.display_name}>{t('common.save')}</button>
		</form>
	</section>

	<section class="card p-5 sm:p-6">
		<h2 class="font-semibold mb-4">{t('settings.language')}</h2>
		<div class="grid grid-cols-3 gap-2">
			{#each LANGUAGES as lang (lang.code)}
				<button
					class="h-14 rounded-2xl border text-[0.95rem] font-medium flex items-center justify-center gap-2 transition-colors
						{me.locale === lang.code ? 'border-primary bg-primary-soft text-primary' : 'border-border hover:bg-surface-2'}"
					aria-pressed={me.locale === lang.code}
					onclick={() => pickLanguage(lang.code)}
				>
					{#if me.locale === lang.code}<Check size={17} />{/if}{lang.label}
				</button>
			{/each}
		</div>
	</section>

	<section class="card p-5 sm:p-6">
		<h2 class="font-semibold mb-4">{t('settings.theme')}</h2>
		<div class="grid grid-cols-2 sm:grid-cols-4 gap-3">
			{#each themes as th (th.id)}
				<button
					class="rounded-2xl border p-3 text-left transition-colors {theme.choice === th.id ? 'border-primary ring-2 ring-primary/25' : 'border-border hover:bg-surface-2'}"
					aria-pressed={theme.choice === th.id}
					onclick={() => pickTheme(th.id)}
				>
					<span class="block h-14 rounded-xl border border-border mb-2.5" style="background:{th.swatch}"></span>
					<span class="flex items-center gap-1.5 text-sm font-medium"><th.icon size={15} />{th.label()}</span>
				</button>
			{/each}
		</div>
	</section>

	<section class="card p-5 sm:p-6">
		<h2 class="font-semibold mb-4">{t('settings.security')}</h2>
		<form class="grid sm:grid-cols-2 gap-3" onsubmit={changePassword}>
			<div>
				<label class="label" for="cur">{t('settings.currentPassword')}</label>
				<input id="cur" class="input" type="password" bind:value={current} autocomplete="current-password" required />
			</div>
			<div>
				<label class="label" for="new">{t('settings.newPassword')}</label>
				<input id="new" class="input" type="password" bind:value={next} autocomplete="new-password" minlength="8" required />
			</div>
			<div class="sm:col-span-2"><button class="btn-outline">{t('settings.changePassword')}</button></div>
		</form>
	</section>

	<section class="card p-5 sm:p-6">
		<h2 class="font-semibold mb-2 flex items-center gap-2"><Smartphone size={18} />{t('settings.install')}</h2>
		{#if standalone}
			<p class="text-sm text-muted">{t('settings.installed')}</p>
		{:else}
			<p class="text-sm text-muted mb-4">{t('settings.installHint')}</p>
			{#if installEvent}
				<button class="btn-primary" onclick={() => installEvent?.prompt()}>{t('settings.installButton')}</button>
			{:else if ios}
				<p class="text-sm">{t('settings.installIos')}</p>
			{/if}
		{/if}
	</section>

	<p class="text-center text-xs text-muted pb-6">{t('settings.version', { v: version })} · AGPL-3.0</p>
</div>
