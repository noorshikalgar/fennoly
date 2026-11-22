<script lang="ts">
	let {
		checked = $bindable(false),
		label,
		hint,
		disabled = false,
		onchange
	}: {
		checked?: boolean;
		label: string;
		hint?: string;
		disabled?: boolean;
		onchange?: (checked: boolean) => void;
	} = $props();

	const id = `sw-${Math.random().toString(36).slice(2, 9)}`;
</script>

<div class="flex items-start gap-3 {disabled ? 'opacity-60' : ''}">
	<button
		type="button"
		role="switch"
		{id}
		aria-checked={checked}
		aria-describedby={hint ? `${id}-hint` : undefined}
		{disabled}
		class="mt-0.5 relative inline-flex h-6 w-11 shrink-0 rounded-full transition-colors {checked ? 'bg-primary' : 'bg-surface-3'}"
		onclick={() => {
			checked = !checked;
			onchange?.(checked);
		}}
	>
		<span class="sr-only">{label}</span>
		<span class="absolute top-0.5 left-0.5 size-5 rounded-full bg-white shadow transition-transform {checked ? 'translate-x-5' : ''}"></span>
	</button>
	<label for={id} class="min-w-0 {disabled ? '' : 'cursor-pointer'}">
		<span class="block text-sm font-medium">{label}</span>
		{#if hint}<span id="{id}-hint" class="block text-xs text-muted mt-0.5">{hint}</span>{/if}
	</label>
</div>
