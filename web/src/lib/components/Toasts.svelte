<script lang="ts">
	import { CircleAlert, CircleCheck, Info, X } from '@lucide/svelte';
	import { fly } from 'svelte/transition';
	import { dismissToast, toasts } from '$lib/stores.svelte';
	import { t } from '$lib/i18n/index.svelte';
</script>

<div
	class="fixed z-[60] left-1/2 -translate-x-1/2 bottom-[calc(5rem+env(safe-area-inset-bottom))] lg:bottom-6
		flex flex-col items-center gap-2 w-[min(28rem,calc(100vw-2rem))] pointer-events-none"
	aria-live="polite"
>
	{#each toasts as toast (toast.id)}
		<div
			class="pointer-events-auto w-full flex items-center gap-3 rounded-2xl bg-text text-bg px-4 py-3 shadow-xl"
			transition:fly={{ y: 16, duration: 180 }}
		>
			{#if toast.kind === 'error'}
				<CircleAlert size={18} class="text-danger shrink-0" />
			{:else if toast.kind === 'success'}
				<CircleCheck size={18} class="text-success shrink-0" />
			{:else}
				<Info size={18} class="shrink-0 opacity-70" />
			{/if}
			<span class="flex-1 text-sm leading-snug">{toast.message}</span>
			{#if toast.action}
				<button
					class="text-sm font-semibold text-primary-soft hover:underline"
					onclick={() => {
						toast.action?.run();
						dismissToast(toast.id);
					}}>{toast.action.label}</button
				>
			{/if}
			<button class="opacity-60 hover:opacity-100" aria-label={t('common.close')} onclick={() => dismissToast(toast.id)}>
				<X size={16} />
			</button>
		</div>
	{/each}
</div>
