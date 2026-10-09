<script lang="ts">
	import { api, errorMessage } from '$lib/api';
	import Button from '$lib/components/Button.svelte';
	import GameItemPicker from '$lib/components/guild/GameItemPicker.svelte';
	import type { GameItemSummary, LootPriority } from '$lib/types';

	/**
	 * Adds a line to the loot plan under a boss (or none): for an item, picked from the item mirror
	 * or named (a Forever newcomer), or for a kind of item ("caster trinket").
	 */
	let {
		zone,
		bossId,
		onadded
	}: { zone: string; bossId: number | null; onadded: (priority: LootPriority) => void } = $props();

	let kind = $state<'item' | 'label'>('item');
	let name = $state('');
	let picked = $state<GameItemSummary | null>(null);
	let label = $state('');
	let saving = $state(false);
	let error = $state('');

	async function submit(event: SubmitEvent) {
		event.preventDefault();
		saving = true;
		error = '';
		const where = { zone, boss_id: bossId };
		try {
			const priority = await api.post<LootPriority>(
				'/api/loot-priorities',
				kind === 'label'
					? { ...where, label }
					: picked
						? { ...where, game_item_id: picked.id }
						: { ...where, item_name: name }
			);
			name = '';
			picked = null;
			label = '';
			onadded(priority);
		} catch (err) {
			error = errorMessage(err, 'Adding it failed. Please try again.');
		} finally {
			saving = false;
		}
	}
</script>

<form onsubmit={submit}>
	<select aria-label="Add an item or a kind of item" bind:value={kind}>
		<option value="item">Item</option>
		<option value="label">Kind of item</option>
	</select>
	<div class="field">
		{#if kind === 'item'}
			<GameItemPicker bind:name bind:selected={picked} label="Item" placeholder="Item name" />
		{:else}
			<input
				type="text"
				aria-label="Kind of item"
				placeholder="Caster trinket"
				maxlength="64"
				required
				bind:value={label}
			/>
		{/if}
	</div>
	<Button size="small" type="submit" disabled={saving}>Add</Button>
	{#if error}<span class="error">{error}</span>{/if}
</form>

<style>
	form {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-2);
	}

	form select {
		width: auto;
	}

	.field {
		flex: 1;
		min-width: 12rem;
		max-width: 24rem;
	}

	.error {
		color: var(--red-text);
		font-size: var(--text-sm);
	}
</style>
