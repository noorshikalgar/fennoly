<script lang="ts">
	import {
		File,
		FileArchive,
		FileImage,
		FileMusic,
		FilePlay,
		FileSpreadsheet,
		FileText,
		Folder,
		Presentation
	} from '@lucide/svelte';
	import { category } from '$lib/format';
	import type { FileNode } from '$lib/types';

	let { node, size = 24 }: { node: Pick<FileNode, 'kind' | 'mime' | 'name'>; size?: number } = $props();

	const styles = {
		folder: { icon: Folder, color: '#6366f1' },
		image: { icon: FileImage, color: '#ec4899' },
		pdf: { icon: FileText, color: '#ef4444' },
		video: { icon: FilePlay, color: '#8b5cf6' },
		audio: { icon: FileMusic, color: '#14b8a6' },
		doc: { icon: FileText, color: '#3b82f6' },
		sheet: { icon: FileSpreadsheet, color: '#16a34a' },
		slides: { icon: Presentation, color: '#f97316' },
		archive: { icon: FileArchive, color: '#a16207' },
		text: { icon: FileText, color: '#64748b' },
		other: { icon: File, color: '#64748b' }
	};

	const s = $derived(styles[category(node)]);
	const Icon = $derived(s.icon);
</script>

<Icon {size} color={s.color} fill={node.kind === 'folder' ? s.color : 'none'} fill-opacity={node.kind === 'folder' ? 0.18 : 0} strokeWidth={1.75} />
