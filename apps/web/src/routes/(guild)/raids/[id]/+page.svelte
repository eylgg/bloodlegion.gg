<script lang="ts">
	import { goto, invalidateAll } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { api, errorMessage } from '$lib/api';
	import { QUALITIES, QUALITY_LABEL, formatDateTime, fullName, zoneName } from '$lib/guild';
	import Button from '$lib/components/Button.svelte';
	import Alert from '$lib/components/Alert.svelte';
	import CharacterLink from '$lib/components/guild/CharacterLink.svelte';
	import LootTable from '$lib/components/guild/LootTable.svelte';
	import RaidForm from '../RaidForm.svelte';
	import type { Attendee, LootEntry, Quality, Raid } from '$lib/types';
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
	const classColor = (slug: string) => data.classes.find((c) => c.slug === slug)?.color;

	// Attendance by class, in the catalog's order.
	const byClass = $derived(
		data.classes
			.map((wowClass) => ({
				wowClass,
				attendees: attendees.filter((a) => a.class === wowClass.slug)
			}))
			.filter((group) => group.attendees.length > 0)
	);

	async function removeRaid() {
		if (!confirm('Delete this raid, with its attendance and loot?')) return;
		try {
			await api.del(`/api/raids/${raid.id}`);
			await goto(resolve('/raids'));
		} catch (err) {
			error = errorMessage(err, 'Deleting failed. Please try again.');
		}
	}

	/* --- attendance --- */

	let adding = $state(false);
	let search = $state('');
	let picked = $state<number[]>([]);

	const attending = $derived(new Set(attendees.map((a) => a.character_id)));
	const candidates = $derived(
		data.characters
			.filter((c) => !attending.has(c.id))
			.filter((c) =>
				`${fullName(c)} ${c.username ?? ''}`.toLowerCase().includes(search.trim().toLowerCase())
			)
			.sort(
				(a, b) => Number(b.is_main) - Number(a.is_main) || fullName(a).localeCompare(fullName(b))
			)
	);
	const seatsLeft = $derived(size - attendees.length);

	function toggle(id: number) {
		picked = picked.includes(id) ? picked.filter((p) => p !== id) : [...picked, id];
	}

	async function addAttendees() {
		error = '';
		try {
			attendees = await api.post<Attendee[]>(`/api/raids/${raid.id}/attendees`, {
				character_ids: picked
			});
			raid = { ...raid, attendee_count: attendees.length };
			picked = [];
			search = '';
			adding = false;
		} catch (err) {
			error = errorMessage(err, 'Adding failed. Please try again.');
		}
	}

	async function removeAttendee(attendee: Attendee) {
		error = '';
		try {
			await api.del(`/api/raids/${raid.id}/attendees/${attendee.character_id}`);
			attendees = attendees.filter((a) => a.character_id !== attendee.character_id);
			raid = { ...raid, attendee_count: attendees.length };
		} catch (err) {
			error = errorMessage(err, 'Removing failed. Please try again.');
		}
	}

	/* --- loot --- */

	// The zone's first boss to start with; trash is the last choice.
	// svelte-ignore state_referenced_locally
	let bossId = $state(String(zoneBosses[0]?.id ?? ''));
	let itemName = $state('');
	let quality = $state<Quality>('epic');
	let gameItemId = $state('');
	let winner = $state('');
	let recording = $state(false);

	// A typed name that matches a known item reuses it; anything else creates a new one.
	const knownItem = $derived(
		data.items.find((i) => i.name.toLowerCase() === itemName.trim().toLowerCase())
	);
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
				...(knownItem
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
		<p class="kicker">{formatDateTime(raid.starts_at)}</p>
		<h1>{zoneName(data.zones, raid.zone)}{raid.title ? ` · ${raid.title}` : ''}</h1>
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
	<div class="section-head">
		<h2>Characters</h2>
		{#if data.officer && !adding}
			<Button
				size="small"
				variant="secondary"
				disabled={seatsLeft <= 0}
				onclick={() => (adding = true)}
			>
				{seatsLeft <= 0 ? 'Full' : 'Add characters'}
			</Button>
		{/if}
	</div>

	{#if adding}
		<div class="panel">
			<div class="row">
				<label class="field">
					Find characters
					<input type="search" bind:value={search} placeholder="Name or player" />
				</label>
				<div class="fit add-actions">
					<span class="muted" class:over={picked.length > seatsLeft}>
						{picked.length} picked · {seatsLeft} seats left
					</span>
					<Button variant="secondary" onclick={() => ((adding = false), (picked = []))}
						>Cancel</Button
					>
					<Button
						variant="primary"
						disabled={picked.length === 0 || picked.length > seatsLeft}
						onclick={addAttendees}
					>
						Add {picked.length || ''}
					</Button>
				</div>
			</div>
			<ul class="candidates">
				{#each candidates as character (character.id)}
					<li>
						<label style:color={classColor(character.class)}>
							<input
								type="checkbox"
								checked={picked.includes(character.id)}
								onchange={() => toggle(character.id)}
							/>
							{fullName(character)}
							<span class="muted small"
								>{character.username ?? 'pug'}{character.is_main ? '' : ' · alt'}</span
							>
						</label>
					</li>
				{:else}
					<li class="muted">Nobody else to add.</li>
				{/each}
			</ul>
		</div>
	{/if}

	{#if attendees.length === 0}
		<p class="muted">Nobody recorded yet.</p>
	{:else}
		<div class="classes">
			{#each byClass as group (group.wowClass.slug)}
				<div class="class-group" style:--class-color={group.wowClass.color}>
					<h3>{group.wowClass.name} <span class="muted">{group.attendees.length}</span></h3>
					<ul>
						{#each group.attendees as attendee (attendee.character_id)}
							<li>
								<CharacterLink
									id={attendee.character_id}
									firstName={attendee.first_name}
									lastName={attendee.last_name}
									cls={attendee.class}
									icon={false}
								/>
								{#if data.officer}
									<button
										type="button"
										class="remove"
										aria-label="Remove {attendee.first_name} {attendee.last_name}"
										onclick={() => removeAttendee(attendee)}>×</button
									>
								{/if}
							</li>
						{/each}
					</ul>
				</div>
			{/each}
		</div>
	{/if}
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
					<input type="text" list="known-items" bind:value={itemName} maxlength="128" required />
					<datalist id="known-items">
						{#each data.items as item (item.id)}
							<option value={item.name}></option>
						{/each}
					</datalist>
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
			{#if itemName.trim() && !knownItem}
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
	.actions,
	.add-actions {
		display: flex;
		align-items: center;
		gap: var(--space-2);
	}

	.section-head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-3);
	}

	h3 {
		margin: 0;
		font-size: var(--text-lg);
	}

	.small {
		font-size: var(--text-sm);
	}

	.over {
		color: var(--red-text);
	}

	.candidates {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(14rem, 1fr));
		gap: var(--space-1) var(--space-3);
		max-height: 20rem;
		overflow-y: auto;
		margin: 0;
		padding: 0;
		list-style: none;
	}

	.candidates label {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		font-weight: 600;
		cursor: pointer;
	}

	.classes {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(13rem, 1fr));
		gap: var(--space-3);
	}

	.class-group {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		padding: var(--space-3);
		background-color: var(--grey-bg);
		border: 1px solid var(--grey-surface);
		border-top: 2px solid var(--class-color);
		border-radius: var(--radius-lg);
	}

	.class-group h3 {
		color: var(--class-color);
		font-size: var(--text-md);
	}

	.class-group ul {
		display: flex;
		flex-direction: column;
		gap: 2px;
		margin: 0;
		padding: 0;
		list-style: none;
	}

	.class-group li {
		display: flex;
		align-items: center;
		justify-content: space-between;
	}

	.remove {
		padding: 0 var(--space-1);
		color: var(--grey-text);
		font-size: var(--text-lg);
		line-height: 1;
		background: none;
		border: 0;
		cursor: pointer;
	}

	.remove:hover {
		color: var(--red-text);
	}
</style>
