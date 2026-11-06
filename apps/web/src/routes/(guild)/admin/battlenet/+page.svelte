<script lang="ts">
	import { invalidateAll } from '$app/navigation';
	import { api, errorMessage } from '$lib/api';
	import Alert from '$lib/components/Alert.svelte';
	import ConfirmButton from '$lib/components/ConfirmButton.svelte';
	import { formatDateTime, playerPath, rulesetLabel } from '$lib/guild';
	import { classIcon } from '$lib/wow/icons';
	import CharacterLink from '$lib/components/guild/CharacterLink.svelte';
	import type { BattlenetCharacter } from '$lib/types';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();

	/** The characters by Battle.net account, in the server's order (by member). */
	const accounts = $derived.by(() => {
		const groups: { key: string; first: BattlenetCharacter; characters: BattlenetCharacter[] }[] =
			[];
		for (const character of data.characters) {
			const key = `${character.username ?? ''}\u0000${character.account ?? ''}`;
			const last = groups.at(-1);
			if (last?.key === key) last.characters.push(character);
			else groups.push({ key, first: character, characters: [character] });
		}
		return groups;
	});

	let unhiding = $state<number | null>(null);
	let error = $state('');
	let done = $state('');

	/** Brings a hidden character onto the roster, as its member's. */
	async function unhide(character: BattlenetCharacter) {
		unhiding = character.id;
		error = '';
		done = '';
		try {
			await api.post(`/api/admin/battlenet-characters/${character.id}/unhide`);
			done = `${character.first_name} ${character.last_name} is on the roster now.`;
			await invalidateAll();
		} catch (err) {
			error = errorMessage(err, 'Unhiding it failed. Please try again.');
		} finally {
			unhiding = null;
		}
	}

	const findClass = (slug: string) => data.classes.find((c) => c.slug === slug);
	const findRace = (slug: string | null) => data.races.find((r) => r.slug === slug);
</script>

<svelte:head>
	<title>Battle.net characters | Blood Legion</title>
</svelte:head>

<div class="page-head">
	<div>
		<p class="kicker">Superusers only</p>
		<h1>Battle.net characters</h1>
	</div>
</div>

<p class="muted intro">
	The WoW: Forever characters on members' Battle.net accounts, read at each sign-in. One already on
	the site is linked to it, and goes to the member if nobody plays it yet. The rest are hidden: only
	superusers see them here, until an officer adds a character of the same name, which brings the
	hidden one onto the roster, or until a superuser unhides it here. Once on the roster it is the
	member's like any other character, and sign-ins no longer change it.
</p>

{#if error}<Alert variant="error">{error}</Alert>{/if}
{#if done}<Alert variant="success">{done}</Alert>{/if}

{#each accounts as account (account.key)}
	<section class="panel">
		<div class="head">
			<h2>
				{#if account.first.username}
					<a href={playerPath(account.first.username)}>{account.first.username}</a>
				{:else}
					<span class="muted">No member yet</span>
				{/if}
			</h2>
			<span class="muted">{account.first.account ?? 'Unnamed account'}</span>
		</div>
		<div class="scroll">
			<table>
				<thead>
					<tr>
						<th>Name</th>
						<th class="number">Level</th>
						<th>Class</th>
						<th>Race</th>
						<th>Ruleset</th>
						<th>On the site</th>
						<th>Last seen</th>
					</tr>
				</thead>
				<tbody>
					{#each account.characters as character (character.id)}
						{@const wowClass = findClass(character.class)}
						<tr class:hidden={character.is_hidden}>
							<td>
								{#if character.is_hidden}
									{character.first_name} {character.last_name}
								{:else}
									<CharacterLink
										firstName={character.first_name}
										lastName={character.last_name}
										cls={character.class}
									/>
								{/if}
							</td>
							<td class="number">{character.level}</td>
							<td>
								<span class="with-icon" style:color={wowClass?.color}>
									<img src={classIcon(character.class)} alt="" width="20" height="20" />
									{wowClass?.name ?? character.class}
								</span>
							</td>
							<td>{findRace(character.race)?.name ?? '—'}</td>
							<td>{rulesetLabel(character.ruleset)}</td>
							<td>
								{#if character.is_hidden}
									<span class="unhide">
										<span class="tag">Hidden</span>
										<ConfirmButton
											size="small"
											question={character.username
												? `Put it on the roster as ${character.username}'s?`
												: 'Put it on the roster, unclaimed?'}
											confirmLabel="Unhide"
											confirmVariant="primary"
											disabled={unhiding !== null}
											onconfirm={() => unhide(character)}
										>
											{unhiding === character.id ? 'Unhiding...' : 'Unhide'}
										</ConfirmButton>
									</span>
								{:else}
									On the roster
								{/if}
							</td>
							<td class="muted">
								{character.seen_at ? formatDateTime(character.seen_at) : 'No longer listed'}
							</td>
						</tr>
					{/each}
				</tbody>
			</table>
		</div>
	</section>
{:else}
	<p class="muted">No Battle.net characters read yet. They are read when members sign in.</p>
{/each}

<style>
	.intro {
		max-width: 44rem;
	}

	.head {
		display: flex;
		flex-wrap: wrap;
		align-items: baseline;
		gap: var(--space-1) var(--space-3);
	}

	.head h2 {
		font-size: var(--text-lg);
	}

	.with-icon {
		display: inline-flex;
		align-items: center;
		gap: var(--space-2);
	}

	.with-icon img {
		border-radius: var(--radius-sm);
	}

	.unhide {
		display: inline-flex;
		align-items: center;
		gap: var(--space-2);
	}

	tr.hidden td:first-child {
		color: var(--grey-text);
	}

	.tag {
		padding: 0 6px;
		color: var(--grey-text);
		font-size: var(--text-xs);
		border: 1px solid var(--grey-surface);
		border-radius: var(--radius-sm);
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

	.number {
		font-variant-numeric: tabular-nums;
		text-align: right;
	}
</style>
