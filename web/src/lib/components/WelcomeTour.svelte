<script lang="ts">
	// The welcome tour, once per person: first the language, then what makes
	// Fennoly tick, in a few short cards. Skipping is always one tap away.
	import { ChevronLeft, CloudUpload, DatabaseBackup, HardDrive, Link, Lock, Share2, Smartphone, Users } from '@lucide/svelte';
	import type { Component } from 'svelte';
	import { api } from '$lib/api';
	import { t, type MessageKey } from '$lib/i18n/index.svelte';
	import { adoptPreferences, session } from '$lib/stores.svelte';
	import type { Locale } from '$lib/types';
	import InstallHelp from './InstallHelp.svelte';
	import LanguagePicker from './LanguagePicker.svelte';
	import Modal from './Modal.svelte';

	let { onclose }: { onclose: () => void } = $props();

	let step = $state(0);
	const first = $derived((session.me?.display_name ?? '').split(' ')[0]);

	interface Card {
		icons: Component[];
		title: MessageKey;
		text: MessageKey;
	}
	const CARDS: Card[] = [
		{ icons: [HardDrive, Lock], title: 'tour.spacesTitle', text: 'tour.spacesText' },
		{ icons: [Users], title: 'tour.familyTitle', text: 'tour.familyText' },
		{ icons: [CloudUpload], title: 'tour.uploadTitle', text: 'tour.uploadText' },
		{ icons: [Share2, Link], title: 'tour.shareTitle', text: 'tour.shareText' },
		{ icons: [DatabaseBackup], title: 'tour.safeTitle', text: 'tour.safeText' },
		{ icons: [Smartphone], title: 'tour.appTitle', text: 'tour.appText' }
	];
	const last = CARDS.length; // step 0 is the language
	const card = $derived(step > 0 ? CARDS[step - 1] : null);

	/** The language is saved to the account at once, so every device follows. */
	function pickLanguage(locale: Locale) {
		api.updateMe({ locale }).then(adoptPreferences).catch(() => {});
	}

	async function finish() {
		onclose();
		try {
			adoptPreferences(await api.updateMe({ tour_seen: true }));
		} catch {
			/* shown again next time; no harm */
		}
	}
</script>

<Modal title={step === 0 ? t('tour.languageTitle') : t('tour.welcome', { name: first })} onclose={finish}>
	<div class="min-h-[18rem]" aria-live="polite">
		{#if step === 0}
			<p class="text-muted mb-4">Choose your language · अपनी भाषा चुनें · आपली भाषा निवडा</p>
			<LanguagePicker onpick={pickLanguage} />
			<p class="text-xs text-muted mt-3">{t('tour.languageLater')}</p>
		{:else if card}
			<div class="flex gap-2 mb-5">
				{#each card.icons as Icon, i (i)}
					<span class="size-14 rounded-2xl bg-primary-soft text-primary grid place-items-center"><Icon size={28} /></span>
				{/each}
			</div>
			<h2 class="text-xl font-bold tracking-tight">{t(card.title)}</h2>
			<p class="text-muted mt-2 leading-relaxed">{t(card.text)}</p>
			{#if step === last}<div class="mt-5"><InstallHelp /></div>{/if}
		{/if}
	</div>
	<div class="flex justify-center gap-1.5 mt-4" aria-hidden="true">
		{#each { length: last + 1 } as _, i (i)}
			<span class="h-1.5 rounded-full transition-all {i === step ? 'w-6 bg-primary' : 'w-1.5 bg-surface-3'}"></span>
		{/each}
	</div>
	{#snippet footer()}
		{#if step > 0}
			<button class="btn-ghost mr-auto" onclick={() => step--}><ChevronLeft size={18} />{t('tour.back')}</button>
		{:else}
			<button class="btn-ghost mr-auto" onclick={finish}>{t('tour.skip')}</button>
		{/if}
		{#if step < last}
			<button class="btn-primary" onclick={() => step++}>{step === 0 ? t('tour.continue') : t('tour.next')}</button>
		{:else}
			<button class="btn-primary" onclick={finish}>{t('tour.start')}</button>
		{/if}
	{/snippet}
</Modal>
