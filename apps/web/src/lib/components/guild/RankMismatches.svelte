<script lang="ts">
	import { RANK_LABEL, gameRankLabel, playerPath } from '$lib/guild';
	import CharacterLink from './CharacterLink.svelte';
	import type { RankMismatch } from '$lib/types';

	/** The players whose rank here is not the highest of their characters' in game. */
	let { mismatches }: { mismatches: RankMismatch[] } = $props();
</script>

<section class="panel">
	<h2>Rank not as in game <span class="count">{mismatches.length}</span></h2>
	<p class="muted">
		Players whose rank here is not the highest their characters have in the guild in game (as the
		guild's ranks in game count, above), or who have a rank but no character in the guild. Change it
		on their page.
	</p>
	{#if mismatches.length > 0}
		<div class="scroll">
			<table>
				<thead>
					<tr>
						<th>Player</th>
						<th>Rank here</th>
						<th>In game</th>
						<th>Characters in the guild</th>
					</tr>
				</thead>
				<tbody>
					{#each mismatches as mismatch (mismatch.user_id)}
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
	{:else}
		<p class="muted">Every player's rank matches their characters'.</p>
	{/if}
</section>

<style>
	.count {
		color: var(--grey-text);
		font-weight: 400;
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
		vertical-align: top;
		border-bottom: 1px solid var(--grey-surface);
	}
</style>
