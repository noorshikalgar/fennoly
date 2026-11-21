<script lang="ts">
	import type { Snippet } from 'svelte';

	let { icon, title, body, children }: { icon: Snippet; title: string; body?: string; children?: Snippet } = $props();
</script>

<div class="flex flex-col items-center text-center py-16 px-6">
	<div class="size-20 rounded-3xl bg-primary-soft text-primary flex items-center justify-center mb-5">
		{@render icon()}
	</div>
	<h3 class="text-lg font-semibold">{title}</h3>
	{#if body}<p class="text-muted mt-1.5 max-w-sm leading-relaxed">{body}</p>{/if}
	{#if children}<div class="mt-5 flex gap-2">{@render children()}</div>{/if}
</div>
