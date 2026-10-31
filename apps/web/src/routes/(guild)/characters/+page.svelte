<script lang="ts">
	import { classIcon, raceIcon } from '$lib/wow/icons';
	import Button from '$lib/components/Button.svelte';
	import CharacterForm from '$lib/components/guild/CharacterForm.svelte';
	import CharacterLink from '$lib/components/guild/CharacterLink.svelte';
	import CharacterSpecs from '$lib/components/guild/CharacterSpecs.svelte';
	import type { GuildCharacter } from '$lib/types';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();

	// svelte-ignore state_referenced_locally
	let characters = $state<GuildCharacter[]>(data.characters);
	let query = $state('');
	let classFilter = $state('');
	let adding = $state(false);

	// Like the guild list in game: a column to sort by, clicked again to turn it around.
	type Column = 'name' | 'race' | 'class' | 'spec' | 'player' | 'main';
	const COLUMNS: { key: Column; label: string }[] = [
		{ key: 'name', label: 'Name' },
		{ key: 'race', label: 'Race' },
		{ key: 'class', label: 'Class' },
		{ key: 'spec', label: 'Specs' },
		{ key: 'player', label: 'Player' },
		{ key: 'main', label: 'Main' }
	];
	let sort = $state<{ column: Column; descending: boolean }>({ column: 'name', descending: false });

	function sortBy(column: Column) {
		sort =
			sort.column === column
				? { column, descending: !sort.descending }
				: { column, descending: false };
	}

	const fullName = (c: GuildCharacter) => `${c.first_name} ${c.last_name}`;
	const findClass = (slug: string) => data.classes.find((c) => c.slug === slug);
	const findRace = (slug: string | null) => data.races.find((r) => r.slug === slug);
	const mainSpec = (c: GuildCharacter) => {
		const spec = c.specs.find((s) => s.is_main) ?? c.specs[0];
		return spec && findClass(c.class)?.specs.find((s) => s.slug === spec.spec)?.name;
	};

	// What each column sorts on; an empty value (no race, no player) goes last either way.
	const sortKey: Record<Column, (c: GuildCharacter) => string | undefined> = {
		name: fullName,
		race: (c) => findRace(c.race)?.name,
		class: (c) => findClass(c.class)?.name ?? c.class,
		spec: mainSpec,
		player: (c) => c.username ?? undefined,
		// Mains first.
		main: (c) => (c.is_main ? '0' : '1')
	};

	const shown = $derived.by(() => {
		const key = sortKey[sort.column];
		const direction = sort.descending ? -1 : 1;
		return characters
			.filter(
				(c) =>
					(!classFilter || c.class === classFilter) &&
					`${fullName(c)} ${c.username ?? ''}`.toLowerCase().includes(query.trim().toLowerCase())
			)
			.sort((a, b) => {
				const [x, y] = [key(a), key(b)];
				if (x !== y) {
					if (x === undefined) return 1;
					if (y === undefined) return -1;
					const order = x.localeCompare(y);
					if (order !== 0) return order * direction;
				}
				return fullName(a).localeCompare(fullName(b));
			});
	});

	function added(character: GuildCharacter) {
		characters = [...characters, character];
		adding = false;
	}
</script>

<svelte:head>
	<title>Characters | Blood Legion</title>
</svelte:head>

<div class="page-head">
	<div>
		<h1>Characters</h1>
	</div>
	{#if data.officer && !adding}
		<Button variant="primary" onclick={() => (adding = true)}>Add a character</Button>
	{/if}
</div>

{#if adding}
	<CharacterForm owners={data.owners} onsaved={added} oncancel={() => (adding = false)} />
{/if}

<div class="row">
	<label class="field">
		Search
		<input type="search" bind:value={query} placeholder="Name or player" />
	</label>
	<label class="field">
		Class
		<select bind:value={classFilter}>
			<option value="">Every class</option>
			{#each data.classes as wowClass (wowClass.slug)}
				<option value={wowClass.slug}>{wowClass.name}</option>
			{/each}
		</select>
	</label>
</div>

<p class="muted">{shown.length} of {characters.length}</p>

<div class="scroll">
	<table>
		<thead>
			<tr>
				{#each COLUMNS as column (column.key)}
					<th
						aria-sort={sort.column === column.key
							? sort.descending
								? 'descending'
								: 'ascending'
							: undefined}
					>
						<button type="button" onclick={() => sortBy(column.key)}>
							{column.label}
							<span class="arrow" aria-hidden="true">
								{sort.column === column.key ? (sort.descending ? '▼' : '▲') : ''}
							</span>
						</button>
					</th>
				{/each}
			</tr>
		</thead>
		<tbody>
			{#each shown as character (character.id)}
				{@const race = findRace(character.race)}
				{@const wowClass = findClass(character.class)}
				<tr>
					<td>
						<CharacterLink
							id={character.id}
							firstName={character.first_name}
							lastName={character.last_name}
							cls={character.class}
							icon={false}
						/>
					</td>
					<td>
						{#if race}
							<span class="with-icon">
								<img src={raceIcon(race.slug, character.sex)} alt="" width="20" height="20" />
								{race.name}
							</span>
						{:else}
							<span class="muted">—</span>
						{/if}
					</td>
					<td>
						<span class="with-icon" style:color={wowClass?.color}>
							<img src={classIcon(character.class)} alt="" width="20" height="20" />
							{wowClass?.name ?? character.class}
						</span>
					</td>
					<td>
						<CharacterSpecs cls={character.class} specs={character.specs} size={20} />
					</td>
					<td>
						{#if character.username}
							{character.username}
						{:else}
							<span class="muted">—</span>
						{/if}
					</td>
					<td>{character.is_main ? 'Main' : 'Alt'}</td>
				</tr>
			{:else}
				<tr><td colspan={COLUMNS.length} class="muted">No characters match.</td></tr>
			{/each}
		</tbody>
	</table>
</div>

<style>
	/* On a phone the table scrolls sideways rather than squeezing its columns. */
	.scroll {
		overflow-x: auto;
	}

	table {
		width: 100%;
		border-collapse: collapse;
		font-size: var(--text-md);
		white-space: nowrap;
	}

	th {
		padding: 0;
		text-align: left;
		border-bottom: 1px solid var(--grey-surface);
	}

	/* The whole heading is the button, so it sorts wherever it is clicked. */
	th button {
		display: flex;
		align-items: center;
		gap: var(--space-1);
		width: 100%;
		padding: var(--space-2) var(--space-3);
		color: var(--grey-text);
		font: inherit;
		font-size: var(--text-xs);
		font-weight: 600;
		letter-spacing: 0.06em;
		text-transform: uppercase;
		background: none;
		border: 0;
		cursor: pointer;
	}

	th button:hover,
	th[aria-sort] button {
		color: var(--foreground);
	}

	.arrow {
		font-size: 0.6rem;
	}

	td {
		padding: var(--space-2) var(--space-3);
		border-bottom: 1px solid var(--grey-surface);
		vertical-align: middle;
	}

	tbody tr:hover td {
		background-color: var(--grey-bg);
	}

	.with-icon {
		display: inline-flex;
		align-items: center;
		gap: var(--space-2);
	}

	img {
		display: block;
		border-radius: var(--radius-sm);
	}
</style>
