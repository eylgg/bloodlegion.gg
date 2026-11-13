<script lang="ts">
	import { resolve } from '$app/paths';
	import { classIcon, raceIcon } from '$lib/wow/icons';
	import Button from '$lib/components/Button.svelte';
	import CharacterForm from '$lib/components/guild/CharacterForm.svelte';
	import CharacterLink from '$lib/components/guild/CharacterLink.svelte';
	import CharacterSpecs from '$lib/components/guild/CharacterSpecs.svelte';
	import {
		RANKS,
		RANK_GROUP,
		RANK_LABEL,
		formatDateTime,
		gameRankLabel,
		playerPath,
		rulesetLabel
	} from '$lib/guild';
	import type { GuildCharacter, NotAddedMember, Rank } from '$lib/types';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();

	// svelte-ignore state_referenced_locally
	let characters = $state<GuildCharacter[]>(data.characters);
	let query = $state('');
	let classFilter = $state('');
	let adding = $state(false);
	// What the add form starts with: a roster member's, from the list of those not added.
	let draft = $state<{
		first_name: string;
		last_name: string;
		class: string;
		level: number;
	} | null>(null);

	// The roster is the guild: the characters the latest roster upload named (Guilded). The chart and
	// the table count, by default, every ranked member's main; one nobody plays yet counts as a
	// member's main (it is all the site knows of them). Characters out of the guild (members' other
	// alts, pugs) are on Administration → Characters.
	let mains = $state(true);
	let alts = $state(false);
	let ranks = $state<Record<Rank, boolean>>({
		leader: true,
		officer: true,
		raider: true,
		trial: true,
		member: true,
		public: false
	});

	// A rank is the player's (the account's), changed on their page.
	const ranksById = $derived(new Map(data.members.map((m) => [m.id, m.guild_rank])));

	/** Its player's rank, or null for an unclaimed character. */
	const rankOf = (c: GuildCharacter): Rank | null =>
		c.user_id === null ? null : (ranksById.get(c.user_id) ?? null);

	// The public are the sign-ups waiting for a rank: only officers filter by them.
	const rankFilters = $derived(RANKS.filter((rank) => rank !== 'public' || data.officer));

	/** The rank a guild character counts under: its player's, or a member's for one nobody plays;
	 * null for one out of the guild. */
	const countsAs = (c: GuildCharacter): Rank | null =>
		c.in_guild ? (rankOf(c) ?? (c.user_id === null ? 'member' : null)) : null;

	const counted = $derived(
		characters.filter((c) => {
			const rank = countsAs(c);
			const main = c.is_main || c.user_id === null;
			return (main ? mains : alts) && rank !== null && ranks[rank];
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
	type Column =
		'name' | 'level' | 'race' | 'class' | 'ruleset' | 'spec' | 'player' | 'rank' | 'game' | 'main';
	const COLUMNS: { key: Column; label: string }[] = [
		{ key: 'name', label: 'Name' },
		{ key: 'level', label: 'Level' },
		{ key: 'race', label: 'Race' },
		{ key: 'class', label: 'Class' },
		{ key: 'ruleset', label: 'Ruleset' },
		{ key: 'spec', label: 'Specs' },
		{ key: 'player', label: 'Player' },
		{ key: 'rank', label: 'Rank' },
		{ key: 'game', label: 'In game' },
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
		// Highest first; at 60, whoever got there first.
		level: (c) => `${String(60 - c.level).padStart(2, '0')} ${c.reached_60_at ?? ''}`,
		race: (c) => findRace(c.race)?.name,
		class: (c) => findClass(c.class)?.name ?? c.class,
		ruleset: (c) => rulesetLabel(c.ruleset),
		spec: mainSpec,
		player: (c) => c.username ?? undefined,
		// Highest first, as the ranks go.
		rank: (c) => {
			const rank = rankOf(c);
			return rank === null ? undefined : String(RANKS.indexOf(rank));
		},
		// The guild master first, as the game numbers its ranks.
		game: (c) => (c.guild_rank === null ? undefined : String(c.guild_rank).padStart(2, '0')),
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

	const claimed = $derived(characters.filter((c) => countsAs(c) !== null).length);
	const shown = $derived(sorted(counted.filter(matches)));

	// Accounts with no characters yet (mostly new sign-ups), for the administrator to rank.
	const withoutCharacters = $derived(
		data.user.is_superuser
			? data.members
					.filter((m) => !characters.some((c) => c.user_id === m.id))
					.toSorted((a, b) => a.username.localeCompare(b.username))
			: []
	);

	function added(character: GuildCharacter) {
		characters = [...characters, character];
		adding = false;
		draft = null;
	}

	// Roster members uploads could not add, until a character of the name is on the site.
	const onSite = (name: string) => {
		const wanted = name.toLowerCase();
		return characters.some((c) => {
			const full = fullName(c).toLowerCase();
			return full === wanted || full.replace(' ', '') === wanted;
		});
	};
	const notAdded = $derived(data.notAdded.filter((m) => !onSite(m.name)));

	/** Opens the add form with the member's name (first and last at its first space), class, and level. */
	function addMember(member: NotAddedMember) {
		const [first, ...rest] = member.name.trim().split(/\s+/);
		const wowClass = data.classes.find((c) => c.name.toLowerCase() === member.class?.toLowerCase());
		draft = {
			first_name: first ?? '',
			last_name: rest.join(''),
			class: wowClass?.slug ?? '',
			level: member.level
		};
		adding = true;
		window.scrollTo({ top: 0, behavior: 'smooth' });
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
								firstName={character.first_name}
								lastName={character.last_name}
								cls={character.class}
								icon={false}
							/>
						</td>
						<td class="level">{character.level}</td>
						<td>
							{#if race}
								<span class="with-icon">
									{#if character.sex}
										<img src={raceIcon(race.slug, character.sex)} alt="" width="20" height="20" />
									{/if}
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
						<td>{rulesetLabel(character.ruleset)}</td>
						<td>
							<CharacterSpecs cls={character.class} specs={character.specs} size={20} />
						</td>
						<td>
							{#if character.username}
								<a href={playerPath(character.username)}>{character.username}</a>
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
						<td>
							{#if character.guild_rank !== null}
								{gameRankLabel(character.guild_rank, character.guild_rank_name)}
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
	<title>Roster | Blood Legion</title>
</svelte:head>

<div class="page-head">
	<div>
		<h1>Roster</h1>
	</div>
	{#if data.officer && !adding}
		<Button variant="primary" onclick={() => (adding = true)}>Add a character</Button>
	{/if}
</div>

{#if adding}
	{#key draft}
		<CharacterForm
			{draft}
			owners={data.owners}
			onsaved={added}
			oncancel={() => {
				adding = false;
				draft = null;
			}}
		/>
	{/key}
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
				{#each rankFilters as rank (rank)}
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
					<span class="track">
						<span class="bar" style:height="{(count / most) * 100}%"></span>
					</span>
					<img src={classIcon(wowClass.slug)} alt={wowClass.name} width="28" height="28" />
					<span class="count">{count}</span>
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

<p class="muted">
	{shown.length} of {claimed}{#if data.user.is_superuser}&nbsp;· characters out of the guild are on
		<a href={resolve('/admin/characters')}>Characters</a>{/if}
</p>

{@render table(shown)}

{#if data.mismatches.length > 0}
	<section>
		<h2>Rank not as in game <span class="count">{data.mismatches.length}</span></h2>
		<p class="muted">
			Members whose rank here is not the highest their characters have in the guild in game (as the
			ranks in game stand for the site's, on <a href={resolve('/admin')}>Administration</a>), or who
			have a rank but no character in the guild. Change it on their page. Only you see this.
		</p>
		<div class="scroll">
			<table>
				<thead>
					<tr>
						<th class="plain">Player</th>
						<th class="plain">Rank here</th>
						<th class="plain">In game</th>
						<th class="plain">Characters in the guild</th>
					</tr>
				</thead>
				<tbody>
					{#each data.mismatches as mismatch (mismatch.user_id)}
						<tr>
							<td><a href={playerPath(mismatch.username)}>{mismatch.username}</a></td>
							<td>{RANK_LABEL[mismatch.rank]}</td>
							<td>
								{#if mismatch.expected}
									{RANK_LABEL[mismatch.expected]}
								{:else}
									<span class="muted">Not in the guild</span>
								{/if}
							</td>
							<td>
								{#each mismatch.characters as character (character.first_name + ' ' + character.last_name)}
									<div>
										<CharacterLink
											firstName={character.first_name}
											lastName={character.last_name}
											cls={character.class}
											icon={false}
										/>
										{#if character.guild_rank !== null}
											<span class="muted"
												>{gameRankLabel(character.guild_rank, character.guild_rank_name)}</span
											>
										{/if}
									</div>
								{:else}
									<span class="muted">—</span>
								{/each}
							</td>
						</tr>
					{/each}
				</tbody>
			</table>
		</div>
	</section>
{/if}

{#if notAdded.length > 0}
	<section>
		<h2>Not added from the roster <span class="count">{notAdded.length}</span></h2>
		<p class="muted">
			Guild members the addon's roster named that it could not add, for a name or class the site
			does not take. Only you see them.
		</p>
		<div class="scroll">
			<table>
				<thead>
					<tr>
						<th class="plain">Name</th>
						<th class="plain">Level</th>
						<th class="plain">Class</th>
						<th class="plain">Last seen</th>
						<th class="plain" aria-label="Add"></th>
					</tr>
				</thead>
				<tbody>
					{#each notAdded as member (member.name)}
						<tr>
							<td>{member.name}</td>
							<td class="level">{member.level}</td>
							<td>{member.class ?? '—'}</td>
							<td class="muted">{formatDateTime(member.seen_at)}</td>
							<td>
								<Button size="small" variant="secondary" onclick={() => addMember(member)}>
									Add
								</Button>
							</td>
						</tr>
					{/each}
				</tbody>
			</table>
		</div>
	</section>
{/if}

{#if withoutCharacters.length > 0}
	<section>
		<h2>No characters yet <span class="count">{withoutCharacters.length}</span></h2>
		<div class="scroll">
			<table>
				<thead>
					<tr><th class="plain">Player</th><th class="plain">Rank</th></tr>
				</thead>
				<tbody>
					{#each withoutCharacters as member (member.id)}
						<tr>
							<td>
								<a href={playerPath(member.username)}>{member.username}</a>
							</td>
							<td>{RANK_LABEL[member.guild_rank]}</td>
						</tr>
					{/each}
				</tbody>
			</table>
		</div>
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

	/* A column a class: its bar up from a shared baseline, its icon under it, and its count under that. */
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

	/* As wide as the class icon under it (28px). */
	.track {
		display: flex;
		align-items: flex-end;
		width: 28px;
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
	.level {
		font-variant-numeric: tabular-nums;
	}

	th[aria-sort] button {
		color: var(--foreground);
	}

	/* A heading that does not sort, set like the ones that do. */
	th.plain {
		padding: var(--space-2) var(--space-3);
		color: var(--grey-text);
		font-size: var(--text-xs);
		font-weight: 600;
		letter-spacing: 0.06em;
		text-transform: uppercase;
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
