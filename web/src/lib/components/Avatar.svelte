<script lang="ts">
	let { name, color, size = 36 }: { name: string; color: string; size?: number } = $props();

	const initials = $derived(
		name
			.trim()
			.split(/\s+/)
			.slice(0, 2)
			.map((w) => Array.from(w)[0] ?? '')
			.join('')
			.toUpperCase() || '?'
	);
</script>

<span
	class="inline-flex items-center justify-center rounded-full font-semibold text-white shrink-0 select-none"
	style="width:{size}px;height:{size}px;background:{color};font-size:{Math.round(size * 0.38)}px"
	aria-hidden="true">{initials}</span
>
