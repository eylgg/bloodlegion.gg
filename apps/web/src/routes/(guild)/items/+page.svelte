<script lang="ts">
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { QUALITIES, QUALITY_LABEL } from '$lib/guild';
	import Button from '$lib/components/Button.svelte';
	import ItemLink from '$lib/components/guild/ItemLink.svelte';
	import ItemForm from './ItemForm.svelte';
	import type { Quality } from '$lib/types';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();

	let query = $state('');
	let quality = $state<Quality | ''>('');
	let adding = $state(false);

	const shown = $derived(
		data.items.filter(
			(i) =>
				(!quality || i.quality === quality) &&
				i.name.toLowerCase().includes(query.trim().toLowerCase())
		)
	);
</script>

<svelte:head>
	<title>Items | Blood Legion</title>
</svelte:head>

<div class="page-head">
	<div>
		<p class="kicker">The guild</p>
		<h1>Items</h1>
	</div>
	{#if data.officer && !adding}
		<Button variant="primary" onclick={() => (adding = true)}>Add an item</Button>
	{/if}
</div>

<p class="muted">Items are added as they drop: recording loot by name creates its item.</p>

{#if adding}
	<ItemForm
		onsaved={(item) => goto(resolve('/(guild)/items/[id]', { id: String(item.id) }))}
		oncancel={() => (adding = false)}
	/>
{/if}

<div class="row">
	<label class="field">
		Search
		<input type="search" bind:value={query} placeholder="Item name" />
	</label>
	<label class="field">
		Quality
		<select bind:value={quality}>
			<option value="">Any quality</option>
			{#each QUALITIES as q (q)}
				<option value={q}>{QUALITY_LABEL[q]}</option>
			{/each}
		</select>
	</label>
</div>

{#if shown.length === 0}
	<p class="muted">{data.items.length === 0 ? 'No items yet.' : 'No items match.'}</p>
{:else}
	<ul class="items">
		{#each shown as item (item.id)}
			<li>
				<ItemLink
					id={item.id}
					name={item.name}
					quality={item.quality}
					icon={item.icon}
					gameItemId={item.game_item_id}
				/>
				<span class="muted">{item.drops}×</span>
			</li>
		{/each}
	</ul>
{/if}

<style>
	.items {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(18rem, 1fr));
		gap: var(--space-2) var(--space-6);
		margin: 0;
		padding: 0;
		list-style: none;
	}

	li {
		display: flex;
		justify-content: space-between;
		gap: var(--space-2);
		padding: var(--space-1) 0;
		border-bottom: 1px solid var(--grey-surface);
	}
</style>
