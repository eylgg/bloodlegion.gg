<script lang="ts">
	import { invalidateAll } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { RANK_LABEL } from '$lib/guild';
	import { api, errorMessage } from '$lib/api';
	import Button from '$lib/components/Button.svelte';
	import Alert from '$lib/components/Alert.svelte';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();
	let checking = $state(false);

	async function recheck() {
		checking = true;
		try {
			await invalidateAll();
		} finally {
			checking = false;
		}
	}

	const overview = $derived(data.overview);
	const report = $derived(overview?.report ?? null);
	const status = $derived(overview?.status ?? null);

	/** The running instance writes every 15 seconds; a minute without means it has gone. */
	const SEEN_WITHIN_MS = 60_000;

	const botState = $derived.by(() => {
		if (!overview?.set_up) return 'unset';
		if (!status?.running_since) return 'stopped';
		const seen = status.seen_at ? Date.now() - new Date(status.seen_at).getTime() : Infinity;
		return seen < SEEN_WITHIN_MS ? 'running' : 'silent';
	});

	const relative = new Intl.RelativeTimeFormat(undefined, { numeric: 'auto' });
	/** "3 minutes ago", "yesterday". */
	function ago(iso: string): string {
		const seconds = (new Date(iso).getTime() - Date.now()) / 1000;
		const units: [Intl.RelativeTimeFormatUnit, number][] = [
			['day', 86_400],
			['hour', 3_600],
			['minute', 60]
		];
		for (const [unit, size] of units) {
			if (Math.abs(seconds) >= size) return relative.format(Math.round(seconds / size), unit);
		}
		return relative.format(Math.round(seconds), 'second');
	}

	let fixing = $state<'categories' | 'roles' | null>(null);
	let fixError = $state('');
	let fixed = $state('');

	/** Moves the bot's categories or roles into order, among their own slots, when asked. */
	async function fix(list: 'categories' | 'roles') {
		const what = list === 'roles' ? "the bot's roles" : "the bot's categories";
		if (!confirm(`Put ${what} in order? Only they move; everything else keeps its place.`)) return;
		fixing = list;
		fixError = '';
		fixed = '';
		try {
			await api.post(`/api/admin/discord/fix/${list}`);
			await invalidateAll();
			fixed = list;
		} catch (err) {
			fixError = errorMessage(err, 'That did not work. Please try again.');
		} finally {
			fixing = null;
		}
	}
</script>

<svelte:head>
	<title>Discord | Blood Legion</title>
</svelte:head>

<div class="page-head">
	<div>
		<p class="kicker"><a href={resolve('/admin')}>Administration</a></p>
		<h1>Discord</h1>
	</div>
	<Button variant="secondary" disabled={checking} onclick={recheck}>
		{checking ? 'Checking...' : 'Check again'}
	</Button>
</div>

<p class="muted intro">
	How the bot and the server stand, read from Discord now. The bot gives and takes no roles, removes
	nothing, and orders nothing on its own: fix what is listed here in Discord (or with a button
	here), then check again.
</p>

{#if data.failure}
	<Alert variant="error">{data.failure}</Alert>
{:else if overview && status}
	<section>
		<h2>The bot</h2>
		<dl class="facts">
			<div>
				<dt>State</dt>
				<dd>
					{#if botState === 'unset'}
						<span class="tag warn">Not set up</span>
						<a href={resolve('/admin')}>Set its token and server</a>
					{:else if botState === 'running'}
						<span class="tag ok">Running</span>
					{:else if botState === 'silent'}
						<span class="tag warn">Not seen lately</span>
						its server may have stopped
					{:else}
						<span class="tag warn">Stopped</span>
					{/if}
				</dd>
			</div>
			{#if status.bot_user}
				<div>
					<dt>Signed in as</dt>
					<dd>{status.bot_user}</dd>
				</div>
			{/if}
			{#if status.running_since}
				<div>
					<dt>Running since</dt>
					<dd>{ago(status.running_since)}</dd>
				</div>
			{/if}
			{#if status.seen_at}
				<div>
					<dt>Last seen</dt>
					<dd>{ago(status.seen_at)}</dd>
				</div>
			{/if}
			{#if status.checked_at}
				<div>
					<dt>Roles and categories checked</dt>
					<dd>{ago(status.checked_at)}</dd>
				</div>
			{/if}
			{#if status.last_error && status.last_error_at}
				<div class="wide">
					<dt>Last error, {ago(status.last_error_at)}</dt>
					<dd><code>{status.last_error}</code></dd>
				</div>
			{/if}
		</dl>
	</section>

	{#if overview.failure}
		<Alert variant="error">Discord could not be read: {overview.failure}</Alert>
	{/if}

	{#if report}
		<section>
			<h2>{report.server_name}</h2>
			<dl class="facts">
				<div>
					<dt>People</dt>
					<dd>{report.people}</dd>
				</div>
				<div>
					<dt>Bots</dt>
					<dd>{report.bots}</dd>
				</div>
				<div>
					<dt>Linked Discord on the site</dt>
					<dd>{report.linked}, {report.linked_in_server} of them in the server</dd>
				</div>
				<div>
					<dt>Announcements</dt>
					<dd>
						{#if report.announcements}
							#{report.announcements.name}{report.announcements.category
								? ` in ${report.announcements.category}`
								: ''}
							<code class="id">{report.announcements.id}</code>, {overview.posts.stored}
							{overview.posts.stored === 1 ? 'post' : 'posts'} kept{overview.posts.latest
								? `, latest ${ago(overview.posts.latest)}`
								: ''}
						{:else}
							<span class="tag warn">No channel</span>
						{/if}
					</dd>
				</div>
			</dl>
		</section>

		{#if fixError}<Alert variant="error">{fixError}</Alert>{/if}

		<section>
			<div class="head">
				<h2>Roles</h2>
				{#if report.roles_in_order}
					<span class="tag ok">In order</span>
				{:else}
					<span class="tag warn">Out of order</span>
				{/if}
			</div>
			<p class="muted">
				The bot's roles, top to bottom, as they should stand in Server Settings, Roles. Holders:
				everyone in the server with the role. Due: linked members in the server whose rank calls for
				it.
			</p>
			{#if fixed === 'roles'}<Alert variant="success">The roles are in order.</Alert>{/if}
			{#if report.roles_out_of_reach.length > 0}
				<Alert variant="info">
					{report.roles_out_of_reach.join(', ')}
					{report.roles_out_of_reach.length === 1 ? 'is' : 'are'} not below the bot's own role, so it
					cannot move {report.roles_out_of_reach.length === 1 ? 'it' : 'them'}. Drag the bot's role
					above them first.
				</Alert>
			{/if}
			<table>
				<thead>
					<tr><th>Role</th><th>ID</th><th class="num">Holders</th><th class="num">Due</th></tr>
				</thead>
				<tbody>
					{#each report.roles as role (role.name)}
						<tr>
							<td>{role.name}</td>
							<td>
								{#if role.id}<code class="id">{role.id}</code>{:else}<span class="tag warn"
										>Missing</span
									>{/if}
							</td>
							<td class="num">{role.id ? role.holders : '–'}</td>
							<td class="num">{role.due}</td>
						</tr>
					{/each}
				</tbody>
			</table>
			{#if !report.roles_in_order}
				<div class="actions">
					<Button variant="secondary" disabled={fixing !== null} onclick={() => fix('roles')}>
						{fixing === 'roles' ? 'Fixing...' : 'Fix order'}
					</Button>
				</div>
			{/if}
		</section>

		<section>
			<div class="head">
				<h2>Categories</h2>
				{#if report.categories_in_order}
					<span class="tag ok">In order</span>
				{:else}
					<span class="tag warn">Out of order</span>
				{/if}
			</div>
			<p class="muted">The bot's categories, top to bottom, as they should stand in the server.</p>
			{#if fixed === 'categories'}<Alert variant="success">The categories are in order.</Alert>{/if}
			<table>
				<thead>
					<tr><th>Category</th><th>ID</th><th class="num">Channels</th></tr>
				</thead>
				<tbody>
					{#each report.categories as category (category.name)}
						<tr>
							<td>{category.name}</td>
							<td>
								{#if category.id}<code class="id">{category.id}</code>{:else}<span class="tag warn"
										>Missing</span
									>{/if}
							</td>
							<td class="num">{category.id ? category.channels : '–'}</td>
						</tr>
					{/each}
				</tbody>
			</table>
			{#if !report.categories_in_order}
				<div class="actions">
					<Button variant="secondary" disabled={fixing !== null} onclick={() => fix('categories')}>
						{fixing === 'categories' ? 'Fixing...' : 'Fix order'}
					</Button>
				</div>
			{/if}
		</section>

		<section>
			<div class="head">
				<h2>Linked members</h2>
				{#if report.members.length === 0}
					<span class="tag ok">All have their role</span>
				{:else}
					<span class="tag warn">{report.members.length} out of line</span>
				{/if}
			</div>
			<p class="muted">
				Members who linked Discord on the site and are not in the server, lack their rank's role, or
				hold another of the bot's roles besides.
			</p>
			{#if report.members.length > 0}
				<ul class="list">
					{#each report.members as member (member.discord_id)}
						<li>
							<div class="who">
								<span class="name">{member.username}</span>
								<span class="muted small">
									{RANK_LABEL[member.rank]} · {member.discord_name ?? 'not in the server'}
								</span>
							</div>
							<div class="needs">
								{#if member.discord_name === null}
									<span class="tag">Not in the server</span>
								{:else if !member.has_role}
									<span class="tag warn">Give {member.role}</span>
								{/if}
								{#each member.other_roles as role (role)}
									<span class="tag extra">Also has {role}</span>
								{/each}
							</div>
						</li>
					{/each}
				</ul>
			{/if}
		</section>

		<section>
			<div class="head">
				<h2>Roles without a linked account</h2>
				{#if report.unlinked.length === 0}
					<span class="tag ok">None</span>
				{:else}
					<span class="tag warn">{report.unlinked.length}</span>
				{/if}
			</div>
			<p class="muted">
				People in the server holding one of the bot's roles who have not linked Discord on the site.
			</p>
			{#if report.unlinked.length > 0}
				<ul class="list">
					{#each report.unlinked as person (person.discord_id)}
						<li>
							<div class="who"><span class="name">{person.discord_name}</span></div>
							<div class="needs">
								{#each person.roles as role (role)}
									<span class="tag extra">{role}</span>
								{/each}
							</div>
						</li>
					{/each}
				</ul>
			{/if}
		</section>
	{/if}
{/if}

<style>
	.intro,
	section {
		max-width: 44rem;
	}

	.kicker a {
		color: inherit;
	}

	.head {
		display: flex;
		flex-wrap: wrap;
		align-items: baseline;
		gap: var(--space-2) var(--space-3);
	}

	.actions {
		display: flex;
		justify-content: flex-end;
		margin-top: var(--space-3);
	}

	.facts {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(12rem, 1fr));
		gap: var(--space-3) var(--space-4);
		margin: 0;
	}

	.facts .wide {
		grid-column: 1 / -1;
	}

	.facts dt {
		color: var(--grey-text);
		font-size: var(--text-sm);
	}

	.facts dd {
		margin: 0;
	}

	code {
		font-family: var(--font-mono);
		font-size: var(--text-sm);
		overflow-wrap: anywhere;
	}

	table {
		width: 100%;
		border-collapse: collapse;
	}

	th,
	td {
		padding: var(--space-1) var(--space-2);
		text-align: left;
		border-bottom: 1px solid var(--grey-surface);
	}

	th {
		color: var(--grey-text);
		font-size: var(--text-sm);
		font-weight: 500;
	}

	.num {
		text-align: right;
		font-variant-numeric: tabular-nums;
	}

	/* Discord ids: long, and for copying into Discord's search or the API. */
	.id {
		color: var(--grey-text);
		user-select: all;
	}

	.list {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		margin: 0;
		padding: 0;
		list-style: none;
	}

	.list li {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-2) var(--space-3);
		padding: var(--space-3) var(--space-4);
		background-color: var(--grey-bg);
		border: 1px solid var(--grey-surface);
		border-radius: var(--radius-lg);
	}

	.who {
		display: flex;
		flex: 1;
		flex-direction: column;
	}

	.name {
		font-weight: 600;
	}

	.small {
		font-size: var(--text-sm);
	}

	.needs {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-1);
	}

	.tag {
		padding: 0 var(--space-2);
		color: var(--grey-text);
		font-size: var(--text-sm);
		white-space: nowrap;
		background-color: var(--grey-surface);
		border-radius: var(--radius-md);
	}

	.tag.ok {
		color: var(--green-text);
		background-color: var(--green-bg);
	}

	.tag.warn {
		color: var(--red-text);
		background-color: var(--red-bg);
	}

	.tag.extra {
		color: var(--accent-text);
		background-color: var(--accent-bg);
	}
</style>
