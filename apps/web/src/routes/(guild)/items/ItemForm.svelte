<script lang="ts">
	import { api, errorMessage } from '$lib/api';
	import { QUALITIES, QUALITY_LABEL } from '$lib/guild';
	import Button from '$lib/components/Button.svelte';
	import Alert from '$lib/components/Alert.svelte';
	import type { Item, Quality } from '$lib/types';

	/** Adds an item, or corrects one: its name, quality, and the game's item id. */
	let {
		item = null,
		onsaved,
		oncancel
	}: { item?: Item | null; onsaved: (item: Item) => void; oncancel: () => void } = $props();

	// svelte-ignore state_referenced_locally
	let name = $state(item?.name ?? '');
	// svelte-ignore state_referenced_locally
	let quality = $state<Quality>(item?.quality ?? 'epic');
	// svelte-ignore state_referenced_locally
	let gameItemId = $state(item?.game_item_id ? String(item.game_item_id) : '');
	let saving = $state(false);
	let error = $state('');

	async function save(event: SubmitEvent) {
		event.preventDefault();
		saving = true;
		error = '';
		const body = { name, quality, game_item_id: gameItemId ? Number(gameItemId) : null };
		try {
			onsaved(
				item
					? await api.put<Item>(`/api/items/${item.id}`, body)
					: await api.post<Item>('/api/items', body)
			);
		} catch (err) {
			error = errorMessage(err, 'Saving failed. Please try again.');
		} finally {
			saving = false;
		}
	}
</script>

<form class="panel" onsubmit={save}>
	<h3>{item ? 'Edit the item' : 'Add an item'}</h3>
	{#if error}<Alert variant="error">{error}</Alert>{/if}
	<div class="row">
		<label class="field">
			Name
			<input type="text" bind:value={name} maxlength="128" required />
		</label>
		<label class="field">
			Quality
			<select bind:value={quality}>
				{#each QUALITIES as q (q)}
					<option value={q}>{QUALITY_LABEL[q]}</option>
				{/each}
			</select>
		</label>
		<label class="field">
			Item id (optional)
			<input type="text" inputmode="numeric" pattern="[0-9]*" bind:value={gameItemId} />
		</label>
	</div>
	<div class="form-actions">
		<Button type="button" variant="secondary" onclick={oncancel}>Cancel</Button>
		<Button type="submit" variant="primary" disabled={saving}>
			{saving ? 'Saving...' : item ? 'Save' : 'Add'}
		</Button>
	</div>
</form>

<style>
	h3 {
		margin: 0;
		font-size: var(--text-lg);
	}
</style>
