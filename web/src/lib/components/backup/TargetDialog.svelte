<script lang="ts" module>
	import type { BackupKind } from '$lib/types';
	import { Cloud, Server, TerminalSquare, Usb } from '@lucide/svelte';

	export const KIND_ICONS: Record<BackupKind, typeof Usb> = { local: Usb, smb: Server, sftp: TerminalSquare, s3: Cloud };

	export type Freq = 'manual' | 'daily' | 'weekly';
	export const DAYS = ['mon', 'tue', 'wed', 'thu', 'fri', 'sat', 'sun'] as const;

	export function parseSchedule(s: string): { freq: Freq; day: string; time: string } {
		const p = s.trim().split(/\s+/);
		if (p[0] === 'daily' && p[1]) return { freq: 'daily', day: 'sun', time: p[1] };
		if (p[0] === 'weekly' && p[2]) return { freq: 'weekly', day: p[1], time: p[2] };
		return { freq: 'manual', day: 'sun', time: '02:00' };
	}

	export function scheduleText(freq: Freq, day: string, time: string): string {
		const hhmm = /^\d{2}:\d{2}/.test(time) ? time.slice(0, 5) : '02:00';
		if (freq === 'daily') return `daily ${hhmm}`;
		if (freq === 'weekly') return `weekly ${day} ${hhmm}`;
		return 'manual';
	}
</script>

<script lang="ts">
	import { ChevronLeft, Lock, LockOpen, TriangleAlert } from '@lucide/svelte';
	import { untrack } from 'svelte';
	import { api } from '$lib/api';
	import { reportError } from '$lib/actions';
	import Modal from '$lib/components/Modal.svelte';
	import Switch from '$lib/components/Switch.svelte';
	import { t, type MessageKey } from '$lib/i18n/index.svelte';
	import type { BackupSecrets, BackupSettings, BackupTarget, BackupTargetInput } from '$lib/types';

	let {
		target = null,
		onclose,
		onsaved
	}: {
		target?: BackupTarget | null;
		onclose: () => void;
		onsaved: (target: BackupTarget, recoveryKey: string | null) => void;
	} = $props();

	// The dialog edits a copy of the destination as it was when opened.
	const start = untrack(() => target);
	const editing = start !== null;
	const initial = parseSchedule(start?.schedule ?? 'daily 02:00');

	let kind = $state<BackupKind | null>(start?.kind ?? null);
	let name = $state(start?.name ?? '');
	let settings = $state<BackupSettings>({ ...(start?.settings ?? {}) });
	let port = $state(start?.settings.port ? String(start.settings.port) : '');
	let secrets = $state<BackupSecrets>({});
	let encrypted = $state(start?.encrypted ?? true);
	let haveKey = $state(false);
	let existingKey = $state('');
	let freq = $state<Freq>(initial.freq);
	let day = $state(initial.day);
	let time = $state(initial.time);
	let keepDaily = $state(start?.keep_daily ?? 7);
	let keepWeekly = $state(start?.keep_weekly ?? 4);
	let keepMonthly = $state(start?.keep_monthly ?? 12);
	let includeTrash = $state(start?.include_trash ?? false);
	let saving = $state(false);

	const kinds: { id: BackupKind; title: MessageKey; hint: MessageKey }[] = [
		{ id: 'local', title: 'backup.kind.local', hint: 'backup.kind.localHint' },
		{ id: 'smb', title: 'backup.kind.smb', hint: 'backup.kind.smbHint' },
		{ id: 'sftp', title: 'backup.kind.sftp', hint: 'backup.kind.sftpHint' },
		{ id: 's3', title: 'backup.kind.s3', hint: 'backup.kind.s3Hint' }
	];

	function choose(k: BackupKind) {
		kind = k;
		if (!name.trim()) name = t(`backup.kind.${k}Default` as MessageKey);
	}

	const isWindows = typeof navigator !== 'undefined' && /Windows/i.test(navigator.userAgent);
	const secretPlaceholder = (has: boolean) => (editing && has ? t('backup.unchanged') : '');

	async function save(e: SubmitEvent) {
		e.preventDefault();
		if (!kind || saving) return;
		const p = parseInt(port, 10);
		const body: BackupTargetInput = {
			name,
			settings: { ...settings, port: Number.isFinite(p) && p > 0 ? p : undefined },
			secrets,
			schedule: scheduleText(freq, day, time),
			keep_daily: Number(keepDaily) || 0,
			keep_weekly: Number(keepWeekly) || 0,
			keep_monthly: Number(keepMonthly) || 0,
			include_trash: includeTrash
		};
		saving = true;
		try {
			if (target) {
				onsaved(await api.backupUpdate(target.id, body), null);
			} else {
				const res = await api.backupCreate({
					...body,
					kind,
					encrypted,
					recovery_key: encrypted && haveKey && existingKey.trim() ? existingKey.trim() : undefined
				});
				onsaved(res.target, res.recovery_key);
			}
		} catch (err) {
			reportError(err);
		} finally {
			saving = false;
		}
	}
</script>

<Modal title={editing ? t('backup.editTitle') : t('backup.addTitle')} size="lg" {onclose}>
	{#if !kind}
		<p class="text-sm text-muted mb-4">{t('backup.chooseKind')}</p>
		<div class="grid grid-cols-1 sm:grid-cols-2 gap-3">
			{#each kinds as k (k.id)}
				{@const Icon = KIND_ICONS[k.id]}
				<button
					type="button"
					class="flex items-start gap-3 text-left rounded-2xl border border-border p-4 hover:border-primary hover:bg-primary-soft/40 transition-colors"
					onclick={() => choose(k.id)}
				>
					<span class="size-10 rounded-xl bg-primary-soft text-primary grid place-items-center shrink-0"><Icon size={20} /></span>
					<span class="min-w-0">
						<span class="block font-medium">{t(k.title)}</span>
						<span class="block text-xs text-muted mt-0.5 leading-relaxed">{t(k.hint)}</span>
					</span>
				</button>
			{/each}
			<div class="flex items-start gap-3 rounded-2xl border border-dashed border-border p-4 opacity-70" aria-disabled="true">
				<span class="size-10 rounded-xl bg-surface-2 text-muted grid place-items-center shrink-0"><Cloud size={20} /></span>
				<span class="min-w-0">
					<span class="block font-medium">{t('backup.kind.drives')}</span>
					<span class="block text-xs text-muted mt-0.5">{t('backup.comingSoon')}</span>
				</span>
			</div>
		</div>
	{:else}
		<form id="backup-target" class="space-y-5" onsubmit={save}>
			{#if !editing}
				<button type="button" class="btn-ghost -ml-3 h-8 px-2 text-muted" onclick={() => (kind = null)}>
					<ChevronLeft size={17} />{t(`backup.kind.${kind}` as MessageKey)}
				</button>
			{/if}

			<div>
				<label class="label" for="bk-name">{t('backup.name')}</label>
				<input id="bk-name" class="input" bind:value={name} maxlength="80" required />
			</div>

			<fieldset class="space-y-4">
				<legend class="text-sm font-semibold mb-3">{t('backup.where')}</legend>
				{#if kind === 'local'}
					<div>
						<label class="label" for="bk-path">{t('backup.folder')}</label>
						<input
							id="bk-path"
							class="input font-mono text-sm"
							bind:value={settings.path}
							placeholder={isWindows ? 'D:\\Backups\\Fennoly' : '/mnt/usb/fennoly-backups'}
							autocapitalize="none"
							spellcheck="false"
							required
						/>
						<p class="text-xs text-muted mt-1.5">{t('backup.folderHint')}</p>
					</div>
				{:else if kind === 'smb'}
					<div class="grid grid-cols-1 sm:grid-cols-2 gap-4">
						<div>
							<label class="label" for="bk-host">{t('backup.server')}</label>
							<input id="bk-host" class="input" bind:value={settings.host} placeholder="192.168.1.20" autocapitalize="none" spellcheck="false" required />
						</div>
						<div>
							<label class="label" for="bk-share">{t('backup.share')}</label>
							<input id="bk-share" class="input" bind:value={settings.share} placeholder="backups" autocapitalize="none" spellcheck="false" required />
						</div>
						<div>
							<label class="label" for="bk-user">{t('backup.user')}</label>
							<input id="bk-user" class="input" bind:value={settings.user} autocapitalize="none" autocomplete="off" spellcheck="false" required />
						</div>
						<div>
							<label class="label" for="bk-pass">{t('backup.password')}</label>
							<input
								id="bk-pass"
								class="input"
								type="password"
								bind:value={secrets.password}
								placeholder={secretPlaceholder(target?.has_password ?? false)}
								autocomplete="new-password"
								required={!editing || !target?.has_password}
							/>
						</div>
						<div>
							<label class="label" for="bk-sub">{t('backup.subfolder')}</label>
							<input id="bk-sub" class="input" bind:value={settings.path} placeholder="fennoly" autocapitalize="none" spellcheck="false" />
						</div>
						<div>
							<label class="label" for="bk-domain">{t('backup.domain')}</label>
							<input id="bk-domain" class="input" bind:value={settings.domain} placeholder="WORKGROUP" autocapitalize="none" spellcheck="false" />
						</div>
					</div>
				{:else if kind === 'sftp'}
					<div class="grid grid-cols-1 sm:grid-cols-[1fr_7rem] gap-4">
						<div>
							<label class="label" for="bk-host">{t('backup.server')}</label>
							<input id="bk-host" class="input" bind:value={settings.host} placeholder="backup.example.com" autocapitalize="none" spellcheck="false" required />
						</div>
						<div>
							<label class="label" for="bk-port">{t('backup.port')}</label>
							<input id="bk-port" class="input" inputmode="numeric" bind:value={port} placeholder="22" />
						</div>
					</div>
					<div class="grid grid-cols-1 sm:grid-cols-2 gap-4">
						<div>
							<label class="label" for="bk-user">{t('backup.user')}</label>
							<input id="bk-user" class="input" bind:value={settings.user} autocapitalize="none" autocomplete="off" spellcheck="false" required />
						</div>
						<div>
							<label class="label" for="bk-pass">{t('backup.password')}</label>
							<input
								id="bk-pass"
								class="input"
								type="password"
								bind:value={secrets.password}
								placeholder={secretPlaceholder(target?.has_password ?? false)}
								autocomplete="new-password"
								required={!editing || !target?.has_password}
							/>
						</div>
					</div>
					<div>
						<label class="label" for="bk-path">{t('backup.remoteFolder')}</label>
						<input id="bk-path" class="input font-mono text-sm" bind:value={settings.path} placeholder="/home/noor/fennoly-backups" autocapitalize="none" spellcheck="false" required />
					</div>
				{:else if kind === 's3'}
					<div>
						<label class="label" for="bk-endpoint">{t('backup.endpoint')}</label>
						<input
							id="bk-endpoint"
							class="input font-mono text-sm"
							bind:value={settings.endpoint}
							placeholder="https://s3.eu-central-003.backblazeb2.com"
							autocapitalize="none"
							spellcheck="false"
							required
						/>
						<p class="text-xs text-muted mt-1.5">{t('backup.endpointHint')}</p>
					</div>
					<div class="grid grid-cols-1 sm:grid-cols-2 gap-4">
						<div>
							<label class="label" for="bk-bucket">{t('backup.bucket')}</label>
							<input id="bk-bucket" class="input" bind:value={settings.bucket} autocapitalize="none" spellcheck="false" required />
						</div>
						<div>
							<label class="label" for="bk-prefix">{t('backup.subfolder')}</label>
							<input id="bk-prefix" class="input" bind:value={settings.path} placeholder="fennoly" autocapitalize="none" spellcheck="false" />
						</div>
						<div>
							<label class="label" for="bk-ak">{t('backup.accessKey')}</label>
							<input
								id="bk-ak"
								class="input font-mono text-sm"
								bind:value={secrets.access_key}
								placeholder={secretPlaceholder(target?.has_keys ?? false)}
								autocapitalize="none"
								autocomplete="off"
								spellcheck="false"
								required={!editing || !target?.has_keys}
							/>
						</div>
						<div>
							<label class="label" for="bk-sk">{t('backup.secretKey')}</label>
							<input
								id="bk-sk"
								class="input font-mono text-sm"
								type="password"
								bind:value={secrets.secret_key}
								placeholder={secretPlaceholder(target?.has_keys ?? false)}
								autocomplete="new-password"
								required={!editing || !target?.has_keys}
							/>
						</div>
						<div>
							<label class="label" for="bk-region">{t('backup.region')}</label>
							<input id="bk-region" class="input" bind:value={settings.region} placeholder="eu-central-003" autocapitalize="none" spellcheck="false" />
						</div>
					</div>
				{/if}
			</fieldset>

			<fieldset>
				<legend class="text-sm font-semibold mb-3">{t('backup.when')}</legend>
				<div class="grid grid-cols-3 gap-2 mb-3" role="radiogroup" aria-label={t('backup.when')}>
					{#each [['daily', t('backup.daily')], ['weekly', t('backup.weekly')], ['manual', t('backup.manual')]] as [value, label] (value)}
						<button
							type="button"
							role="radio"
							aria-checked={freq === value}
							class="h-10 rounded-xl border text-sm font-medium transition-colors
								{freq === value ? 'border-primary bg-primary-soft text-primary' : 'border-border hover:bg-surface-2'}"
							onclick={() => (freq = value as Freq)}>{label}</button
						>
					{/each}
				</div>
				{#if freq !== 'manual'}
					<div class="flex flex-wrap items-end gap-3">
						{#if freq === 'weekly'}
							<div class="flex-1 min-w-[9rem]">
								<label class="label" for="bk-day">{t('backup.day')}</label>
								<select id="bk-day" class="input" bind:value={day}>
									{#each DAYS as d (d)}<option value={d}>{t(`backup.days.${d}` as MessageKey)}</option>{/each}
								</select>
							</div>
						{/if}
						<div class="w-36">
							<label class="label" for="bk-time">{t('backup.time')}</label>
							<input id="bk-time" class="input" type="time" bind:value={time} required />
						</div>
					</div>
					<p class="text-xs text-muted mt-2">{t('backup.timeHint')}</p>
				{:else}
					<p class="text-xs text-muted">{t('backup.manualHint')}</p>
				{/if}
			</fieldset>

			<fieldset>
				<legend class="text-sm font-semibold mb-1">{t('backup.keep')}</legend>
				<p class="text-xs text-muted mb-3">{t('backup.keepHint')}</p>
				<div class="grid grid-cols-3 gap-3">
					<div>
						<label class="label text-xs" for="bk-kd">{t('backup.keepDaily')}</label>
						<input id="bk-kd" class="input" type="number" min="0" max="365" bind:value={keepDaily} />
					</div>
					<div>
						<label class="label text-xs" for="bk-kw">{t('backup.keepWeekly')}</label>
						<input id="bk-kw" class="input" type="number" min="0" max="520" bind:value={keepWeekly} />
					</div>
					<div>
						<label class="label text-xs" for="bk-km">{t('backup.keepMonthly')}</label>
						<input id="bk-km" class="input" type="number" min="0" max="240" bind:value={keepMonthly} />
					</div>
				</div>
			</fieldset>

			<Switch bind:checked={includeTrash} label={t('backup.includeTrash')} hint={t('backup.includeTrashHint')} />

			<div class="rounded-2xl bg-surface-2 p-4 space-y-3">
				{#if editing}
					<div class="flex items-start gap-3">
						{#if encrypted}<Lock size={20} class="text-primary shrink-0 mt-0.5" />{:else}<LockOpen size={20} class="text-muted shrink-0 mt-0.5" />{/if}
						<div>
							<div class="text-sm font-medium">{encrypted ? t('backup.encrypted') : t('backup.notEncrypted')}</div>
							<div class="text-xs text-muted mt-0.5">{t('backup.encryptionFixed')}</div>
						</div>
					</div>
				{:else}
					<Switch bind:checked={encrypted} label={t('backup.encrypt')} hint={t('backup.encryptHint')} />
					{#if encrypted}
						<div class="pl-14">
							<Switch bind:checked={haveKey} label={t('backup.haveKey')} hint={t('backup.haveKeyHint')} />
							{#if haveKey}
								<input
									class="input font-mono text-sm mt-3"
									bind:value={existingKey}
									placeholder="XXXX-XXXX-XXXX-XXXX-XXXX-XXXX-XXXX-XXXX"
									aria-label={t('backup.keyLabel')}
									autocapitalize="characters"
									autocomplete="off"
									spellcheck="false"
									required
								/>
							{/if}
						</div>
					{:else}
						<div class="flex gap-2 text-warning text-xs">
							<TriangleAlert size={16} class="shrink-0" />
							<span>{t('backup.noEncryptWarning')}</span>
						</div>
					{/if}
				{/if}
			</div>
		</form>
	{/if}

	{#snippet footer()}
		<button class="btn-ghost" onclick={onclose}>{t('common.cancel')}</button>
		{#if kind}
			<button class="btn-primary" form="backup-target" disabled={saving}>{editing ? t('common.save') : t('backup.add')}</button>
		{/if}
	{/snippet}
</Modal>
