<script lang="ts">
	import { api, errorMessage } from '$lib/api';
	import { classIcon, raceIcon } from '$lib/wow/icons';
	import Alert from '$lib/components/Alert.svelte';
	import CharacterLink from '$lib/components/guild/CharacterLink.svelte';
	import CharacterSpecs from '$lib/components/guild/CharacterSpecs.svelte';
	import { RANK_LABEL, canSetRank, rankChoices } from '$lib/guild';
	import type { Rank } from '$lib/types';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();

	// svelte-ignore state_referenced_locally
	let rank = $state<Rank>(data.player.guild_rank);
	let error = $state('');
	let saved = $state(false);

	const findClass = (slug: string) => data.classes.find((c) => c.slug === slug);
	const findRace = (slug: string | null) => data.races.find((r) => r.slug === slug);

	// Decided on the rank the page opened with, so a change does not take the picker away.
	const editable = $derived(canSetRank(data.user, data.player.id, data.player.guild_rank));

	async function setRank(to: Rank) {
		const from = rank;
		rank = to;
		error = '';
		saved = false;
		try {
			await api.put(`/api/guild/members/${data.player.id}/rank`, { rank: to });
			saved = true;
		} catch (err) {
			rank = from;
			error = errorMessage(err, 'Changing the rank failed. Please try again.');
		}
	}
</script>

<svelte:head>
	<title>{data.player.username} | Blood Legion</title>
</svelte:head>

<div class="page-head">
	<div>
		<p class="kicker">Player</p>
		<h1>{data.player.username}</h1>
	</div>
</div>

{#if error}<Alert variant="error">{error}</Alert>{/if}

<section class="panel">
	{#if editable}
		<label class="field rank">
			Guild rank
			<select
				value={rank}
				onchange={(event) => setRank(event.currentTarget.value as Rank)}
				aria-describedby="rank-hint"
			>
				{#each rankChoices(data.user) as choice (choice)}
					<option value={choice}>{RANK_LABEL[choice]}</option>
				{/each}
			</select>
		</label>
		<p class="muted" id="rank-hint">
			{saved ? 'Saved.' : 'The rank is the player’s, for all their characters.'}
		</p>
	{:else}
		<p><span class="label">Guild rank</span> {RANK_LABEL[rank]}</p>
	{/if}
</section>

<section>
	<h2>Characters <span class="count">{data.player.characters.length}</span></h2>
	{#if data.player.characters.length === 0}
		<p class="muted">No characters yet.</p>
	{:else}
		<div class="scroll">
			<table>
				<thead>
					<tr>
						<th>Name</th>
						<th>Level</th>
						<th>Race</th>
						<th>Class</th>
						<th>Specs</th>
						<th>Main</th>
					</tr>
				</thead>
				<tbody>
					{#each data.player.characters as character (character.id)}
						{@const race = findRace(character.race)}
						{@const wowClass = findClass(character.class)}
						<tr>
							<td>
								<CharacterLink
									firstName={character.first_name}
									lastName={character.last_name}
									cls={character.class}
									icon={false}
								/>
							</td>
							<td class="number">{character.level}</td>
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
							<td>
								<CharacterSpecs cls={character.class} specs={character.specs} size={20} />
							</td>
							<td>{character.is_main ? 'Main' : 'Alt'}</td>
						</tr>
					{/each}
				</tbody>
			</table>
		</div>
	{/if}
</section>

<style>
	.rank {
		max-width: 16rem;
	}

	.label {
		margin-right: var(--space-2);
		color: var(--grey-text);
		font-size: var(--text-xs);
		font-weight: 600;
		letter-spacing: 0.06em;
		text-transform: uppercase;
	}

	.panel p {
		margin: 0;
	}

	.count {
		color: var(--grey-text);
		font-weight: 400;
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
		vertical-align: middle;
	}

	.number {
		font-variant-numeric: tabular-nums;
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
