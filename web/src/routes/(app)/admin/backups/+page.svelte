<script lang="ts">
	import {
		ArchiveRestore,
		CircleCheck,
		CircleX,
		DatabaseBackup,
		EllipsisVertical,
		History,
		KeyRound,
		LoaderCircle,
		Lock,
		LockOpen,
		Pause,
		Pencil,
		Play,
		PlugZap,
		Plus,
		Trash,
		TriangleAlert
	} from '@lucide/svelte';
	import { goto } from '$app/navigation';
	import { api } from '$lib/api';
	import { reportError } from '$lib/actions';
	import EmptyState from '$lib/components/EmptyState.svelte';
	import Menu, { type MenuItem } from '$lib/components/Menu.svelte';
	import HistoryDialog from '$lib/components/backup/HistoryDialog.svelte';
	import RecoveryKeyDialog from '$lib/components/backup/RecoveryKeyDialog.svelte';
	import SnapshotBrowser from '$lib/components/backup/SnapshotBrowser.svelte';
	import TargetDialog, { KIND_ICONS, parseSchedule } from '$lib/components/backup/TargetDialog.svelte';
	import { formatBytes, formatDateTime, relativeTime } from '$lib/format';
	import { t, type MessageKey } from '$lib/i18n/index.svelte';
	import { backupStatus, session, toast } from '$lib/stores.svelte';
	import type { BackupOverview, BackupTarget } from '$lib/types';
	import { confirmDialog, promptDialog, ui } from '$lib/ui.svelte';

	let data = $state<BackupOverview | null>(null);
	let editing = $state<BackupTarget | null | undefined>(undefined); // undefined: closed, null: new
	let keyShown = $state<{ target: BackupTarget; key: string; firstTime: boolean } | null>(null);
	let history = $state<BackupTarget | null>(null);
	let browsing = $state<BackupTarget | null>(null);
	let testing = $state<Record<string, boolean>>({});
	let menu = $state<{ x: number; y: number; items: MenuItem[] } | null>(null);
	/** Runs started from this page, announced when they finish. */
	const watching = new Map<string, { runId: string; kind: 'backup' | 'restore' }>();

	const busy = $derived(!!data?.targets.some((x) => x.running !== null) || Object.values(testing).some(Boolean));

	async function load() {
		try {
			const next = await api.backups();
			data = next;
			// A destination stays busy until its run is recorded, so "not busy"
			// means a watched run has finished (however quickly it went).
			for (const target of next.targets) {
				if (target.running === null && watching.has(target.id)) finished(target);
			}
		} catch (e) {
			reportError(e);
		}
	}

	/** Tells the admin how a run they started ended. */
	async function finished(target: BackupTarget) {
		const w = watching.get(target.id);
		if (!w) return;
		watching.delete(target.id);
		backupStatus.refresh();
		try {
			const run = (await api.backupRuns(target.id, 10)).find((r) => r.id === w.runId);
			if (!run) return;
			if (run.status === 'failed') {
				toast(`${target.name}: ${run.error ?? t('errors.generic')}`, 'error');
			} else if (w.kind === 'restore') {
				toast(t('backup.restoredToast'), 'success', {
					label: t('common.open'),
					run: () => goto(`/files/${session.me?.home_id}`)
				});
			} else {
				toast(t('backup.doneToast', { name: target.name }), 'success');
			}
		} catch {
			/* the card shows the result anyway */
		}
	}

	$effect(() => {
		ui.folder = null;
		ui.space = null;
		if (session.me?.role !== 'admin') goto('/');
		else load();
	});

	// Refresh quickly while something runs, slowly otherwise.
	$effect(() => {
		if (!data) return;
		const timer = setInterval(load, busy ? 2500 : 30000);
		return () => clearInterval(timer);
	});

	function saved(target: BackupTarget, key: string | null) {
		const isNew = editing === null;
		editing = undefined;
		load();
		backupStatus.refresh();
		if (key) keyShown = { target, key, firstTime: true };
		toast(isNew ? t('backup.added', { name: target.name }) : t('settings.saved'), 'success');
		// Check a new destination right away, so mistakes show up now and not at 2 a.m.
		if (isNew && data?.engine.restic) test(target, true);
	}

	async function test(target: BackupTarget, quiet = false) {
		testing[target.id] = true;
		try {
			await api.backupTest(target.id);
			toast(t('backup.testOk', { name: target.name }), 'success');
		} catch (e) {
			if (!quiet || !(e instanceof Error)) reportError(e);
			else toast(`${target.name}: ${e.message}`, 'error');
		} finally {
			testing[target.id] = false;
			load();
		}
	}

	async function runNow(target: BackupTarget) {
		try {
			const { run_id } = await api.backupRun(target.id);
			watching.set(target.id, { runId: run_id, kind: 'backup' });
			toast(t('backup.started', { name: target.name }));
			load();
		} catch (e) {
			reportError(e);
		}
	}

	async function showKey(target: BackupTarget) {
		const password = await promptDialog({
			title: t('backup.showKeyTitle'),
			label: t('backup.yourPassword'),
			confirmLabel: t('backup.showKey'),
			inputType: 'password'
		});
		if (!password) return;
		try {
			const { recovery_key } = await api.backupKey(target.id, password);
			if (recovery_key) keyShown = { target, key: recovery_key, firstTime: false };
		} catch (e) {
			reportError(e);
		}
	}

	async function remove(target: BackupTarget) {
		const ok = await confirmDialog({
			title: t('backup.removeTitle'),
			message: t('backup.removeConfirm', { name: target.name }),
			confirmLabel: t('backup.remove'),
			danger: true
		});
		if (!ok) return;
		try {
			await api.backupDelete(target.id);
			load();
			backupStatus.refresh();
		} catch (e) {
			reportError(e);
		}
	}

	async function setEnabled(target: BackupTarget, enabled: boolean) {
		try {
			await api.backupUpdate(target.id, { enabled });
			toast(enabled ? t('backup.resumed') : t('backup.paused'), 'success');
			load();
			backupStatus.refresh();
		} catch (e) {
			reportError(e);
		}
	}

	function actions(target: BackupTarget): MenuItem[] {
		const items: MenuItem[] = [
			{ label: t('backup.edit'), icon: Pencil, run: () => (editing = target) },
			{ label: t('backup.test'), icon: PlugZap, run: () => test(target) },
			{ label: t('backup.history'), icon: History, run: () => (history = target) }
		];
		if (target.encrypted) items.push({ label: t('backup.showKey'), icon: KeyRound, run: () => showKey(target) });
		items.push(
			target.enabled
				? { label: t('backup.pause'), icon: Pause, run: () => setEnabled(target, false) }
				: { label: t('backup.resume'), icon: Play, run: () => setEnabled(target, true) },
			{ label: t('backup.remove'), icon: Trash, danger: true, separatorBefore: true, run: () => remove(target) }
		);
		return items;
	}

	function scheduleLabel(target: BackupTarget): string {
		const s = parseSchedule(target.schedule);
		if (s.freq === 'manual') return t('backup.manualOnly');
		const [h, m] = s.time.split(':').map(Number);
		const at = new Date(2000, 0, 1, h, m).toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' });
		return s.freq === 'daily'
			? t('backup.everyDay', { time: at })
			: t('backup.everyWeek', { day: t(`backup.days.${s.day}` as MessageKey), time: at });
	}

	const missingEngine = (target: BackupTarget) =>
		!!data && (!data.engine.restic || ((target.kind === 'smb' || target.kind === 'sftp') && !data.engine.rclone));
</script>

<svelte:head><title>{t('backup.title')} · Fennoly</title></svelte:head>

<div class="max-w-4xl pt-3 lg:pt-1">
	<div class="flex items-start gap-3 mb-6">
		<div class="flex-1 min-w-0">
			<h1 class="text-xl sm:text-2xl font-bold tracking-tight">{t('backup.title')}</h1>
			<p class="text-muted text-sm mt-1">{t('backup.subtitle')}</p>
		</div>
		{#if data?.targets.length}
			<button class="btn-primary shrink-0" onclick={() => (editing = null)}><Plus size={18} /><span class="max-sm:hidden">{t('backup.add')}</span></button>
		{/if}
	</div>

	{#if data && !data.engine.restic}
		<div class="card p-4 mb-4 flex gap-3 border-warning/40">
			<TriangleAlert size={20} class="text-warning shrink-0 mt-0.5" />
			<div class="text-sm">
				<div class="font-medium">{t('backup.noEngine')}</div>
				<p class="text-muted mt-1">{t('backup.noEngineHint')}</p>
			</div>
		</div>
	{/if}

	{#if data === null}
		<p class="text-sm text-muted py-10 text-center">{t('common.loading')}</p>
	{:else if data.targets.length === 0}
		<EmptyState title={t('backup.emptyTitle')} body={t('backup.emptyText')}>
			{#snippet icon()}<DatabaseBackup size={36} />{/snippet}
			<button class="btn-primary" onclick={() => (editing = null)}><Plus size={18} />{t('backup.addFirst')}</button>
		</EmptyState>
	{:else}
		<ul class="grid grid-cols-1 gap-3">
			{#each data.targets as target (target.id)}
				{@const Icon = KIND_ICONS[target.kind]}
				{@const last = target.last_run}
				<li class="card p-4 sm:p-5 {target.enabled ? '' : 'opacity-75'}">
					<div class="flex items-start gap-3 sm:gap-4">
						<span class="size-11 rounded-2xl bg-primary-soft text-primary grid place-items-center shrink-0"><Icon size={22} /></span>
						<div class="flex-1 min-w-0">
							<div class="flex items-center gap-2 flex-wrap">
								<span class="font-semibold truncate">{target.name}</span>
								{#if target.encrypted}
									<span class="inline-flex items-center gap-1 text-xs px-2 py-0.5 rounded-full bg-surface-2 text-muted"><Lock size={12} />{t('backup.encrypted')}</span>
								{:else}
									<span class="inline-flex items-center gap-1 text-xs px-2 py-0.5 rounded-full bg-surface-2 text-muted"><LockOpen size={12} />{t('backup.notEncrypted')}</span>
								{/if}
								{#if !target.enabled}<span class="text-xs px-2 py-0.5 rounded-full bg-surface-2 text-muted font-medium">{t('backup.pausedBadge')}</span>{/if}
							</div>
							<div class="text-sm text-muted truncate font-mono mt-0.5" title={target.location}>{target.location}</div>

							<div class="mt-3 text-sm space-y-1">
								{#if target.running !== null}
									<div class="flex items-center gap-2 text-primary font-medium">
										<LoaderCircle size={16} class="animate-spin" />{t('backup.running')}
										<span class="tabular-nums text-muted font-normal">{Math.round(target.running * 100)}%</span>
									</div>
									<div class="h-1.5 rounded-full bg-surface-3 overflow-hidden max-w-sm" role="progressbar" aria-valuemin="0" aria-valuemax="100" aria-valuenow={Math.round(target.running * 100)}>
										<div class="h-full rounded-full bg-primary transition-[width] duration-700" style="width:{Math.max(2, target.running * 100)}%"></div>
									</div>
								{:else if testing[target.id]}
									<div class="flex items-center gap-2 text-primary"><LoaderCircle size={16} class="animate-spin" />{t('backup.testing')}</div>
								{:else if last?.status === 'failed'}
									<div class="flex items-start gap-2 text-danger">
										<CircleX size={16} class="shrink-0 mt-0.5" />
										<span class="min-w-0 break-words">{t('backup.lastFailed', { when: relativeTime(last.started_at) })} {last.error}</span>
									</div>
									{#if target.last_success_at}
										<div class="text-muted text-xs pl-6">{t('backup.lastGood', { when: relativeTime(target.last_success_at) })}</div>
									{/if}
								{:else if last?.status === 'success'}
									<div class="flex items-start gap-2">
										<CircleCheck size={16} class="text-success shrink-0 mt-0.5" />
										<span class="min-w-0">
											{t('backup.lastOk', { when: relativeTime(last.finished_at ?? last.started_at) })}
											{#if last.total_files !== null}
												<span class="text-muted">· {t('backup.lastStats', { files: last.total_files.toLocaleString(), size: formatBytes(last.total_bytes ?? 0), added: formatBytes(last.bytes_added ?? 0) })}</span>
											{/if}
										</span>
									</div>
								{:else}
									<div class="text-muted">{t('backup.never')}</div>
								{/if}
								{#if missingEngine(target)}
									<div class="flex items-start gap-2 text-warning text-xs"><TriangleAlert size={14} class="shrink-0 mt-px" />{t('backup.engineNeeded')}</div>
								{/if}
								<div class="text-muted text-xs">
									<span>{scheduleLabel(target)}</span>
									{#if target.enabled && target.next_run_at && target.running === null}
										<span title={formatDateTime(target.next_run_at)}>· {t('backup.next', { when: relativeTime(target.next_run_at) })}</span>
									{/if}
								</div>
							</div>
						</div>
						<button
							class="icon-btn -mr-1 shrink-0"
							aria-label={t('common.moreActions')}
							onclick={(e) => {
								const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
								menu = { x: r.right - 220, y: r.bottom + 4, items: actions(target) };
							}}
						>
							<EllipsisVertical size={18} />
						</button>
					</div>
					<div class="flex flex-wrap gap-2 mt-4 sm:pl-[3.75rem] max-sm:[&>*]:flex-1">
						<button class="btn-outline" disabled={target.running !== null || missingEngine(target)} onclick={() => runNow(target)}>
							<DatabaseBackup size={17} />{t('backup.runNow')}
						</button>
						<button class="btn-ghost" disabled={missingEngine(target) || !target.last_success_at} onclick={() => (browsing = target)}>
							<ArchiveRestore size={17} />{t('backup.browse')}
						</button>
					</div>
				</li>
			{/each}
		</ul>
		<p class="text-xs text-muted mt-4 leading-relaxed">{t('backup.footnote')}</p>
	{/if}
</div>

{#if menu}<Menu x={menu.x} y={menu.y} items={menu.items} onclose={() => (menu = null)} />{/if}

{#if editing !== undefined}
	<TargetDialog target={editing} onclose={() => (editing = undefined)} onsaved={saved} />
{/if}
{#if keyShown}
	<RecoveryKeyDialog target={keyShown.target} recoveryKey={keyShown.key} firstTime={keyShown.firstTime} onclose={() => (keyShown = null)} />
{/if}
{#if history}
	<HistoryDialog target={history} onclose={() => (history = null)} />
{/if}
{#if browsing}
	{@const target = browsing}
	<SnapshotBrowser
		{target}
		onclose={() => (browsing = null)}
		onrestore={(runId) => {
			browsing = null;
			watching.set(target.id, { runId, kind: 'restore' });
			toast(t('backup.restoreStarted'));
			load();
		}}
	/>
{/if}
