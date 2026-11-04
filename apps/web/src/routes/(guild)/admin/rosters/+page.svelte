<script lang="ts">
	import { invalidateAll } from '$app/navigation';
	import { api, errorMessage } from '$lib/api';
	import { formatDateTime } from '$lib/guild';
	import Alert from '$lib/components/Alert.svelte';
	import ConfirmButton from '$lib/components/ConfirmButton.svelte';
	import CharacterLink from '$lib/components/guild/CharacterLink.svelte';
	import type { RosterUpload } from '$lib/types';
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
			const { reverted, kept } = await api.post<{ reverted: number; kept: number }>(
				`/api/admin/rosters/${upload.id}/undo`
			);
			done =
				`Undone: ${reverted} ${reverted === 1 ? 'character' : 'characters'} put back` +
				(kept ? `, ${kept} left as they are (changed since).` : '.');
			await invalidateAll();
		} catch (err) {
			error = errorMessage(err, 'Undoing it failed. Please try again.');
		} finally {
			undoing = null;
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
	each one changed. Undoing one puts back the characters still as it left them; any changed since,
	by a later upload or an officer, keep their level.
</p>

{#if error}<Alert variant="error">{error}</Alert>{/if}
{#if done}<Alert variant="success">{done}</Alert>{/if}

{#each data.uploads as upload (upload.id)}
	<section class="panel" class:undone={upload.undone_at}>
		<div class="head">
			<div>
				<h2>{formatDateTime(upload.created_at)}</h2>
				<p class="muted">
					By {upload.username ?? 'a deleted account'} · {upload.members} members, {upload.matched}
					matched, {upload.changes.length} changed
				</p>
			</div>
			{#if upload.undone_at}
				<span class="tag">
					Undone {formatDateTime(upload.undone_at)}{upload.undone_by
						? ` by ${upload.undone_by}`
						: ''}
				</span>
			{:else if upload.changes.length > 0}
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
											id={change.character_id}
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

		{#if upload.unmatched.length > 0}
			<details>
				<summary>{upload.unmatched.length} not on the site</summary>
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
