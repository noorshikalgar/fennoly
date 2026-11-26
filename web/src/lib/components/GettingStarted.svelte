<script lang="ts">
	// The admin's first steps after setting up. Each step ticks itself when
	// the real thing exists; the card can be put away at any time.
	import { Check, ChevronRight, X } from '@lucide/svelte';
	import { api } from '$lib/api';
	import { t, type MessageKey } from '$lib/i18n/index.svelte';
	import { install } from '$lib/install.svelte';
	import { adoptPreferences, changes, session } from '$lib/stores.svelte';
	import type { Progress } from '$lib/types';
	import InstallHelp from './InstallHelp.svelte';

	let progress = $state<Progress | null>(null);
	let showInstall = $state(false);

	$effect(() => {
		void changes.version;
		api.progress().then((p) => (progress = p)).catch(() => {});
	});

	interface Step {
		id: string;
		label: MessageKey;
		hint: MessageKey;
		done: boolean;
		href?: string;
	}
	const steps = $derived.by((): Step[] => {
		const p = progress;
		const family = session.me?.family_id ? `/files/${session.me.family_id}` : '/';
		return [
			{ id: 'family', label: 'start.family', hint: 'start.familyHint', done: (p?.members ?? 0) > 1, href: '/admin' },
			{ id: 'files', label: 'start.files', hint: 'start.filesHint', done: (p?.family_items ?? 0) > 0, href: family },
			{ id: 'share', label: 'start.share', hint: 'start.shareHint', done: (p?.shares ?? 0) > 0, href: family },
			{ id: 'backup', label: 'start.backup', hint: 'start.backupHint', done: (p?.backups ?? 0) > 0, href: '/admin/backups' },
			{ id: 'install', label: 'start.install', hint: 'start.installHint', done: install.installed }
		];
	});
	const doneCount = $derived(steps.filter((s) => s.done).length);

	async function hide() {
		try {
			adoptPreferences(await api.updateMe({ checklist_hidden: true }));
		} catch {
			/* stays for now */
		}
	}
</script>

{#if progress}
	<section class="card overflow-hidden mt-6" aria-labelledby="start-title">
		<div class="flex items-start gap-3 px-5 pt-4 pb-3">
			<div class="flex-1 min-w-0">
				<h2 id="start-title" class="font-semibold">{doneCount === steps.length ? t('start.allDone') : t('start.title')}</h2>
				<p class="text-xs text-muted mt-0.5">{t('start.progress', { done: doneCount, n: steps.length })}</p>
			</div>
			<button class="icon-btn -mr-2 -mt-1" aria-label={t('start.hide')} title={t('start.hide')} onclick={hide}><X size={18} /></button>
		</div>
		<div class="mx-5 h-1.5 rounded-full bg-surface-3 overflow-hidden">
			<div class="h-full rounded-full bg-primary transition-all" style="width:{(doneCount / steps.length) * 100}%"></div>
		</div>
		<ul class="mt-2 divide-y divide-border">
			{#each steps as s (s.id)}
				<li>
					{#snippet body()}
						<span class="size-6 rounded-full border-2 grid place-items-center shrink-0 {s.done ? 'bg-primary border-primary text-on-primary' : 'border-border'}">
							{#if s.done}<Check size={14} strokeWidth={3} />{/if}
						</span>
						<span class="flex-1 min-w-0">
							<span class="block text-sm font-medium {s.done ? 'line-through text-muted' : ''}">{t(s.label)}</span>
							{#if !s.done}<span class="block text-xs text-muted">{t(s.hint)}</span>{/if}
						</span>
						{#if !s.done}<ChevronRight size={16} class="text-muted shrink-0" />{/if}
					{/snippet}
					{#if s.href && !s.done}
						<a class="flex items-center gap-3 px-5 py-2.5 hover:bg-surface-2" href={s.href}>{@render body()}</a>
					{:else}
						<button class="w-full flex items-center gap-3 px-5 py-2.5 text-left hover:bg-surface-2 disabled:hover:bg-transparent" disabled={s.done} onclick={() => (showInstall = !showInstall)}>{@render body()}</button>
					{/if}
					{#if s.id === 'install' && showInstall && !s.done}<div class="px-5 pb-3 pl-14"><InstallHelp /></div>{/if}
				</li>
			{/each}
		</ul>
	</section>
{/if}
