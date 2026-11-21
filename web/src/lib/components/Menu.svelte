<script lang="ts" module>
	import type { Component } from 'svelte';

	export interface MenuItem {
		label: string;
		icon?: Component<{ size?: number }>;
		run: () => void;
		danger?: boolean;
		separatorBefore?: boolean;
	}
</script>

<script lang="ts">
	import { scale } from 'svelte/transition';

	let { x, y, items, onclose }: { x: number; y: number; items: MenuItem[]; onclose: () => void } = $props();

	let el: HTMLDivElement;
	let pos = $state({ left: 0, top: 0 });

	$effect(() => {
		// Keep the menu on screen.
		const r = el.getBoundingClientRect();
		pos = {
			left: Math.max(8, Math.min(x, window.innerWidth - r.width - 8)),
			top: y + r.height > window.innerHeight - 8 ? Math.max(8, y - r.height) : y
		};
		el.querySelector<HTMLButtonElement>('button')?.focus();
		const close = (e: Event) => {
			if (e.type === 'keydown' && (e as KeyboardEvent).key !== 'Escape') return;
			if (e.type === 'pointerdown' && el.contains(e.target as Node)) return;
			onclose();
		};
		const t = setTimeout(() => {
			window.addEventListener('pointerdown', close);
			window.addEventListener('keydown', close);
			window.addEventListener('scroll', close, true);
			window.addEventListener('resize', close);
		});
		return () => {
			clearTimeout(t);
			window.removeEventListener('pointerdown', close);
			window.removeEventListener('keydown', close);
			window.removeEventListener('scroll', close, true);
			window.removeEventListener('resize', close);
		};
	});

	function onkeydown(e: KeyboardEvent) {
		const buttons = Array.from(el.querySelectorAll<HTMLButtonElement>('button'));
		const i = buttons.indexOf(document.activeElement as HTMLButtonElement);
		if (e.key === 'ArrowDown') buttons[(i + 1) % buttons.length]?.focus();
		else if (e.key === 'ArrowUp') buttons[(i - 1 + buttons.length) % buttons.length]?.focus();
		else return;
		e.preventDefault();
	}
</script>

<div
	bind:this={el}
	role="menu"
	tabindex="-1"
	{onkeydown}
	class="fixed z-[55] min-w-52 py-1.5 rounded-2xl bg-surface border border-border shadow-xl origin-top-left"
	style="left:{pos.left}px;top:{pos.top}px"
	transition:scale={{ start: 0.95, duration: 120 }}
>
	{#each items as item (item.label)}
		{#if item.separatorBefore}<div class="my-1.5 h-px bg-border"></div>{/if}
		<button
			role="menuitem"
			class="w-full flex items-center gap-3 px-4 h-10 text-sm text-left hover:bg-surface-2 focus:bg-surface-2
				focus:outline-none {item.danger ? 'text-danger' : 'text-text'}"
			onclick={() => {
				onclose();
				item.run();
			}}
		>
			{#if item.icon}<item.icon size={18} />{/if}
			{item.label}
		</button>
	{/each}
</div>
