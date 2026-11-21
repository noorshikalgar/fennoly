<script lang="ts">
	import { DatabaseBackup, TriangleAlert, X } from '@lucide/svelte';
	import { page } from '$app/state';
	import { t } from '$lib/i18n/index.svelte';
	import { backupStatus, session } from '$lib/stores.svelte';

	const NUDGE_KEY = 'fennoly-backup-nudge';
	const NUDGE_AGAIN_DAYS = 30;

	let nudgeHidden = $state(readNudge());

	function readNudge(): boolean {
		try {
			const at = Number(localStorage.getItem(NUDGE_KEY));
			return at > 0 && Date.now() - at < NUDGE_AGAIN_DAYS * 86400_000;
		} catch {
			return false;
		}
	}

	function hideNudge() {
		nudgeHidden = true;
		try {
			localStorage.setItem(NUDGE_KEY, String(Date.now()));
		} catch {
			/* storage unavailable: hidden for this visit */
		}
	}

	$effect(() => {
		if (session.me?.role !== 'admin') return;
		backupStatus.refresh();
		const timer = setInterval(() => backupStatus.refresh(), 10 * 60_000);
		return () => clearInterval(timer);
	});

	const s = $derived(backupStatus.value);
	const onBackupsPage = $derived(page.url.pathname.startsWith('/admin/backups'));
	const days = $derived(s?.last_success_at ? Math.floor((Date.now() - Date.parse(s.last_success_at)) / 86400_000) : null);
	const kind = $derived.by(() => {
		if (!s || onBackupsPage || session.me?.role !== 'admin') return null;
		if (s.stale) return 'stale';
		if (s.last_failed) return 'failed';
		if (s.targets === 0 && !nudgeHidden) return 'nudge';
		return null;
	});
</script>

{#if kind === 'stale' || kind === 'failed'}
	<div class="mt-3 lg:mt-1 mb-2 rounded-2xl border border-warning/40 bg-surface px-4 py-3 flex items-start sm:items-center gap-3 shadow-card" role="status">
		<TriangleAlert size={20} class="text-warning shrink-0 mt-0.5 sm:mt-0" />
		<p class="text-sm flex-1 min-w-0">
			{#if kind === 'failed'}
				{t('backup.bannerFailed')}
			{:else if days !== null}
				{t('backup.bannerStale', { n: days })}
			{:else}
				{t('backup.bannerNever')}
			{/if}
		</p>
		<a href="/admin/backups" class="btn-outline h-9 shrink-0">{t('backup.bannerOpen')}</a>
	</div>
{:else if kind === 'nudge'}
	<div class="mt-3 lg:mt-1 mb-2 rounded-2xl bg-primary-soft px-4 py-3 flex items-start sm:items-center gap-3" role="status">
		<DatabaseBackup size={20} class="text-primary shrink-0 mt-0.5 sm:mt-0" />
		<p class="text-sm flex-1 min-w-0">{t('backup.bannerNudge')}</p>
		<a href="/admin/backups" class="btn-primary h-9 shrink-0 max-sm:hidden">{t('backup.bannerSetUp')}</a>
		<button class="icon-btn size-8 shrink-0" aria-label={t('common.close')} onclick={hideNudge}><X size={16} /></button>
	</div>
	<a href="/admin/backups" class="sm:hidden btn-primary h-9 w-full mb-2">{t('backup.bannerSetUp')}</a>
{/if}
