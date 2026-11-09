<script lang="ts">
	import { invalidateAll } from '$app/navigation';
	import { api, errorMessage } from '$lib/api';
	import { RANK_LABEL, formatDate, playerPath } from '$lib/guild';
	import Alert from '$lib/components/Alert.svelte';
	import Button from '$lib/components/Button.svelte';
	import type { AdminAccount, Connection } from '$lib/types';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();

	type Filter = 'all' | 'no-battlenet' | 'no-discord';
	let filter = $state<Filter>('all');
	let search = $state('');

	let renaming = $state<number | null>(null);
	let newName = $state('');
	let saving = $state(false);
	let error = $state('');
	let done = $state('');

	/** The account's connected sign-in with that provider, if any. */
	const connected = (account: AdminAccount, provider: string) =>
		account.connections.find((c) => c.provider === provider && !c.disconnected_at);
	/** Ones it disconnected, or with any other provider. */
	const others = (account: AdminAccount) =>
		account.connections.filter(
			(c) => c.disconnected_at || (c.provider !== 'battlenet' && c.provider !== 'discord')
		);

	const counts = $derived({
		all: data.accounts.length,
		'no-battlenet': data.accounts.filter((a) => !connected(a, 'battlenet')).length,
		'no-discord': data.accounts.filter((a) => !connected(a, 'discord')).length
	});

	const shown = $derived.by(() => {
		const query = search.trim().toLowerCase();
		return data.accounts.filter((account) => {
			if (filter === 'no-battlenet' && connected(account, 'battlenet')) return false;
			if (filter === 'no-discord' && connected(account, 'discord')) return false;
			if (!query) return true;
			return [
				account.username,
				account.first_name,
				account.last_name,
				...account.connections.map((c) => c.label)
			].some((value) => value?.toLowerCase().includes(query));
		});
	});

	function startRename(account: AdminAccount) {
		renaming = account.id;
		newName = account.username;
		error = '';
		done = '';
	}

	async function rename(event: SubmitEvent, account: AdminAccount) {
		event.preventDefault();
		const username = newName.trim();
		if (username === account.username) {
			renaming = null;
			return;
		}
		saving = true;
		error = '';
		done = '';
		try {
			await api.put(`/api/admin/users/${account.id}/username`, { username });
			done = `${account.username} is now ${username}.`;
			renaming = null;
			await invalidateAll();
		} catch (err) {
			error = errorMessage(err, 'Renaming failed. Please try again.');
		} finally {
			saving = false;
		}
	}

	const FILTERS: { value: Filter; label: string }[] = [
		{ value: 'all', label: 'Everyone' },
		{ value: 'no-battlenet', label: 'No Battle.net' },
		{ value: 'no-discord', label: 'No Discord' }
	];
</script>

<svelte:head>
	<title>Users | Blood Legion</title>
</svelte:head>

<div class="page-head">
	<div>
		<p class="kicker">Superusers only</p>
		<h1>Users</h1>
	</div>
</div>

<p class="muted intro">
	Every account on the site, with the Battle.net and Discord accounts its member signed in with. An
	account made by an admin, or by a password sign-in, has none until its member connects one from
	their profile. A rename follows the same rules as signing up.
</p>

{#if error}<Alert variant="error">{error}</Alert>{/if}
{#if done}<Alert variant="success">{done}</Alert>{/if}

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
	<input type="search" placeholder="Search names and accounts" bind:value={search} />
</div>

{#snippet sign_in(connection: Connection | undefined)}
	{#if connection}
		<span title={connection.subject}>{connection.label ?? connection.subject}</span>
	{:else}
		<span class="none">None</span>
	{/if}
{/snippet}

<section class="panel">
	<div class="scroll">
		<table>
			<thead>
				<tr>
					<th>Username</th>
					<th>Rank</th>
					<th>Battle.net</th>
					<th>Discord</th>
					<th>Other sign-ins</th>
					<th class="number">Characters</th>
					<th>Joined</th>
				</tr>
			</thead>
			<tbody>
				{#each shown as account (account.id)}
					<tr class:disabled={account.disabled}>
						<td>
							{#if renaming === account.id}
								<form class="rename" onsubmit={(event) => rename(event, account)}>
									<!-- svelte-ignore a11y_autofocus -->
									<input
										aria-label="New username"
										bind:value={newName}
										required
										autofocus
										onfocus={(event) => event.currentTarget.select()}
										onkeydown={(event) => {
											if (event.key === 'Escape') renaming = null;
										}}
									/>
									<Button type="submit" variant="primary" size="small" disabled={saving}>
										{saving ? 'Saving...' : 'Save'}
									</Button>
									<Button
										type="button"
										variant="secondary"
										size="small"
										disabled={saving}
										onclick={() => (renaming = null)}
									>
										Cancel
									</Button>
								</form>
							{:else}
								<span class="name">
									<a href={playerPath(account.username)}>{account.username}</a>
									{#if account.is_superuser}<span class="tag">Superuser</span>{/if}
									{#if account.disabled}<span class="tag">Disabled</span>{/if}
									<button
										type="button"
										class="link"
										disabled={renaming !== null}
										onclick={() => startRename(account)}
									>
										Rename
									</button>
								</span>
								{#if account.first_name || account.last_name}
									<div class="muted small">
										{[account.first_name, account.last_name].filter(Boolean).join(' ')}
									</div>
								{/if}
							{/if}
						</td>
						<td>{RANK_LABEL[account.guild_rank]}</td>
						<td>{@render sign_in(connected(account, 'battlenet'))}</td>
						<td>{@render sign_in(connected(account, 'discord'))}</td>
						<td>
							{#each others(account) as connection (connection.provider + connection.subject)}
								<div class:muted={connection.disconnected_at}>
									{connection.provider_name}: {connection.label ?? connection.subject}
									{#if connection.disconnected_at}(disconnected){/if}
								</div>
							{/each}
							{#if account.has_password}<div>Password</div>{/if}
							{#if !account.has_password && others(account).length === 0}
								<span class="none">—</span>
							{/if}
						</td>
						<td class="number">{account.characters}</td>
						<td class="muted">{formatDate(account.created_at)}</td>
					</tr>
				{:else}
					<tr><td colspan="7" class="muted">Nobody matches.</td></tr>
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
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-2);
	}

	.rename {
		display: flex;
		align-items: center;
		gap: var(--space-2);
	}

	.rename input {
		width: 10rem;
	}

	.link {
		padding: 0;
		color: var(--grey-text);
		font: inherit;
		font-size: var(--text-xs);
		text-decoration: underline;
		background: none;
		border: 0;
		cursor: pointer;
	}

	.link:disabled {
		cursor: default;
		opacity: 0.5;
	}

	.tag {
		padding: 0 6px;
		color: var(--grey-text);
		font-size: var(--text-xs);
		border: 1px solid var(--grey-surface);
		border-radius: var(--radius-sm);
	}

	.none {
		color: var(--grey-text);
	}

	.small {
		font-size: var(--text-xs);
	}

	tr.disabled td:first-child a {
		color: var(--grey-text);
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

	.number {
		font-variant-numeric: tabular-nums;
		text-align: right;
	}
</style>
