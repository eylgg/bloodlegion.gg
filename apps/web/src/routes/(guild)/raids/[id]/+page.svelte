<script lang="ts">
	import { goto, invalidateAll } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { api, errorMessage } from '$lib/api';
	import { QUALITIES, QUALITY_LABEL, formatInZone, fullName, zoneName } from '$lib/guild';
	import Button from '$lib/components/Button.svelte';
	import Alert from '$lib/components/Alert.svelte';
	import LootTable from '$lib/components/guild/LootTable.svelte';
	import GameItemPicker from '$lib/components/guild/GameItemPicker.svelte';
	import RaidBuilder from '$lib/components/guild/RaidBuilder.svelte';
	import Roster from '$lib/components/guild/Roster.svelte';
	import { putOn, takeOff, type Target } from '$lib/wow/planning';
	import RaidForm from '../RaidForm.svelte';
	import type { Attendee, GameItemSummary, LootEntry, Quality, Raid, RaidDetail } from '$lib/types';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();

	// Edited in place after each change, and reset when the page moves to another raid (the
	// component is reused): writable deriveds do both.
	let raid = $derived<Raid>(data.detail.raid);
	let attendees = $derived<Attendee[]>(data.detail.attendees);
	let loot = $derived<LootEntry[]>(data.detail.loot);
	let error = $state('');
	let editing = $state(false);

	const zone = $derived(data.zones.find((z) => z.slug === raid.zone));
	const size = $derived(zone?.size ?? 0);
	const zoneBosses = $derived(data.bosses.filter((b) => b.zone === raid.zone));

	async function removeRaid() {
		if (!confirm('Delete this raid, with its attendance and loot?')) return;
		try {
			await api.del(`/api/raids/${raid.id}`);
			await goto(resolve('/raids'));
		} catch (err) {
			error = errorMessage(err, 'Deleting failed. Please try again.');
		}
	}

	/* --- groups --- */

	// The roster's pick, to put down in a free slot.
	let selected = $state<number | null>(null);
	const detail = $derived<RaidDetail>({ raid, attendees, loot });
	const places = (characterId: number) => {
		const a = attendees.find((a) => a.character_id === characterId);
		return a
			? [{ raidId: raid.id, name: zoneName(data.zones, raid.zone), group: a.group_number }]
			: [];
	};

	async function add(characterId: number, target: Target) {
		const character = data.characters.find((c) => c.id === characterId);
		if (!character) return;
		error = '';
		try {
			const next = await putOn(
				[detail],
				detail,
				character,
				target,
				size,
				(message) => confirm(message),
				() => zoneName(data.zones, raid.zone)
			);
			if (next) setAttendees(next[0].attendees);
		} catch (err) {
			error = err instanceof Error ? errorMessage(err, err.message) : 'Adding them failed.';
		} finally {
			selected = null;
		}
	}

	async function remove(characterId: number) {
		error = '';
		try {
			setAttendees((await takeOff([detail], raid.id, characterId))[0].attendees);
		} catch (err) {
			error = errorMessage(err, 'Taking them off failed. Please try again.');
		}
	}

	function setAttendees(next: Attendee[]) {
		attendees = next;
		raid = { ...raid, attendee_count: next.length };
	}

	/* --- loot --- */

	// The zone's first boss to start with; trash is the last choice.
	// svelte-ignore state_referenced_locally
	let bossId = $state(String(zoneBosses[0]?.id ?? ''));
	let itemName = $state('');
	// The item mirror's item, when one was picked from the suggestions.
	let gameItem = $state<GameItemSummary | null>(null);
	let quality = $state<Quality>('epic');
	let gameItemId = $state('');
	let winner = $state('');
	let recording = $state(false);

	// A picked mirror item is used as is. A typed name that matches one of the guild's items
	// reuses it; the server also matches it against the mirror. Only a name neither knows needs a
	// quality.
	const knownItem = $derived(
		data.items.find((i) => i.name.toLowerCase() === itemName.trim().toLowerCase())
	);
	const attending = $derived(new Set(attendees.map((a) => a.character_id)));
	const others = $derived(
		data.characters
			.filter((c) => !attending.has(c.id))
			.sort((a, b) => fullName(a).localeCompare(fullName(b)))
	);

	async function recordLoot(event: SubmitEvent) {
		event.preventDefault();
		if (!itemName.trim()) {
			error = 'Name the item.';
			return;
		}
		recording = true;
		error = '';
		try {
			await api.post<LootEntry>(`/api/raids/${raid.id}/loot`, {
				boss_id: bossId ? Number(bossId) : null,
				character_id: winner ? Number(winner) : null,
				...(gameItem
					? { game_item_id: gameItem.id }
					: knownItem
						? { item_id: knownItem.id }
						: {
								item_name: itemName.trim(),
								item_quality: quality,
								game_item_id: gameItemId ? Number(gameItemId) : null
							})
			});
			// Refetched rather than patched: the loot keeps the server's order, and a new item joins
			// the picker.
			itemName = '';
			gameItem = null;
			gameItemId = '';
			winner = '';
			await invalidateAll();
		} catch (err) {
			error = errorMessage(err, 'Recording the loot failed. Please try again.');
		} finally {
			recording = false;
		}
	}

	async function removeLoot(entry: LootEntry) {
		if (!confirm(`Remove ${entry.item_name} from this raid's loot?`)) return;
		error = '';
		try {
			await api.del(`/api/loot/${entry.id}`);
			loot = loot.filter((l) => l.id !== entry.id);
			raid = { ...raid, loot_count: loot.length };
		} catch (err) {
			error = errorMessage(err, 'Removing failed. Please try again.');
		}
	}
</script>

<svelte:head>
	<title>{zoneName(data.zones, raid.zone)} | Blood Legion</title>
</svelte:head>

<div class="page-head">
	<div>
		<p class="kicker">
			{raid.week ? `Week ${raid.week.number} · ` : ''}{formatInZone(raid.starts_at, raid.time_zone)}
		</p>
		<h1>{zoneName(data.zones, raid.zone)}</h1>
		<p class="muted">
			{attendees.length}/{size} characters · {loot.length}
			{loot.length === 1 ? 'item' : 'items'}
		</p>
	</div>
	{#if data.officer && !editing}
		<div class="actions">
			<Button variant="secondary" size="small" onclick={() => (editing = true)}>Edit</Button>
			<Button variant="danger" size="small" onclick={removeRaid}>Delete</Button>
		</div>
	{/if}
</div>

{#if error}<Alert variant="error">{error}</Alert>{/if}

{#if editing}
	<RaidForm
		{raid}
		onsaved={(saved) => {
			raid = saved;
			editing = false;
		}}
		oncancel={() => (editing = false)}
	/>
{/if}

<section>
	<h2>Groups</h2>
	<div class="groups-layout" class:with-roster={data.officer}>
		{#if data.officer}
			<Roster
				characters={data.characters}
				{places}
				editable
				bind:selected
				onreturn={(_, characterId) => remove(characterId)}
			/>
		{/if}
		<RaidBuilder
			raidId={raid.id}
			{size}
			{attendees}
			onchange={setAttendees}
			effects={data.effects}
			editable={data.officer}
			onremove={(attendee) => remove(attendee.character_id)}
			incoming={selected}
			onincoming={add}
		/>
	</div>
</section>

<section>
	<h2>Loot</h2>

	{#if data.officer}
		<form class="panel" onsubmit={recordLoot}>
			<h3>Record loot</h3>
			{#if zoneBosses.length === 0}
				<p class="muted small">
					No bosses entered for {zone?.name ?? 'this raid'} yet; add them on the
					<a href={resolve('/bosses')}>bosses page</a>. Until then, loot can be recorded as trash.
				</p>
			{/if}
			<div class="row">
				<label class="field">
					From
					<select bind:value={bossId}>
						{#each zoneBosses as boss (boss.id)}
							<option value={String(boss.id)}>{boss.name}</option>
						{/each}
						<option value="">Trash</option>
					</select>
				</label>
				<label class="field">
					Item
					<GameItemPicker bind:name={itemName} bind:selected={gameItem} />
				</label>
				<label class="field">
					Won by
					<select bind:value={winner}>
						<option value="">Nobody (disenchanted or banked)</option>
						<optgroup label="In this raid">
							{#each attendees as attendee (attendee.character_id)}
								<option value={String(attendee.character_id)}>{fullName(attendee)}</option>
							{/each}
						</optgroup>
						<optgroup label="Everyone else">
							{#each others as character (character.id)}
								<option value={String(character.id)}>{fullName(character)}</option>
							{/each}
						</optgroup>
					</select>
				</label>
			</div>
			{#if itemName.trim() && !gameItem && !knownItem}
				<div class="row">
					<label class="field">
						Quality (a new item)
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
			{/if}
			<div class="form-actions">
				<Button type="submit" variant="primary" disabled={recording}>
					{recording ? 'Recording...' : 'Record'}
				</Button>
			</div>
		</form>
	{/if}

	<LootTable
		{loot}
		raid={false}
		empty="No loot recorded."
		onremove={data.officer ? removeLoot : undefined}
	/>
</section>

<style>
	.groups-layout.with-roster {
		display: grid;
		grid-template-columns: 16rem minmax(0, 1fr);
		gap: var(--space-4);
		align-items: start;
	}

	@media (max-width: 52rem) {
		.groups-layout.with-roster {
			grid-template-columns: 1fr;
		}
	}

	.actions,
	h3 {
		margin: 0;
		font-size: var(--text-lg);
	}

	.small {
		font-size: var(--text-sm);
	}
</style>
