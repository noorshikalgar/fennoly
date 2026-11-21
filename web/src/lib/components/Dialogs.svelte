<script lang="ts">
	// Host for confirmDialog() / promptDialog().
	import { dialog } from '$lib/ui.svelte';
	import { t } from '$lib/i18n/index.svelte';
	import Modal from './Modal.svelte';

	let value = $state('');
	let input = $state<HTMLInputElement>();

	$effect(() => {
		const d = dialog.current;
		if (d?.kind === 'prompt') {
			value = d.value ?? '';
			queueMicrotask(() => {
				if (!input) return;
				input.focus();
				const dot = value.lastIndexOf('.');
				if (d.selectStem && dot > 0) input.setSelectionRange(0, dot);
				else input.select();
			});
		}
	});

	function finish(result: string | boolean | null) {
		const d = dialog.current;
		dialog.current = null;
		d?.resolve(result);
	}

	function submit(e: SubmitEvent) {
		e.preventDefault();
		const d = dialog.current;
		if (!d) return;
		if (d.kind === 'prompt') {
			if (value.trim() || d.allowEmpty) finish(value.trim());
		} else finish(true);
	}
</script>

{#if dialog.current}
	{@const d = dialog.current}
	<Modal title={d.title} size="sm" onclose={() => finish(d.kind === 'confirm' ? false : null)}>
		<form id="fennoly-dialog" onsubmit={submit}>
			{#if d.message}
				<p class="text-muted leading-relaxed">{d.message}</p>
			{/if}
			{#if d.kind === 'prompt'}
				{#if d.label}<label class="label" for="fennoly-dialog-input">{d.label}</label>{/if}
				<input
					id="fennoly-dialog-input"
					class="input"
					type={d.inputType ?? 'text'}
					bind:this={input}
					bind:value
					autocomplete="off"
				/>
			{/if}
		</form>
		{#snippet footer()}
			<button type="button" class="btn-ghost" onclick={() => finish(d.kind === 'confirm' ? false : null)}>
				{t('common.cancel')}
			</button>
			<button type="submit" form="fennoly-dialog" class={d.danger ? 'btn-danger' : 'btn-primary'}>
				{d.confirmLabel}
			</button>
		{/snippet}
	</Modal>
{/if}
