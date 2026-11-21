<script lang="ts">
	import { EllipsisVertical, HardDrive, KeyRound, ShieldCheck, Trash, UserCheck, UserPlus, UserX, User } from '@lucide/svelte';
	import { goto } from '$app/navigation';
	import { api } from '$lib/api';
	import { reportError } from '$lib/actions';
	import Avatar from '$lib/components/Avatar.svelte';
	import Menu, { type MenuItem } from '$lib/components/Menu.svelte';
	import Modal from '$lib/components/Modal.svelte';
	import { formatBytes, relativeTime } from '$lib/format';
	import { i18n, t } from '$lib/i18n/index.svelte';
	import { session, toast } from '$lib/stores.svelte';
	import type { AdminUser } from '$lib/types';
	import { confirmDialog, promptDialog, ui } from '$lib/ui.svelte';

	let users = $state<AdminUser[]>([]);
	let adding = $state(false);
	let form = $state({ display_name: '', username: '', password: '', role: 'member' });
	let menu = $state<{ x: number; y: number; items: MenuItem[] } | null>(null);

	async function load() {
		try {
			users = await api.adminUsers();
		} catch (e) {
			reportError(e);
		}
	}

	$effect(() => {
		ui.folder = null;
		ui.space = null;
		if (session.me?.role !== 'admin') goto('/');
		else load();
	});

	async function create(e: SubmitEvent) {
		e.preventDefault();
		try {
			const u = await api.adminCreate({ ...form, locale: i18n.locale });
			toast(t('admin.added', { name: u.display_name }), 'success');
			adding = false;
			form = { display_name: '', username: '', password: '', role: 'member' };
			load();
		} catch (err) {
			reportError(err);
		}
	}

	async function update(u: AdminUser, patch: Record<string, unknown>, message?: string) {
		try {
			await api.adminUpdate(u.id, patch);
			if (message) toast(message, 'success');
			load();
		} catch (e) {
			reportError(e);
		}
	}

	function actions(u: AdminUser): MenuItem[] {
		const self = u.id === session.me?.id;
		const items: MenuItem[] = [
			{
				label: t('admin.resetPassword'),
				icon: KeyRound,
				run: async () => {
					const pw = await promptDialog({
						title: t('admin.newPasswordFor', { name: u.display_name }),
						label: t('admin.password'),
						confirmLabel: t('common.save'),
						inputType: 'password'
					});
					if (pw) update(u, { password: pw }, t('admin.passwordReset', { name: u.display_name }));
				}
			},
			{
				label: t('admin.quota'),
				icon: HardDrive,
				run: async () => {
					const gb = await promptDialog({
						title: t('admin.quota'),
						label: t('admin.quotaGb'),
						value: u.quota_bytes ? String(+(u.quota_bytes / 1024 ** 3).toFixed(2)) : '',
						confirmLabel: t('common.save'),
						inputType: 'text',
						allowEmpty: true
					});
					if (gb === null) return;
					const n = parseFloat(gb);
					update(u, { quota_bytes: isFinite(n) && n > 0 ? Math.round(n * 1024 ** 3) : null }, t('settings.saved'));
				}
			}
		];
		if (!self) {
			items.push(
				u.role === 'admin'
					? { label: t('admin.makeMember'), icon: User, run: () => update(u, { role: 'member' }) }
					: { label: t('admin.makeAdmin'), icon: ShieldCheck, run: () => update(u, { role: 'admin' }) },
				u.disabled
					? { label: t('admin.enable'), icon: UserCheck, run: () => update(u, { disabled: false }) }
					: { label: t('admin.disable'), icon: UserX, run: () => update(u, { disabled: true }) },
				{
					label: t('common.delete'),
					icon: Trash,
					danger: true,
					separatorBefore: true,
					run: async () => {
						const ok = await confirmDialog({
							title: t('common.delete'),
							message: t('admin.deleteConfirm', { name: u.display_name }),
							confirmLabel: t('common.deleteForever'),
							danger: true
						});
						if (!ok) return;
						try {
							await api.adminDelete(u.id);
							load();
						} catch (e) {
							reportError(e);
						}
					}
				}
			);
		}
		return items;
	}
</script>

<svelte:head><title>{t('admin.title')} · Fennoly</title></svelte:head>

<div class="max-w-4xl pt-3 lg:pt-1">
	<div class="flex items-start gap-3 mb-6">
		<div class="flex-1">
			<h1 class="text-xl sm:text-2xl font-bold tracking-tight">{t('admin.title')}</h1>
			<p class="text-muted text-sm mt-1">{t('admin.subtitle')}</p>
		</div>
		<button class="btn-primary" onclick={() => (adding = true)}><UserPlus size={18} />{t('admin.add')}</button>
	</div>

	<ul class="grid grid-cols-1 gap-3">
		{#each users as u (u.id)}
			{@const pct = u.quota_bytes ? Math.min(1, u.used_bytes / u.quota_bytes) : 0}
			<li class="card p-4 flex items-center gap-4 {u.disabled ? 'opacity-60' : ''}">
				<Avatar name={u.display_name} color={u.avatar_color} size={44} />
				<div class="flex-1 min-w-0">
					<div class="flex items-center gap-2 flex-wrap">
						<span class="font-semibold truncate">{u.display_name}</span>
						{#if u.id === session.me?.id}<span class="text-xs px-2 py-0.5 rounded-full bg-surface-2 text-muted">{t('admin.you')}</span>{/if}
						{#if u.role === 'admin'}<span class="text-xs px-2 py-0.5 rounded-full bg-primary-soft text-primary font-medium">{t('admin.roleAdmin')}</span>{/if}
						{#if u.disabled}<span class="text-xs px-2 py-0.5 rounded-full bg-danger-soft text-danger font-medium">{t('admin.disabled')}</span>{/if}
					</div>
					<div class="text-sm text-muted truncate">
						@{u.username} · {u.last_seen_at ? t('admin.lastSeen', { when: relativeTime(u.last_seen_at) }) : t('admin.neverSeen')}
					</div>
				</div>
				<div class="hidden sm:block w-44 text-right">
					<div class="text-sm tabular-nums">
						{formatBytes(u.used_bytes)}{#if u.quota_bytes}<span class="text-muted"> / {formatBytes(u.quota_bytes)}</span>{/if}
					</div>
					{#if u.quota_bytes}
						<div class="h-1.5 rounded-full bg-surface-3 overflow-hidden mt-1.5">
							<div class="h-full rounded-full {pct > 0.9 ? 'bg-danger' : 'bg-primary'}" style="width:{pct * 100}%"></div>
						</div>
					{:else}
						<div class="text-xs text-muted">{t('admin.noLimit')}</div>
					{/if}
				</div>
				<button
					class="icon-btn"
					aria-label={t('common.moreActions')}
					onclick={(e) => {
						const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
						menu = { x: r.right - 220, y: r.bottom + 4, items: actions(u) };
					}}
				>
					<EllipsisVertical size={18} />
				</button>
			</li>
		{/each}
	</ul>
</div>

{#if menu}<Menu x={menu.x} y={menu.y} items={menu.items} onclose={() => (menu = null)} />{/if}

{#if adding}
	<Modal title={t('admin.add')} onclose={() => (adding = false)}>
		<form id="add-user" class="space-y-4" onsubmit={create}>
			<div>
				<label class="label" for="a-name">{t('admin.displayName')}</label>
				<input id="a-name" class="input" bind:value={form.display_name} required maxlength="64" />
			</div>
			<div>
				<label class="label" for="a-user">{t('admin.username')}</label>
				<input id="a-user" class="input" bind:value={form.username} autocapitalize="none" autocomplete="off" required />
				<p class="text-xs text-muted mt-1.5">{t('setup.usernameHint')}</p>
			</div>
			<div>
				<label class="label" for="a-pass">{t('admin.password')}</label>
				<input id="a-pass" class="input" type="password" bind:value={form.password} minlength="8" autocomplete="new-password" required />
			</div>
			<div>
				<span class="label">{t('admin.role')}</span>
				<div class="grid grid-cols-2 gap-2">
					{#each [['member', t('admin.roleMember')], ['admin', t('admin.roleAdmin')]] as [value, label] (value)}
						<button
							type="button"
							class="h-11 rounded-xl border text-sm font-medium {form.role === value ? 'border-primary bg-primary-soft text-primary' : 'border-border hover:bg-surface-2'}"
							onclick={() => (form.role = value)}>{label}</button
						>
					{/each}
				</div>
			</div>
		</form>
		{#snippet footer()}
			<button class="btn-ghost" onclick={() => (adding = false)}>{t('common.cancel')}</button>
			<button class="btn-primary" form="add-user">{t('admin.add')}</button>
		{/snippet}
	</Modal>
{/if}
