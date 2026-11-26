<script lang="ts">
	// Fennoly brand mark: a teal sprout on a rounded tile, plus the wordmark.
	// The geometry matches web/static/icons/icon.svg and scripts/make-icons.py.
	let {
		size = 32,
		wordmark = false,
		class: className = ''
	}: { size?: number; wordmark?: boolean; class?: string } = $props();
</script>

<span class="inline-flex items-center gap-2.5 {className}">
	<svg width={size} height={size} viewBox="0 0 64 64" aria-hidden={wordmark} role={wordmark ? undefined : 'img'} aria-label={wordmark ? undefined : 'Fennoly'}>
		<rect width="64" height="64" rx="15" fill="#0F6E56" />
		<path d="M32.5 46v8" stroke="#E1F5EE" stroke-width="3" stroke-linecap="round" />
		<path d="M32 50C23 45 18 33 23 17c11 4 16 17 9 33z" fill="#9FE1CB" />
		<path d="M33 50c9-4 15-14 12-27-9 3-14 13-12 27z" fill="#E1F5EE" />
	</svg>
	{#if wordmark}
		<span translate="no" class="font-bold tracking-[-0.03em] text-text leading-none" style="font-size:{Math.round(size * 0.66)}px">Fennoly</span>
	{/if}
</span>
