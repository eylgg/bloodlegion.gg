<script lang="ts">
	import { page } from '$app/state';
	import { api, errorMessage } from '$lib/api';
	import { fromLocalInput, toLocalInput } from '$lib/guild';
	import Button from '$lib/components/Button.svelte';
	import Alert from '$lib/components/Alert.svelte';
	import type { Raid, Zone } from '$lib/types';

	/** Schedules a raid, or edits one: its zone, an optional title, and when it starts. */
	let {
		raid = null,
		onsaved,
		oncancel
	}: { raid?: Raid | null; onsaved: (raid: Raid) => void; oncancel: () => void } = $props();

	const zones = $derived((page.data.zones as Zone[] | undefined) ?? []);

	// A new raid defaults to tonight at eight, local time.
	function tonight(): string {
		const now = new Date();
		const eight = new Date(now.getFullYear(), now.getMonth(), now.getDate(), 20);
		return toLocalInput(eight.toISOString());
	}

	// svelte-ignore state_referenced_locally
	let zone = $state(raid?.zone ?? '');
	// svelte-ignore state_referenced_locally
	let title = $state(raid?.title ?? '');
	// svelte-ignore state_referenced_locally
	let startsAt = $state(raid ? toLocalInput(raid.starts_at) : tonight());
	let saving = $state(false);
	let error = $state('');

	async function save(event: SubmitEvent) {
		event.preventDefault();
		if (!zone) {
			error = 'Pick a raid.';
			return;
		}
		saving = true;
		error = '';
		const body = { zone, title: title.trim() || null, starts_at: fromLocalInput(startsAt) };
		try {
			onsaved(
				raid
					? await api.put<Raid>(`/api/raids/${raid.id}`, body)
					: await api.post<Raid>('/api/raids', body)
			);
		} catch (err) {
			error = errorMessage(err, 'Saving failed. Please try again.');
		} finally {
			saving = false;
		}
	}
</script>

<form class="panel" onsubmit={save}>
	<h3>{raid ? 'Edit the raid' : 'Schedule a raid'}</h3>
	{#if error}<Alert variant="error">{error}</Alert>{/if}
	<div class="row">
		<label class="field">
			Raid
			<select bind:value={zone} required>
				<option value="" disabled>Pick one</option>
				{#each zones as z (z.slug)}
					<option value={z.slug}>{z.name} ({z.size} players)</option>
				{/each}
			</select>
		</label>
		<label class="field">
			Starts
			<input type="datetime-local" bind:value={startsAt} required />
		</label>
		<label class="field">
			Title (optional)
			<input type="text" bind:value={title} maxlength="64" placeholder="Group 2" />
		</label>
	</div>
	<div class="form-actions">
		<Button type="button" variant="secondary" onclick={oncancel}>Cancel</Button>
		<Button type="submit" variant="primary" disabled={saving}>
			{saving ? 'Saving...' : raid ? 'Save' : 'Schedule'}
		</Button>
	</div>
</form>

<style>
	h3 {
		margin: 0;
		font-size: var(--text-lg);
	}
</style>
