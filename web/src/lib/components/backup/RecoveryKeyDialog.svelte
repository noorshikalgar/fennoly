<script lang="ts">
	import { Copy, Download, KeyRound } from '@lucide/svelte';
	import { copyText } from '$lib/actions';
	import Modal from '$lib/components/Modal.svelte';
	import { t } from '$lib/i18n/index.svelte';
	import { toast } from '$lib/stores.svelte';
	import type { BackupTarget } from '$lib/types';

	let {
		target,
		recoveryKey,
		firstTime = false,
		onclose
	}: { target: BackupTarget; recoveryKey: string; firstTime?: boolean; onclose: () => void } = $props();

	let saved = $state(false);
	const canClose = $derived(!firstTime || saved);

	async function copy() {
		if (await copyText(recoveryKey)) toast(t('backup.keyCopied'), 'success');
	}

	function download() {
		const text = [
			t('backup.keyFileTitle'),
			'',
			`${t('backup.destination')}: ${target.name}`,
			`${t('backup.location')}: ${target.location}`,
			`${t('backup.keyLabel')}: ${recoveryKey}`,
			'',
			t('backup.keyFileHelp'),
			''
		].join('\r\n');
		const url = URL.createObjectURL(new Blob([text], { type: 'text/plain;charset=utf-8' }));
		const a = document.createElement('a');
		a.href = url;
		a.download = `fennoly-recovery-key-${target.name.replace(/[^\p{L}\p{N}]+/gu, '-').toLowerCase()}.txt`;
		document.body.appendChild(a);
		a.click();
		a.remove();
		setTimeout(() => URL.revokeObjectURL(url), 1000);
	}

	function close() {
		if (canClose) onclose();
	}
</script>

<Modal title={t('backup.keyTitle')} onclose={close}>
	<div class="space-y-4">
		<div class="flex gap-3 rounded-2xl bg-primary-soft text-primary p-4">
			<KeyRound size={22} class="shrink-0 mt-0.5" />
			<p class="text-sm leading-relaxed">{t('backup.keyExplain')}</p>
		</div>

		<div
			class="rounded-2xl border border-border bg-surface-2 px-4 py-4 text-center font-mono text-base sm:text-lg tracking-wider
				select-all break-all leading-relaxed"
			aria-label={t('backup.keyLabel')}
		>
			{recoveryKey}
		</div>

		<div class="grid grid-cols-2 gap-2">
			<button type="button" class="btn-outline" onclick={copy}><Copy size={17} />{t('backup.keyCopy')}</button>
			<button type="button" class="btn-outline" onclick={download}><Download size={17} />{t('backup.keyDownload')}</button>
		</div>

		{#if firstTime}
			<label class="flex items-start gap-3 cursor-pointer rounded-xl p-3 -mx-1 hover:bg-surface-2">
				<input type="checkbox" class="mt-0.5 size-5 accent-[var(--primary)] shrink-0" bind:checked={saved} />
				<span class="text-sm">{t('backup.keySaved')}</span>
			</label>
		{/if}
	</div>
	{#snippet footer()}
		<button class="btn-primary" disabled={!canClose} onclick={onclose}>{t('backup.keyDone')}</button>
	{/snippet}
</Modal>
