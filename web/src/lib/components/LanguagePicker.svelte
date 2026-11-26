<script lang="ts">
	// The first question, before anything else is explained: which language.
	// Each choice greets in its own language, so it can be read whatever the
	// screen is showing now.
	import { Check } from '@lucide/svelte';
	import { LANGUAGES, i18n, setLocale } from '$lib/i18n/index.svelte';
	import type { Locale } from '$lib/types';

	let { onpick }: { onpick?: (locale: Locale) => void } = $props();
</script>

<div role="radiogroup" aria-label="Language · भाषा" class="grid gap-3">
	{#each LANGUAGES as lang (lang.code)}
		{@const on = i18n.locale === lang.code}
		<button
			type="button"
			role="radio"
			aria-checked={on}
			lang={lang.code}
			class="flex items-center gap-4 rounded-2xl border-2 px-4 py-3.5 text-left transition-colors {on ? 'border-primary bg-primary-soft' : 'border-border hover:bg-surface-2'}"
			onclick={() => {
				setLocale(lang.code);
				onpick?.(lang.code);
			}}
		>
			<span class="flex-1 min-w-0">
				<span class="block text-lg font-semibold">{lang.label}</span>
				<span class="block text-sm text-muted">{lang.hello}</span>
			</span>
			<span class="size-6 rounded-full border-2 grid place-items-center shrink-0 {on ? 'bg-primary border-primary text-on-primary' : 'border-border'}">
				{#if on}<Check size={14} strokeWidth={3} />{/if}
			</span>
		</button>
	{/each}
</div>
