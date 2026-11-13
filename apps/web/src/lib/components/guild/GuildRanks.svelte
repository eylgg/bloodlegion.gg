<script lang="ts">
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
		} catch (err) {
			error = errorMessage(err, 'Saving the rank failed. Please try again.');
		}
	}
</script>

<section class="panel">
	<h2>{guild}'s ranks in game</h2>
	<p class="muted">
		The guild's ranks as the roster uploads saw them, shown as the armory numbers them (Rank 1 is
		the guild master), with the name the guild gives each in game. "On the site" is the rank a
		member holds here (Leader, Officer, Raider, Trial, Member, or Public: what they can do on the
		site). A member whose rank here is not the highest of their characters' in game is pointed out
		on the roster page; a rank not compared (an alt rank) is left out. Each new rank is matched the
		first time an upload sees it: the guild master is the leader, a rank named exactly as one of the
		site's is that rank, and any other is not compared. Renaming a rank in game keeps its match.
	</p>
	{#if error}<Alert variant="error">{error}</Alert>{/if}
	{#if ranks.length > 0}
		<table>
			<thead>
				<tr><th>Rank</th><th>Name in game</th><th>Characters</th><th>On the site</th></tr>
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
</section>

<style>
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
