<script lang="ts">
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { api, errorMessage } from '$lib/api';
	import { fullName, raidNames, zoneName } from '$lib/guild';
	import { dragCharacter } from '$lib/wow/drag';
	import Alert from '$lib/components/Alert.svelte';
	import Button from '$lib/components/Button.svelte';
	import ItemLink from '$lib/components/guild/ItemLink.svelte';
	import Roster from '$lib/components/guild/Roster.svelte';
	import AddPriority from './AddPriority.svelte';
	import type { InLine, LootPriority } from '$lib/types';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();

	// Lines change in place as officers edit them; a new load (another zone or week) resets.
	let priorities = $derived<LootPriority[]>(data.priorities);
	let selected = $state<number | null>(null);
	let error = $state('');
	// Where a drag would land: a line, and the place in it.
	let over = $state<{ id: number; index: number } | null>(null);

	const names = $derived(raidNames(data.zones, data.weekRaids));
	const nameOf = (raidId: number) => names.get(raidId) ?? '';
	const selectedName = $derived.by(() => {
		const character = data.characters.find((c) => c.id === selected);
		return character ? fullName(character) : '';
	});

	/** The zone's lines, boss by boss, then those from no boss in particular. */
	const sections = $derived.by(() => {
		const shown = priorities.filter((p) => p.zone === data.zone);
		return [
			...data.bosses.map((b) => ({
				key: `boss-${b.id}`,
				bossId: b.id as number | null,
				title: b.name,
				rows: shown.filter((p) => p.boss_id === b.id)
			})),
			{
				key: 'other',
				bossId: null,
				title: data.bosses.length > 0 ? 'Trash and the rest' : 'Items',
				rows: shown.filter((p) => p.boss_id === null)
			}
		];
	});

	/** Where each character is placed in this zone's raids this week, for the roster's badges. */
	const places = (characterId: number) =>
		data.raids.flatMap((r) => {
			const a = r.attendees.find((a) => a.character_id === characterId);
			return a ? [{ raidId: r.raid.id, name: nameOf(r.raid.id), group: a.group_number }] : [];
		});

	const classColor = (cls: string) => data.classes.find((c) => c.slug === cls)?.color ?? '#999';

	/** Dark text on a light class color (a priest's white), light text on a dark one. */
	function textOn(hex: string) {
		const [r, g, b] = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16) / 255);
		return 0.2126 * r + 0.7152 * g + 0.0722 * b > 0.45 ? '#111' : '#fff';
	}

	type Place = { character_id: number; note: string | null };

	const line = (priority: LootPriority): Place[] =>
		priority.characters.map(({ character_id, note }) => ({ character_id, note }));

	// Edits save one at a time, each starting from the line as the last one left it, so two quick
	// edits cannot undo each other.
	let saving: Promise<void> = Promise.resolve();

	function edit(id: number, change: (line: Place[]) => Place[]) {
		saving = saving.then(async () => {
			const current = priorities.find((p) => p.id === id);
			if (!current) return;
			error = '';
			try {
				const saved = await api.put<LootPriority>(`/api/loot-priorities/${id}/characters`, {
					characters: change(line(current))
				});
				priorities = priorities.map((p) => (p.id === saved.id ? saved : p));
			} catch (err) {
				error = errorMessage(err, 'Saving the line failed. Please try again.');
			}
		});
	}

	/** Puts a character at `index` in a line: moved there when already in it, else added. */
	function placeIn(priority: LootPriority, characterId: number, index: number) {
		edit(priority.id, (next) => {
			const from = next.findIndex((c) => c.character_id === characterId);
			let entry: Place = { character_id: characterId, note: null };
			let at = index;
			if (from >= 0) {
				[entry] = next.splice(from, 1);
				if (from < at) at--;
			}
			next.splice(Math.min(at, next.length), 0, entry);
			return next;
		});
	}

	function remove(priority: LootPriority, characterId: number) {
		edit(priority.id, (next) => next.filter((c) => c.character_id !== characterId));
	}

	function editNote(priority: LootPriority, character: InLine) {
		const note = prompt(
			`A note for ${fullName(character)} (what they want it for), or nothing:`,
			character.note ?? ''
		);
		if (note === null) return;
		edit(priority.id, (next) =>
			next.map((c) =>
				c.character_id === character.character_id ? { ...c, note: note.trim() || null } : c
			)
		);
	}

	function addSelected(priority: LootPriority) {
		if (selected === null) return;
		placeIn(priority, selected, Infinity);
		selected = null;
	}

	function dragOver(event: DragEvent, id: number, index: number) {
		event.preventDefault();
		event.stopPropagation();
		over = { id, index };
	}

	function drop(event: DragEvent, priority: LootPriority, index: number) {
		event.preventDefault();
		event.stopPropagation();
		over = null;
		const characterId = Number(event.dataTransfer?.getData('text/plain'));
		if (characterId > 0) placeIn(priority, characterId, index);
	}

	async function deletePriority(priority: LootPriority) {
		const what = priority.item_name ?? priority.label;
		if (priority.characters.length > 0 && !confirm(`Delete the line for ${what}?`)) return;
		error = '';
		try {
			await api.del(`/api/loot-priorities/${priority.id}`);
			priorities = priorities.filter((p) => p.id !== priority.id);
		} catch (err) {
			error = errorMessage(err, 'Deleting the line failed. Please try again.');
		}
	}

	async function moveTo(priority: LootPriority, bossId: number | null) {
		error = '';
		try {
			const saved = await api.put<LootPriority>(`/api/loot-priorities/${priority.id}`, {
				boss_id: bossId
			});
			priorities = priorities.map((p) => (p.id === saved.id ? saved : p));
		} catch (err) {
			error = errorMessage(err, 'Moving the line failed. Please try again.');
		}
	}

	/** Those in line who are on `raid`, with their place in line; the first without it is next. */
	function inRaid(priority: LootPriority, raidId: number) {
		const raid = data.raids.find((r) => r.raid.id === raidId);
		const here = priority.characters
			.map((character, i) => ({ character, place: i + 1 }))
			.filter(({ character }) =>
				raid?.attendees.some((a) => a.character_id === character.character_id)
			);
		const next = here.find(({ character }) => !character.received);
		return here.map((entry) => ({ ...entry, next: entry === next }));
	}

	function go(zone: string, week: number) {
		// eslint-disable-next-line svelte/no-navigation-without-resolve -- same page, new query
		goto(`${resolve('/planner/loot')}?zone=${zone}&week=${week}`);
	}
</script>

<svelte:head>
	<title>Loot plan | Blood Legion</title>
</svelte:head>

<svelte:window onkeydown={(e) => e.key === 'Escape' && (selected = null)} />

{#snippet chip(character: InLine, place: number, next = false)}
	<span
		class="chip"
		class:received={character.received}
		class:next
		style:--chip-bg={classColor(character.class)}
		style:--chip-fg={textOn(classColor(character.class))}
		title="{place}. {fullName(character)}{character.username
			? ` (${character.username})`
			: ''}{character.received ? ', has it' : ''}"
	>
		<span class="place">{place}</span>
		{fullName(character)}
		{#if character.note}<span class="note">({character.note})</span>{/if}
	</span>
{/snippet}

<div class="page-head">
	<div>
		<p class="kicker">Loot plan</p>
		<h1>{zoneName(data.zones, data.zone)} <span class="muted span">Week {data.week}</span></h1>
	</div>
	<div class="nav">
		<div class="zones" role="tablist" aria-label="Zone">
			{#each data.zones as zone (zone.slug)}
				<button
					type="button"
					role="tab"
					aria-selected={zone.slug === data.zone}
					onclick={() => go(zone.slug, data.week)}>{zone.name}</button
				>
			{/each}
		</div>
		<select
			aria-label="Week"
			value={String(data.week)}
			onchange={(e) => go(data.zone, Number(e.currentTarget.value))}
		>
			{#each data.weeks as week (week)}
				<option value={String(week)}>Week {week}</option>
			{/each}
		</select>
	</div>
</div>

{#if error}<Alert variant="error">{error}</Alert>{/if}

<div class="layout">
	<Roster
		characters={data.characters}
		{places}
		editable
		bind:selected
		hint="Drag characters into an item's line, or click one then its Add button. The badges say who is in which raid this week."
	/>

	<div class="sections">
		<p class="muted small help">
			Drag within a line to reorder, and double-click someone for a note ("dm"). Each raid column
			shows who in line is on that raid this week, in order; the outlined one is next, and those who
			have the item are struck through.
		</p>
		{#each sections as section (section.key)}
			<section>
				<h2>{section.title}</h2>
				{#if section.rows.length > 0}
					<div class="table-wrap">
						<table>
							<thead>
								<tr>
									<th class="item-col">Item</th>
									<th>In line</th>
									{#each data.raids as raid (raid.raid.id)}
										<th class="raid-col">{nameOf(raid.raid.id)}</th>
									{/each}
								</tr>
							</thead>
							<tbody>
								{#each section.rows as priority (priority.id)}
									<tr>
										<td class="item-col">
											{#if priority.item_id !== null && priority.item_name && priority.item_quality}
												<ItemLink
													id={priority.item_id}
													name={priority.item_name}
													quality={priority.item_quality}
													icon={priority.item_icon}
													gameItemId={priority.game_item_id}
												/>
											{:else}
												<span class="label">{priority.label}</span>
											{/if}
											<div class="row-actions">
												{#if data.bosses.length > 0}
													<select
														aria-label="Boss"
														value={priority.boss_id === null ? '' : String(priority.boss_id)}
														onchange={(e) =>
															moveTo(
																priority,
																e.currentTarget.value ? Number(e.currentTarget.value) : null
															)}
													>
														{#each data.bosses as boss (boss.id)}
															<option value={String(boss.id)}>{boss.name}</option>
														{/each}
														<option value="">No boss</option>
													</select>
												{/if}
												<button
													type="button"
													class="delete"
													onclick={() => deletePriority(priority)}>Delete</button
												>
											</div>
										</td>
										<td
											class="line"
											class:dropping={over?.id === priority.id}
											ondragover={(e) => dragOver(e, priority.id, priority.characters.length)}
											ondragleave={() => (over = null)}
											ondrop={(e) => drop(e, priority, priority.characters.length)}
										>
											<ol>
												{#each priority.characters as character, i (character.character_id)}
													<li
														class:insert={over?.id === priority.id && over.index === i}
														draggable="true"
														ondragstart={(e) => dragCharacter(e, character.character_id)}
														ondragover={(e) => dragOver(e, priority.id, i)}
														ondrop={(e) => drop(e, priority, i)}
														ondblclick={() => editNote(priority, character)}
													>
														{@render chip(character, i + 1)}
														<button
															type="button"
															class="x"
															title="Take {fullName(character)} out of line"
															aria-label="Take {fullName(character)} out of line"
															onclick={() => remove(priority, character.character_id)}>×</button
														>
													</li>
												{/each}
												{#if selected !== null && !priority.characters.some((c) => c.character_id === selected)}
													<li>
														<Button
															size="small"
															variant="secondary"
															onclick={() => addSelected(priority)}>Add {selectedName}</Button
														>
													</li>
												{:else if priority.characters.length === 0}
													<li class="muted small">Nobody yet</li>
												{/if}
											</ol>
										</td>
										{#each data.raids as raid (raid.raid.id)}
											<td class="raid-col">
												<ol>
													{#each inRaid(priority, raid.raid.id) as entry (entry.character.character_id)}
														<li>{@render chip(entry.character, entry.place, entry.next)}</li>
													{/each}
												</ol>
											</td>
										{/each}
									</tr>
								{/each}
							</tbody>
						</table>
					</div>
				{/if}
				<AddPriority
					zone={data.zone}
					bossId={section.bossId}
					onadded={(priority) => (priorities = [...priorities, priority])}
				/>
			</section>
		{/each}
		{#if data.raids.length === 0}
			<p class="muted small">
				No {zoneName(data.zones, data.zone)} raids in week {data.week}, so there are no raid
				columns.
				<a href={resolve('/planner')}>Plan the raids</a>
			</p>
		{/if}
	</div>
</div>

<style>
	.span {
		margin-left: var(--space-2);
		font-size: var(--text-lg);
		font-weight: 400;
	}

	.nav,
	.zones {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-2);
	}

	.nav select {
		width: auto;
	}

	.zones button {
		padding: var(--space-1) var(--space-3);
		font: inherit;
		font-size: var(--text-sm);
		background: var(--grey-bg);
		border: 1px solid var(--grey-surface);
		border-radius: var(--radius-md);
		cursor: pointer;
	}

	.zones button[aria-selected='true'] {
		border-color: var(--accent-solid);
		box-shadow: 0 0 0 1px var(--accent-solid);
	}

	.small {
		font-size: var(--text-sm);
	}

	.layout {
		display: grid;
		grid-template-columns: 17rem minmax(0, 1fr);
		gap: var(--space-4);
		align-items: start;
	}

	@media (max-width: 52rem) {
		.layout {
			grid-template-columns: 1fr;
		}
	}

	.sections {
		display: flex;
		flex-direction: column;
		gap: var(--space-6);
		min-width: 0;
	}

	.help {
		margin: 0;
	}

	section {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}

	h2 {
		font-size: var(--text-lg);
	}

	.table-wrap {
		overflow-x: auto;
	}

	table {
		width: 100%;
		border-collapse: collapse;
		/* Names are picked up whole, never text-selected (a selection would be dragged along). */
		user-select: none;
		-webkit-user-select: none;
	}

	th,
	td {
		padding: var(--space-1) var(--space-2);
		text-align: left;
		vertical-align: top;
		border-bottom: 1px solid var(--grey-surface);
	}

	th {
		color: var(--grey-text);
		font-size: var(--text-xs);
		font-weight: 600;
		white-space: nowrap;
	}

	.item-col {
		min-width: 13rem;
	}

	.raid-col {
		min-width: 9rem;
		background-color: var(--grey-bg);
	}

	/* A line's own controls show when it is pointed at or focused (always, without a pointer). */
	.row-actions {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		margin-top: 2px;
		visibility: hidden;
	}

	tr:hover .row-actions,
	tr:focus-within .row-actions {
		visibility: visible;
	}

	@media (hover: none) {
		.row-actions {
			visibility: visible;
		}
	}

	.row-actions select {
		width: auto;
		padding-block: 0;
		font-size: var(--text-xs);
	}

	.delete {
		padding: 0;
		color: var(--red-text);
		font: inherit;
		font-size: var(--text-xs);
		background: none;
		border: 0;
		cursor: pointer;
	}

	.delete:hover {
		text-decoration: underline;
	}

	.line {
		min-width: 16rem;
	}

	.line.dropping {
		outline: 1px dashed var(--accent-solid);
		outline-offset: -2px;
	}

	ol {
		display: flex;
		flex-wrap: wrap;
		gap: 3px;
		margin: 0;
		padding: 0;
		list-style: none;
	}

	.raid-col ol {
		flex-direction: column;
		align-items: flex-start;
	}

	.line li {
		display: inline-flex;
		align-items: center;
		cursor: grab;
	}

	.line li.insert {
		box-shadow: -3px 0 0 var(--accent-solid);
	}

	.chip {
		display: inline-flex;
		align-items: baseline;
		gap: 4px;
		padding: 1px 6px;
		color: var(--chip-fg);
		font-size: var(--text-sm);
		font-weight: 600;
		white-space: nowrap;
		background-color: var(--chip-bg);
		border-radius: var(--radius-sm);
	}

	.chip.received {
		opacity: 0.45;
		text-decoration: line-through;
	}

	.chip.next {
		outline: 2px solid var(--foreground);
		outline-offset: 1px;
	}

	.place {
		font-size: var(--text-xs);
		font-weight: 400;
		opacity: 0.75;
	}

	.note {
		font-weight: 400;
	}

	.x {
		margin-left: 1px;
		padding: 0 4px;
		color: var(--grey-text);
		font: inherit;
		line-height: 1;
		background: none;
		border: 0;
		cursor: pointer;
	}

	.x:hover {
		color: var(--red-text);
	}

	.label {
		font-style: italic;
		font-weight: 600;
	}
</style>
