<script lang="ts">
	import { classIcon, raceIcon } from '$lib/wow/icons';
	import Button from '$lib/components/Button.svelte';
	import CharacterForm from '$lib/components/guild/CharacterForm.svelte';
	import CharacterLink from '$lib/components/guild/CharacterLink.svelte';
	import CharacterSpecs from '$lib/components/guild/CharacterSpecs.svelte';
	import { RANKS, RANK_GROUP, RANK_LABEL } from '$lib/guild';
	import type { GuildCharacter, Rank } from '$lib/types';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();

	// svelte-ignore state_referenced_locally
	let characters = $state<GuildCharacter[]>(data.characters);
	let query = $state('');
	let classFilter = $state('');
	let adding = $state(false);

	// Which characters the chart and the table count: by default the raiding roster's mains. An
	// unclaimed character (a pug) has no rank, so it is never among them.
	let mains = $state(true);
	let alts = $state(false);
	let ranks = $state<Record<Rank, boolean>>({
		leader: true,
		officer: true,
		raider: true,
		trial: false,
		member: false,
		public: false
	});

	/** Its player's rank, or null for an unclaimed character. */
	const rankOf = (c: GuildCharacter): Rank | null =>
		c.user_id === null ? null : (data.ranks[c.user_id] ?? null);

	const counted = $derived(
		characters.filter((c) => {
			const rank = rankOf(c);
			return (c.is_main ? mains : alts) && rank !== null && ranks[rank];
		})
	);

	// How many of each class, in the catalog's order; a bar picks its class for the table.
	const spread = $derived(
		data.classes.map((wowClass) => ({
			wowClass,
			count: counted.filter((c) => c.class === wowClass.slug).length
		}))
	);
	const most = $derived(Math.max(1, ...spread.map((s) => s.count)));

	// Like the guild list in game: a column to sort by, clicked again to turn it around.
	type Column = 'name' | 'race' | 'class' | 'spec' | 'player' | 'rank' | 'main';
	const COLUMNS: { key: Column; label: string }[] = [
		{ key: 'name', label: 'Name' },
		{ key: 'race', label: 'Race' },
		{ key: 'class', label: 'Class' },
		{ key: 'spec', label: 'Specs' },
		{ key: 'player', label: 'Player' },
		{ key: 'rank', label: 'Rank' },
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
		// Highest first, as the ranks go.
		rank: (c) => {
			const rank = rankOf(c);
			return rank === null ? undefined : String(RANKS.indexOf(rank));
		},
		// Mains first.
		main: (c) => (c.is_main ? '0' : '1')
	};

	// The search and the class (picked here or by a bar) narrow both tables.
	const matches = (c: GuildCharacter) =>
		(!classFilter || c.class === classFilter) &&
		`${fullName(c)} ${c.username ?? ''}`.toLowerCase().includes(query.trim().toLowerCase());

	function sorted(list: GuildCharacter[]): GuildCharacter[] {
		const key = sortKey[sort.column];
		const direction = sort.descending ? -1 : 1;
		return list.toSorted((a, b) => {
			const [x, y] = [key(a), key(b)];
			if (x !== y) {
				if (x === undefined) return 1;
				if (y === undefined) return -1;
				const order = x.localeCompare(y);
				if (order !== 0) return order * direction;
			}
			return fullName(a).localeCompare(fullName(b));
		});
	}

	const claimed = $derived(characters.filter((c) => c.user_id !== null).length);
	const shown = $derived(sorted(counted.filter(matches)));
	// Characters no account claims (pugs), apart from the guild's and for the admin's eyes only.
	const unclaimed = $derived(
		data.user.is_superuser ? sorted(characters.filter((c) => c.user_id === null && matches(c))) : []
	);
	const anyUnclaimed = $derived(
		data.user.is_superuser && characters.some((c) => c.user_id === null)
	);

	function added(character: GuildCharacter) {
		characters = [...characters, character];
		adding = false;
	}
</script>

<!-- The guild list: every column sorts, and the one sort holds for both tables. -->
{#snippet table(rows: GuildCharacter[])}
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
				{#each rows as character (character.id)}
					{@const race = findRace(character.race)}
					{@const wowClass = findClass(character.class)}
					{@const rank = rankOf(character)}
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
						<td>
							{#if rank}
								{RANK_LABEL[rank]}
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
{/snippet}

<svelte:head>
	<title>Characters | Blood Legion</title>
</svelte:head>

<div class="page-head">
	<div>
		<h1>Characters</h1>
	</div>
	{#if data.user.is_superuser && !adding}
		<Button variant="primary" onclick={() => (adding = true)}>Add a character</Button>
	{/if}
</div>

{#if adding}
	<CharacterForm owners={data.owners} onsaved={added} oncancel={() => (adding = false)} />
{/if}

<section class="panel" aria-label="Characters by class">
	<div class="toggles">
		<div class="filter" role="group" aria-labelledby="filter-characters">
			<span class="label" id="filter-characters">Characters</span>
			<div class="group">
				<button type="button" aria-pressed={mains} onclick={() => (mains = !mains)}>Mains</button>
				<button type="button" aria-pressed={alts} onclick={() => (alts = !alts)}>Alts</button>
			</div>
		</div>
		<div class="filter" role="group" aria-labelledby="filter-ranks">
			<span class="label" id="filter-ranks">Guild rank</span>
			<div class="group">
				{#each RANKS as rank (rank)}
					<button
						type="button"
						aria-pressed={ranks[rank]}
						onclick={() => (ranks[rank] = !ranks[rank])}>{RANK_GROUP[rank]}</button
					>
				{/each}
			</div>
		</div>
	</div>

	<ul class="bars">
		{#each spread as { wowClass, count } (wowClass.slug)}
			<li style:--class-color={wowClass.color}>
				<button
					type="button"
					aria-pressed={classFilter === wowClass.slug}
					class:faded={classFilter !== '' && classFilter !== wowClass.slug}
					title="{wowClass.name}: {count}"
					onclick={() => (classFilter = classFilter === wowClass.slug ? '' : wowClass.slug)}
				>
					<span class="count">{count}</span>
					<span class="track">
						<span class="bar" style:height="{(count / most) * 100}%"></span>
					</span>
					<img src={classIcon(wowClass.slug)} alt={wowClass.name} width="28" height="28" />
				</button>
			</li>
		{/each}
	</ul>
</section>

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

<p class="muted">{shown.length} of {claimed}</p>

{@render table(shown)}

{#if anyUnclaimed}
	<section>
		<h2>Unclaimed <span class="count">{unclaimed.length}</span></h2>
		{@render table(unclaimed)}
	</section>
{/if}

<style>
	/* Two filters, each under its own name, with a rule between them: beside each other on a
	   wide screen, one above the other on a phone. */
	.toggles {
		display: flex;
		gap: var(--space-6);
	}

	.filter {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}

	.filter + .filter {
		padding-left: var(--space-6);
		border-left: 1px solid var(--grey-surface);
	}

	@media (max-width: 40rem) {
		.toggles {
			flex-direction: column;
			gap: var(--space-4);
		}

		.filter + .filter {
			padding-top: var(--space-4);
			padding-left: 0;
			border-top: 1px solid var(--grey-surface);
			border-left: 0;
		}
	}

	.label {
		color: var(--grey-text);
		font-size: var(--text-xs);
		font-weight: 600;
		letter-spacing: 0.06em;
		text-transform: uppercase;
	}

	.group {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-1);
	}

	.group button {
		padding: var(--space-1) var(--space-3);
		color: var(--grey-text);
		font: inherit;
		font-size: var(--text-sm);
		font-weight: 500;
		background: none;
		border: 1px solid var(--grey-soft);
		border-radius: 999px;
		cursor: pointer;
	}

	.group button:hover {
		color: var(--foreground);
	}

	.group button[aria-pressed='true'] {
		color: var(--foreground);
		background-color: var(--red-bg);
		border-color: var(--red-solid);
	}

	/* A column a class: its count, its bar up from a shared baseline, its icon under it. */
	.bars {
		display: grid;
		grid-template-columns: repeat(9, minmax(0, 1fr));
		gap: var(--space-2);
		margin: 0;
		padding: 0;
		list-style: none;
	}

	.bars button {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: var(--space-1);
		width: 100%;
		padding: var(--space-1) 0;
		color: var(--foreground);
		font: inherit;
		background: none;
		border: 0;
		border-radius: var(--radius-md);
		cursor: pointer;
	}

	.bars button:hover {
		background-color: var(--grey-surface);
	}

	.bars button.faded {
		opacity: 0.35;
	}

	.count {
		font-weight: 700;
		font-variant-numeric: tabular-nums;
	}

	.track {
		display: flex;
		align-items: flex-end;
		width: min(100%, 2.5rem);
		height: 8rem;
		border-bottom: 1px solid var(--grey-soft);
	}

	.bar {
		width: 100%;
		background-color: var(--class-color);
		border-radius: 4px 4px 0 0;
		transition: height 200ms ease-out;
	}

	@media (prefers-reduced-motion: reduce) {
		.bar {
			transition: none;
		}
	}

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

	.count {
		color: var(--grey-text);
		font-weight: 400;
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
