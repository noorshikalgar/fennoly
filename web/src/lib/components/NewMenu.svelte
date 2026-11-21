<script lang="ts">
	// The "New" button: upload files, upload a folder, or create a folder in the
	// current folder (or in My files when no folder is open).
	import { FolderPlus, FolderUp, Plus, Upload } from '@lucide/svelte';
	import { goto } from '$app/navigation';
	import { newFolder } from '$lib/actions';
	import { t } from '$lib/i18n/index.svelte';
	import { folderChanged, session } from '$lib/stores.svelte';
	import { ui } from '$lib/ui.svelte';
	import { enqueue, enqueueTree, filesFromFolderInput } from '$lib/uploads.svelte';
	import Menu, { type MenuItem } from './Menu.svelte';

	let { variant = 'sidebar' }: { variant?: 'sidebar' | 'fab' } = $props();

	let fileInput: HTMLInputElement;
	let folderInput: HTMLInputElement;
	let menu = $state<{ x: number; y: number } | null>(null);

	const target = $derived(ui.folder?.writable ? ui.folder.id : session.me?.home_id ?? null);

	const items: MenuItem[] = $derived([
		{ label: t('files.uploadFiles'), icon: Upload, run: () => fileInput.click() },
		{ label: t('files.uploadFolder'), icon: FolderUp, run: () => folderInput.click() },
		{
			label: t('common.newFolder'),
			icon: FolderPlus,
			separatorBefore: true,
			run: async () => {
				if (!target) return;
				const f = await newFolder(target);
				if (f) {
					folderChanged(target);
					if (!ui.folder) goto(`/files/${target}`);
				}
			}
		}
	]);

	function open(e: MouseEvent) {
		const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
		menu = variant === 'fab' ? { x: r.right - 220, y: r.top - 150 } : { x: r.left, y: r.bottom + 6 };
	}

	function picked(input: HTMLInputElement, folder: boolean) {
		if (!target || !input.files?.length) return;
		if (folder) enqueueTree(filesFromFolderInput(input.files), target);
		else enqueue(Array.from(input.files), target);
		input.value = '';
	}
</script>

<input bind:this={fileInput} type="file" multiple hidden onchange={() => picked(fileInput, false)} />
<input bind:this={folderInput} type="file" webkitdirectory hidden onchange={() => picked(folderInput, true)} />

{#if variant === 'fab'}
	<button
		class="size-14 rounded-2xl bg-primary text-on-primary shadow-xl shadow-primary/30 flex items-center justify-center active:scale-95 transition-transform"
		aria-label={t('common.upload')}
		onclick={open}
	>
		<Plus size={26} />
	</button>
{:else}
	<button class="btn-primary h-12 px-5 rounded-2xl shadow-lg shadow-primary/20 text-[0.95rem]" onclick={open}>
		<Plus size={20} />{t('common.upload')}
	</button>
{/if}

{#if menu}
	<Menu x={menu.x} y={menu.y} {items} onclose={() => (menu = null)} />
{/if}
