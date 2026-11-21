<script lang="ts">
	import { ArchiveRestore, CircleCheck, CircleX, LoaderCircle, ShieldCheck, UploadCloud } from '@lucide/svelte';
	import { api } from '$lib/api';
	import { reportError } from '$lib/actions';
	import Modal from '$lib/components/Modal.svelte';
	import { formatBytes, formatDateTime, formatDuration } from '$lib/format';
	import { t } from '$lib/i18n/index.svelte';
	import type { BackupRun, BackupTarget } from '$lib/types';

	let { target, onclose }: { target: BackupTarget; onclose: () => void } = $props();

	let runs = $state<BackupRun[] | null>(null);

	async function load() {
		try {
			runs = await api.backupRuns(target.id, 50);
		} catch (e) {
			reportError(e);
			runs = [];
		}
	}

	$effect(() => {
		load();
		const timer = setInterval(() => {
			if (runs?.some((r) => r.status === 'running')) load();
		}, 3000);
		return () => clearInterval(timer);
	});

	const kindIcon = { backup: UploadCloud, check: ShieldCheck, restore: ArchiveRestore };
	const kindLabel = (r: BackupRun) =>
		r.kind === 'backup' ? t('backup.runBackup') : r.kind === 'check' ? t('backup.runCheck') : t('backup.runRestore');
	const took = (r: BackupRun) =>
		r.finished_at ? formatDuration((Date.parse(r.finished_at) - Date.parse(r.started_at)) / 1000) : '';
</script>

<Modal title={t('backup.historyTitle', { name: target.name })} size="lg" {onclose}>
	{#if runs === null}
		<p class="text-sm text-muted py-6 text-center">{t('common.loading')}</p>
	{:else if runs.length === 0}
		<p class="text-sm text-muted py-6 text-center">{t('backup.noRuns')}</p>
	{:else}
		<ul class="divide-y divide-border">
			{#each runs as r (r.id)}
				{@const Icon = kindIcon[r.kind]}
				<li class="py-3 flex gap-3">
					<span class="size-9 rounded-xl bg-surface-2 grid place-items-center shrink-0 text-muted"><Icon size={18} /></span>
					<div class="min-w-0 flex-1">
						<div class="flex items-center gap-2 flex-wrap">
							<span class="font-medium text-sm">{kindLabel(r)}</span>
							{#if r.status === 'success'}
								<span class="inline-flex items-center gap-1 text-xs text-success"><CircleCheck size={14} />{t('backup.ok')}</span>
							{:else if r.status === 'failed'}
								<span class="inline-flex items-center gap-1 text-xs text-danger"><CircleX size={14} />{t('backup.failed')}</span>
							{:else}
								<span class="inline-flex items-center gap-1 text-xs text-primary"><LoaderCircle size={14} class="animate-spin" />{t('backup.inProgress')}</span>
							{/if}
							{#if r.trigger === 'schedule'}<span class="text-xs text-muted">· {t('backup.scheduled')}</span>{/if}
						</div>
						<div class="text-xs text-muted mt-0.5 tabular-nums">
							{formatDateTime(r.started_at)}{#if took(r)} · {took(r)}{/if}
						</div>
						{#if r.status === 'success' && r.kind === 'backup' && r.total_files !== null}
							<div class="text-xs text-muted mt-1">
								{t('backup.runStats', {
									files: (r.total_files ?? 0).toLocaleString(),
									size: formatBytes(r.total_bytes ?? 0),
									added: formatBytes(r.bytes_added ?? 0)
								})}
							</div>
						{/if}
						{#if r.error}<p class="text-xs text-danger mt-1 break-words">{r.error}</p>{/if}
					</div>
				</li>
			{/each}
		</ul>
	{/if}
</Modal>
