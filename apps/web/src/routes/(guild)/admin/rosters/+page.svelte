<script lang="ts">
	import { invalidateAll } from '$app/navigation';
	import { api, errorMessage } from '$lib/api';
	import { RANKS, RANK_LABEL, formatDateTime } from '$lib/guild';
	import Alert from '$lib/components/Alert.svelte';
	import ConfirmButton from '$lib/components/ConfirmButton.svelte';
	import CharacterLink from '$lib/components/guild/CharacterLink.svelte';
	import type { GameRank, Rank, RosterUpload } from '$lib/types';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();

	let undoing = $state<number | null>(null);
	let error = $state('');
	let done = $state('');

	/** Undoes an upload: its characters still as it left them go back to how they were. */
	async function undo(upload: RosterUpload) {
		undoing = upload.id;
		error = '';
		done = '';
		try {
			const { reverted, kept, removed } = await api.post<{
				reverted: number;
				kept: number;
				removed: number;
			}>(`/api/admin/rosters/${upload.id}/undo`);
			done =
				`Undone: ${reverted} ${reverted === 1 ? 'character' : 'characters'} put back` +
				(kept ? `, ${kept} left as they are (changed since)` : '') +
				(removed ? `, ${removed} it added removed.` : '.');
			await invalidateAll();
		} catch (err) {
			error = errorMessage(err, 'Undoing it failed. Please try again.');
		} finally {
			undoing = null;
		}
	}

	// svelte-ignore state_referenced_locally
	let ranks = $state<GameRank[]>(data.ranks);
	let rankError = $state('');

	/** Says which site rank a rank in game stands for, or none to leave it out. */
	async function setRank(rank: GameRank, value: string) {
		rankError = '';
		try {
			ranks = await api.put<GameRank[]>(
				`/api/admin/rosters/ranks?name=${encodeURIComponent(rank.name)}`,
				{ site_rank: value === '' ? null : (value as Rank) }
			);
		} catch (err) {
			rankError = errorMessage(err, 'Saving the rank failed. Please try again.');
		}
	}

	const sixty = (iso: string | null) => (iso ? formatDateTime(iso) : '—');
</script>

<svelte:head>
	<title>Roster uploads | Blood Legion</title>
</svelte:head>

<div class="page-head">
	<div>
		<p class="kicker">Superusers only</p>
		<h1>Roster uploads</h1>
	</div>
</div>

<p class="muted intro">
	The rosters officers' Blood Legion clients sent from the addon, latest first, with every level
	each one changed and every member it added as a character. Undoing one puts back the characters
	still as it left them (any changed since, by a later upload or an officer, keep their level) and
	removes the characters it added that nobody plays yet and that never raided.
</p>

<section class="panel standing">
	<h2>The guild now</h2>
	{#if data.standing.latest}
		<p>
			<strong>{data.standing.in_guild}</strong> of the site's {data.standing.characters} characters are
			in the guild, as the roster upload of
			<strong>{formatDateTime(data.standing.latest.created_at)}</strong> saw it ({data.standing
				.latest.members} members, by {data.standing.latest.username ?? 'a deleted account'}).
		</p>
		<p class="muted">
			Only roster uploads say who is in the guild: each one marks the characters it named and no
			others, and undoing one goes back to the upload before.
		</p>
	{:else}
		<p class="muted">
			No roster upload has said who is in the guild yet ({data.standing.in_guild} of
			{data.standing.characters} characters are marked in it from older uploads). The next upload decides.
		</p>
	{/if}
</section>

<section class="panel">
	<h2>Ranks in game</h2>
	<p class="muted">
		The guild's ranks as the roster uploads saw them, and the site rank each stands for. A member
		whose site rank is not the highest of their characters' in the guild is pointed out on the
		roster page; a rank left out (an alt rank) is not compared. Each new rank is matched the first
		time an upload sees it: the guild master is the leader, a rank named exactly as one of the
		site's is that rank, and any other is not compared.
	</p>
	{#if rankError}<Alert variant="error">{rankError}</Alert>{/if}
	{#if ranks.length > 0}
		<table class="ranks">
			<thead>
				<tr><th>#</th><th>In game</th><th>Characters</th><th>On the site</th></tr>
			</thead>
			<tbody>
				{#each ranks as rank (rank.name)}
					<tr>
						<td class="muted">{rank.rank_index}</td>
						<td>{rank.name}</td>
						<td>{rank.characters}</td>
						<td>
							<select
								aria-label="Site rank for {rank.name}"
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

{#if error}<Alert variant="error">{error}</Alert>{/if}
{#if done}<Alert variant="success">{done}</Alert>{/if}

{#each data.uploads as upload (upload.id)}
	<section class="panel" class:undone={upload.undone_at}>
		<div class="head">
			<div>
				<h2>{formatDateTime(upload.created_at)}</h2>
				<p class="muted">
					By {upload.username ?? 'a deleted account'} · {upload.members} members, {upload.matched}
					matched, {upload.changes.length} changed, {upload.added.length} added
				</p>
			</div>
			{#if upload.undone_at}
				<span class="tag">
					Undone {formatDateTime(upload.undone_at)}{upload.undone_by
						? ` by ${upload.undone_by}`
						: ''}
				</span>
			{:else if upload.changes.length > 0 || upload.added.length > 0}
				<ConfirmButton
					size="small"
					question="Undo this upload?"
					confirmLabel="Undo"
					disabled={undoing !== null}
					onconfirm={() => undo(upload)}
				>
					{undoing === upload.id ? 'Undoing...' : 'Undo'}
				</ConfirmButton>
			{/if}
		</div>

		{#if upload.changes.length > 0}
			<details>
				<summary>{upload.changes.length} changed</summary>
				<div class="scroll">
					<table>
						<thead>
							<tr>
								<th>Character</th>
								<th>Level</th>
								<th>Reached 60</th>
								<th>Now</th>
							</tr>
						</thead>
						<tbody>
							{#each upload.changes as change (change.character_id)}
								<tr>
									<td>
										<CharacterLink
											firstName={change.first_name}
											lastName={change.last_name}
											cls={change.class}
										/>
									</td>
									<td class="number">{change.old_level} → {change.new_level}</td>
									<td>
										{#if change.old_reached_60_at !== change.new_reached_60_at}
											{sixty(change.old_reached_60_at)} → {sixty(change.new_reached_60_at)}
										{:else}
											<span class="muted">—</span>
										{/if}
									</td>
									<td class="muted">
										{change.current ? 'As it left it' : 'Changed since'}
									</td>
								</tr>
							{/each}
						</tbody>
					</table>
				</div>
			</details>
		{/if}

		{#if upload.added.length > 0}
			<details>
				<summary>{upload.added.length} added</summary>
				<ul class="unmatched">
					{#each upload.added as added (added.character_id)}
						<li>
							<CharacterLink
								firstName={added.first_name}
								lastName={added.last_name}
								cls={added.class}
							/>
							{#if added.revealed}<span class="muted">from a Battle.net sign-in</span>{/if}
						</li>
					{/each}
				</ul>
			</details>
		{/if}

		{#if upload.unmatched.length > 0}
			<details>
				<summary>{upload.unmatched.length} not added</summary>
				<ul class="unmatched">
					{#each upload.unmatched as member (member.name)}
						<li>
							{member.name}
							<span class="muted">{member.level}{member.class ? ` ${member.class}` : ''}</span>
						</li>
					{/each}
				</ul>
			</details>
		{/if}
	</section>
{:else}
	<p class="muted">No rosters uploaded yet.</p>
{/each}

<style>
	.intro {
		max-width: 44rem;
	}

	.panel.undone {
		opacity: 0.6;
	}

	.head {
		display: flex;
		flex-wrap: wrap;
		align-items: flex-start;
		justify-content: space-between;
		gap: var(--space-2) var(--space-4);
	}

	.head h2 {
		font-size: var(--text-lg);
	}

	.head p {
		margin: 0;
	}

	.tag {
		padding: 0 6px;
		color: var(--grey-text);
		font-size: var(--text-xs);
		border: 1px solid var(--grey-surface);
		border-radius: var(--radius-sm);
	}

	summary {
		cursor: pointer;
		font-size: var(--text-sm);
		font-weight: 600;
	}

	/* On a phone the table scrolls sideways rather than squeezing its columns. */
	.scroll {
		overflow-x: auto;
	}

	table {
		width: 100%;
		margin-top: var(--space-2);
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

	.number {
		font-variant-numeric: tabular-nums;
	}

	.unmatched {
		margin: var(--space-2) 0 0;
		padding-left: var(--space-4);
		font-size: var(--text-sm);
	}
</style>
