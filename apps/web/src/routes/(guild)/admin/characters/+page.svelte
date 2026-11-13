<script lang="ts">
	import { formatDateTime, gameRankLabel, playerPath } from '$lib/guild';
	import CharacterLink from '$lib/components/guild/CharacterLink.svelte';
	import type { AdminCharacter } from '$lib/types';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();

	type Filter = 'all' | 'in-guild' | 'not-in-guild' | 'unclaimed';
	let filter = $state<Filter>('all');
	let search = $state('');

	const FILTERS: { value: Filter; label: string; test: (c: AdminCharacter) => boolean }[] = [
		{ value: 'all', label: 'Everyone', test: () => true },
		{ value: 'in-guild', label: 'Guilded', test: (c) => c.in_guild },
		{ value: 'not-in-guild', label: 'Not guilded', test: (c) => !c.in_guild },
		{ value: 'unclaimed', label: 'Nobody plays', test: (c) => c.user_id === null }
	];

	const counts = $derived(
		Object.fromEntries(
			FILTERS.map((option) => [option.value, data.characters.filter(option.test).length])
		) as Record<Filter, number>
	);

	const shown = $derived.by(() => {
		const test = FILTERS.find((option) => option.value === filter)!.test;
		const query = search.trim().toLowerCase();
		return data.characters.filter(
			(c) =>
				test(c) &&
				(!query ||
					`${c.first_name} ${c.last_name} ${c.username ?? ''} ${c.guild_rank_name ?? ''}`
						.toLowerCase()
						.includes(query))
		);
	});

	const findClass = (slug: string) => data.classes.find((c) => c.slug === slug);
	const findRace = (slug: string | null) => data.races.find((r) => r.slug === slug)?.name;
</script>

<svelte:head>
	<title>Characters | Blood Legion</title>
</svelte:head>

<div class="page-head">
	<div>
		<p class="kicker">Superusers only</p>
		<h1>Characters</h1>
	</div>
</div>

<p class="muted intro">
	Every character on the site: the guild's, which are on the roster, and the rest (members' alts out
	of the guild, characters Battle.net sign-ins listed, pugs nobody plays). Guilded is the latest
	roster upload's say, with the rank it saw in game.
</p>

<div class="controls">
	<div class="filters" role="group" aria-label="Show">
		{#each FILTERS as option (option.value)}
			<button
				type="button"
				class:active={filter === option.value}
				aria-pressed={filter === option.value}
				onclick={() => (filter = option.value)}
			>
				{option.label} <span class="count">{counts[option.value]}</span>
			</button>
		{/each}
	</div>
	<input type="search" placeholder="Search names, players, and ranks" bind:value={search} />
</div>

<section class="panel">
	<div class="scroll">
		<table>
			<thead>
				<tr>
					<th>Name</th>
					<th>Level</th>
					<th>Class</th>
					<th>Race</th>
					<th>Player</th>
					<th>In game</th>
					<th>Battle.net</th>
					<th>Added</th>
				</tr>
			</thead>
			<tbody>
				{#each shown as character (character.id)}
					<tr>
						<td>
							<span class="name">
								<CharacterLink
									firstName={character.first_name}
									lastName={character.last_name}
									cls={character.class}
									icon={false}
								/>
								{#if character.is_main}<span class="tag">Main</span>{/if}
								{#if character.in_guild}<span class="tag guild">Guilded</span>{/if}
							</span>
						</td>
						<td class="number">{character.level}</td>
						<td style:color={findClass(character.class)?.color}>
							{findClass(character.class)?.name ?? character.class}
						</td>
						<td>{findRace(character.race) ?? '—'}</td>
						<td>
							{#if character.username}
								<a href={playerPath(character.username)}>{character.username}</a>
							{:else}
								<span class="muted">Nobody</span>
							{/if}
						</td>
						<td>{gameRankLabel(character.guild_rank, character.guild_rank_name) ?? '—'}</td>
						<td class="muted">
							{character.battlenet_seen_at ? formatDateTime(character.battlenet_seen_at) : '—'}
						</td>
						<td class="muted">{formatDateTime(character.created_at)}</td>
					</tr>
				{:else}
					<tr><td colspan="8" class="muted">No characters match.</td></tr>
				{/each}
			</tbody>
		</table>
	</div>
</section>

<style>
	.intro {
		max-width: 44rem;
	}

	.controls {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-3);
		margin-bottom: var(--space-3);
	}

	.filters {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-2);
	}

	.filters button {
		padding: var(--space-1) var(--space-3);
		color: var(--grey-text);
		font: inherit;
		font-size: var(--text-sm);
		background: none;
		border: 1px solid var(--grey-surface);
		border-radius: var(--radius-sm);
		cursor: pointer;
	}

	.filters button.active {
		color: inherit;
		border-color: currentColor;
	}

	.count {
		font-variant-numeric: tabular-nums;
		opacity: 0.7;
	}

	.controls input {
		min-width: 0;
		width: 16rem;
		max-width: 100%;
	}

	.name {
		display: inline-flex;
		align-items: center;
		gap: var(--space-2);
	}

	.tag {
		padding: 0 6px;
		color: var(--grey-text);
		font-size: var(--text-xs);
		border: 1px solid var(--grey-surface);
		border-radius: var(--radius-sm);
	}

	.tag.guild {
		color: var(--green-text);
		border-color: currentColor;
	}

	.number {
		font-variant-numeric: tabular-nums;
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
		padding: var(--space-2) var(--space-3);
		color: var(--grey-text);
		font-size: var(--text-xs);
		font-weight: 600;
		letter-spacing: 0.06em;
		text-align: left;
		text-transform: uppercase;
		border-bottom: 1px solid var(--grey-surface);
	}

	td {
		padding: var(--space-2) var(--space-3);
		border-bottom: 1px solid var(--grey-surface);
	}
</style>
