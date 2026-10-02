<script lang="ts">
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { api, errorMessage } from '$lib/api';
	import { QUALITY_COLOR, QUALITY_LABEL } from '$lib/guild';
	import Button from '$lib/components/Button.svelte';
	import Alert from '$lib/components/Alert.svelte';
	import LootTable from '$lib/components/guild/LootTable.svelte';
	import ItemTooltip from '$lib/components/guild/ItemTooltip.svelte';
	import ItemForm from '../ItemForm.svelte';
	import type { Item } from '$lib/types';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();

	// Edited in place, and reset when the page moves to another item.
	let item = $derived<Item>(data.detail.item);
	let editing = $state(false);
	let error = $state('');

	async function remove() {
		if (!confirm(`Remove ${item.name}?`)) return;
		error = '';
		try {
			await api.del(`/api/items/${item.id}`);
			await goto(resolve('/items'));
		} catch (err) {
			error = errorMessage(err, 'Removing failed. Please try again.');
		}
	}
</script>

<svelte:head>
	<title>{item.name} | Blood Legion</title>
</svelte:head>

<div class="page-head">
	<div>
		<p class="kicker">
			{QUALITY_LABEL[item.quality]}{item.game_item_id ? ` · Item ${item.game_item_id}` : ''}
		</p>
		<h1 style:color={QUALITY_COLOR[item.quality]}>{item.name}</h1>
		<p class="muted">Won {item.drops} {item.drops === 1 ? 'time' : 'times'}</p>
	</div>
	{#if data.officer && !editing}
		<div class="actions">
			<Button variant="secondary" size="small" onclick={() => (editing = true)}>Edit</Button>
			{#if item.drops === 0}
				<Button variant="danger" size="small" onclick={remove}>Remove</Button>
			{/if}
		</div>
	{/if}
</div>

{#if error}<Alert variant="error">{error}</Alert>{/if}

{#if data.gameItem?.preview}
	<section>
		<ItemTooltip
			preview={data.gameItem.preview}
			name={item.name}
			quality={data.gameItem.quality}
			icon={data.gameItem.icon}
		/>
		<p class="muted small">
			From the game's item database (Classic Era until WoW: Forever's is available).
		</p>
	</section>
{/if}

{#if editing}
	<ItemForm
		{item}
		onsaved={(saved) => {
			item = saved;
			editing = false;
		}}
		oncancel={() => (editing = false)}
	/>
{/if}

<section>
	<h2>Won by</h2>
	<LootTable loot={data.detail.loot} empty="Nobody yet." />
</section>

<style>
	.small {
		font-size: var(--text-sm);
	}

	.actions {
		display: flex;
		gap: var(--space-2);
	}
</style>
