<script lang="ts">
	import { invalidateAll } from '$app/navigation';
	import { api, errorMessage } from '$lib/api';
	import { RANKS, RANK_LABEL, gameRankLabel, gameRankNumber } from '$lib/guild';
	import Alert from '$lib/components/Alert.svelte';
	import type { GameRank, Rank } from '$lib/types';

	/** The guild's ranks in game, as roster uploads saw them. */
	let { ranks: initial, guild }: { ranks: GameRank[]; guild: string } = $props();

	// svelte-ignore state_referenced_locally
	let ranks = $state<GameRank[]>(initial);
	let error = $state('');

	/** Says which site rank a rank in game stands for, or none to leave it out. */
	async function setRank(rank: GameRank, value: string) {
		error = '';
		try {
			ranks = await api.put<GameRank[]>(`/api/admin/rosters/ranks/${rank.rank}`, {
				site_rank: value === '' ? null : (value as Rank)
			});
			// Who is pointed out as not matching changes with it.
			await invalidateAll();
		} catch (err) {
			error = errorMessage(err, 'Saving the rank failed. Please try again.');
		}
	}
</script>

<div class="ranks">
	<h3>Ranks in game</h3>
	<p class="muted">
		{guild}'s ranks as the roster uploads saw them, numbered as the armory numbers them (Rank 1 is
		the guild master), with the name the guild gives each in game; both come from the game and
		change only with an upload. "Counts as" is the site rank a member with a character at that rank
		should hold, for pointing out the players whose rank here differs (below): several ranks can
		count as the same one, and a rank not compared (an alt rank) is left out. Each new rank is
		matched the first time an upload sees it: the guild master is the leader, a rank named exactly
		as one of the site's is that rank, and any other is not compared. Renaming a rank in game keeps
		its match.
	</p>
	{#if error}<Alert variant="error">{error}</Alert>{/if}
	{#if ranks.length > 0}
		<table>
			<thead>
				<tr><th>Rank</th><th>Name in game</th><th>Characters</th><th>Counts as</th></tr>
			</thead>
			<tbody>
				{#each ranks as rank (rank.rank)}
					<tr>
						<td class="muted">{gameRankNumber(rank.rank)}</td>
						<td>{rank.name ?? '—'}</td>
						<td>{rank.characters}</td>
						<td>
							<select
								aria-label="Site rank for {gameRankLabel(rank.rank, rank.name)}"
								value={rank.site_rank ?? ''}
								onchange={(event) => setRank(rank, event.currentTarget.value)}
							>
								<option value="">Not compared</option>
								{#each RANKS as value (value)}
									<option {value}>{RANK_LABEL[value]}</option>
								{/each}
							</select>
						</td>
					</tr>
				{/each}
			</tbody>
		</table>
	{:else}
		<p class="muted">No upload has sent the guild's ranks yet: they come with addon 0.5.0.</p>
	{/if}
</div>

<style>
	.ranks {
		margin-top: var(--space-4);
	}

	h3 {
		margin: 0 0 var(--space-2);
		font-size: var(--text-md);
	}

	table {
		width: 100%;
		border-collapse: collapse;
		font-size: var(--text-md);
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
