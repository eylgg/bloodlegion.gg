<script lang="ts">
	import { QUALITY_COLOR } from '$lib/guild';
	import ItemTooltip from '$lib/components/guild/ItemTooltip.svelte';
	import type { Quality } from '$lib/types';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();

	const item = $derived(data.item);
</script>

<svelte:head>
	<title>{item.name} | Blood Legion</title>
</svelte:head>

<div class="page-head">
	<div>
		<p class="kicker">
			{item.slot}{item.item_subclass ? ` · ${item.item_subclass}` : ''} · Item level
			{item.item_level} · Item {item.id}
		</p>
		<h1 style:color={QUALITY_COLOR[item.quality as Quality] ?? '#fff'}>{item.name}</h1>
		<p class="muted">Not won by the guild yet.</p>
	</div>
</div>

{#if item.preview}
	<ItemTooltip preview={item.preview} name={item.name} quality={item.quality} icon={item.icon} />
{:else}
	<p class="muted">Its tooltip has not been synced yet.</p>
{/if}
