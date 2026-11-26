<script lang="ts">
	// How to put Fennoly on the home screen, for the browser in hand.
	import { CircleCheck, Download, EllipsisVertical, Share, SquarePlus } from '@lucide/svelte';
	import { install, promptInstall } from '$lib/install.svelte';
	import { t } from '$lib/i18n/index.svelte';
</script>

{#if install.installed}
	<p class="flex items-center gap-2 text-sm font-medium text-success"><CircleCheck size={18} />{t('install.done')}</p>
{:else if install.canPrompt}
	<button type="button" class="btn-primary" onclick={promptInstall}><Download size={18} />{t('install.button')}</button>
{:else if install.ios}
	<ol class="space-y-2 text-sm">
		<li class="flex items-center gap-2"><span class="size-6 rounded-full bg-surface-2 grid place-items-center text-xs font-semibold">1</span>{t('install.iosShare')}<Share size={16} class="text-primary" /></li>
		<li class="flex items-center gap-2"><span class="size-6 rounded-full bg-surface-2 grid place-items-center text-xs font-semibold">2</span>{t('install.iosAdd')}<SquarePlus size={16} class="text-primary" /></li>
	</ol>
{:else if install.insecure}
	<p class="text-sm text-muted">{t('install.needsHttps')}</p>
{:else}
	<p class="flex items-center gap-1.5 text-sm text-muted">{t('install.menu')}<EllipsisVertical size={16} /></p>
{/if}
