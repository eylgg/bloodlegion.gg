<script lang="ts">
	import { invalidateAll } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { RANK_LABEL } from '$lib/guild';
	import { api, errorMessage } from '$lib/api';
	import Button from '$lib/components/Button.svelte';
	import Alert from '$lib/components/Alert.svelte';
	import ConfirmButton from '$lib/components/ConfirmButton.svelte';
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

	let fixing = $state<string | null>(null);
	let fixError = $state('');
	let fixed = $state<string | null>(null);

	/**
	 * Runs one of the fixes (`roles`, `categories`, `create`, `announcements`, `permissions`,
	 * `sync/<id>`).
	 */
	async function fix(action: string) {
		fixing = action;
		fixError = '';
		fixed = null;
		try {
			await api.post(`/api/admin/discord/fix/${action}`);
			await invalidateAll();
			fixed = action;
		} catch (err) {
			fixError = errorMessage(err, 'That did not work. Please try again.');
		} finally {
			fixing = null;
		}
	}

	/** For each of the server's other roles, which of the bot's roles to give its holders. */
	let giveTo = $state<Record<string, string>>({});
	let given = $state<{ from: string; to: string; count: number } | null>(null);

	/** Gives the bot's role `to` (its key) to everyone holding the server's role `from`. */
	async function give(from: { id: string; name: string }, to: string) {
		fixing = `give/${from.id}`;
		fixError = '';
		fixed = null;
		given = null;
		try {
			const result = await api.post<{ given: number }>('/api/admin/discord/fix/give', {
				from: from.id,
				to
			});
			await invalidateAll();
			const name = report?.roles.find((r) => r.key === to)?.name ?? to;
			given = { from: from.name, to: name, count: result.given };
		} catch (err) {
			fixError = errorMessage(err, 'That did not work. Please try again.');
		} finally {
			fixing = null;
		}
	}

	/** The bot's roles that are in the server, which can be given. */
	const givable = $derived(report?.roles.filter((r) => r.id) ?? []);

	/** The bot's roles, categories, and announcements channels the server lacks. */
	const missing = $derived(
		report
			? [
					...report.roles.filter((r) => !r.id).map((r) => `the ${r.name} role`),
					...report.categories.filter((c) => !c.id).map((c) => c.name),
					...report.announcements.filter((c) => !c.id).map((c) => `#${c.name}`)
				]
			: []
	);

	const announcementsToFix = $derived(
		report?.announcements.filter((channel) => channel.id && (channel.named || channel.category)) ??
			[]
	);
	/** The bot's categories and announcements channels whose permission entries differ. */
	const permissionsToFix = $derived(
		report?.permissions?.filter((channel) => channel.differing.length > 0) ?? []
	);
	const announcementsInPlace = $derived(
		!!report &&
			report.announcements_in_order &&
			report.announcements.every((c) => c.id && !c.named && !c.category)
	);

	/** The groups of everyone in the server, in the order the page lists them. */
	const groups = $derived(
		report
			? [
					{ title: 'Linked to the site', people: report.everyone.linked },
					{ title: "Not linked, with the bot's roles", people: report.everyone.unlinked_ours },
					{ title: 'Not linked, with other roles only', people: report.everyone.unlinked_others },
					{ title: 'No roles', people: report.everyone.no_roles },
					{ title: 'Bots', people: report.everyone.bots }
				]
			: []
	);
</script>

{#snippet createMissing()}
	<ConfirmButton
		disabled={fixing !== null}
		question={`Make ${missing.join(', ')}? One already there under that name is taken over instead.`}
		confirmLabel="Create"
		confirmVariant="primary"
		onconfirm={() => fix('create')}
	>
		{fixing === 'create' ? 'Creating...' : 'Create missing'}
	</ConfirmButton>
{/snippet}

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
	How the bot and the server stand, read from Discord now. The bot changes nothing in Discord on its
	own, and never takes a role away or removes anything: fix what is listed here in Discord (or with
	a button here), then check again.
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
					<dt>Posts kept for the site</dt>
					<dd>
						{overview.posts.stored}{overview.posts.latest
							? `, latest ${ago(overview.posts.latest)}`
							: ''}
					</dd>
				</div>
			</dl>
		</section>

		{#if fixError}<Alert variant="error">{fixError}</Alert>{/if}
		{#if fixed === 'create'}<Alert variant="success">What was missing is made.</Alert>{/if}

		<section>
			<div class="head">
				<h2>The bot's role</h2>
				{#if !report.bot_role}
					<span class="tag warn">None</span>
				{:else}
					{#if report.bot_role.above.length === 0}
						<span class="tag ok">At the top</span>
					{:else}
						<span class="tag warn">Not at the top</span>
					{/if}
					{#if report.bot_role.shown_separately}
						<span class="tag ok">Shown separately</span>
					{:else}
						<span class="tag warn">Not shown separately</span>
					{/if}
					{#if report.bot_role.mentionable}
						<span class="tag warn">Mentionable</span>
					{:else}
						<span class="tag ok">Not mentionable</span>
					{/if}
				{/if}
			</div>
			<p class="muted">
				The bot's highest role should be the server's top role, since it can manage only the roles
				below it, shown separately in the member list, so the bot stands apart from everyone else,
				and not mentionable by anyone, since mentioning it only pings the bot.
			</p>
			{#if report.bot_role}
				<dl class="facts">
					<div>
						<dt>Role</dt>
						<dd>{report.bot_role.name} <code class="id">{report.bot_role.id}</code></dd>
					</div>
					<div>
						<dt>Above it</dt>
						<dd>{report.bot_role.above.length ? report.bot_role.above.join(', ') : 'Nothing'}</dd>
					</div>
				</dl>
			{/if}
			{#if !report.bot_role || report.bot_role.above.length > 0}
				<Alert variant="info">
					There is no button for this: Discord lets no one move their own top role, the bot
					included, nor any role above it. In Server Settings, Roles, the server's owner (or someone
					with a role above the bot's) drags the bot's role to the top.
				</Alert>
			{/if}
			{#if report.bot_role && !report.bot_role.shown_separately}
				<Alert variant="info">
					There is no button for this either: Discord lets no one change their own top role. In
					Server Settings, Roles, the server's owner (or someone with a role above the bot's) opens
					{report.bot_role.name} and turns on "Display role members separately from online members".
				</Alert>
			{/if}
			{#if report.bot_role?.mentionable}
				<Alert variant="info">
					In Server Settings, Roles, the server's owner (or someone with a role above the bot's)
					opens {report.bot_role.name} and turns off "Allow anyone to mention this role". No button, for
					the same reason.
				</Alert>
			{/if}
		</section>

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
				it. The bot makes none on its own: Create missing makes them.
			</p>
			{#if fixed === 'roles'}<Alert variant="success">The roles are in order.</Alert>{/if}
			{#if report.roles_out_of_reach.length > 0}
				<Alert variant="info">
					The bot cannot move {report.roles_out_of_reach.join(', ')}: they are not below its own
					role (see the bot's role above).
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
			{#if report.roles.some((r) => !r.id) || !report.roles_in_order}
				<div class="actions">
					{#if report.roles.some((r) => !r.id)}{@render createMissing()}{/if}
					{#if !report.roles_in_order}
						<ConfirmButton
							disabled={fixing !== null}
							question="Put the bot's roles in order? Only they move; everything else keeps its place."
							confirmLabel="Fix"
							confirmVariant="primary"
							onconfirm={() => fix('roles')}
						>
							{fixing === 'roles' ? 'Fixing...' : 'Fix order'}
						</ConfirmButton>
					{/if}
				</div>
			{/if}
		</section>

		<section>
			<h2>Other roles</h2>
			<p class="muted">
				The server's roles that are not the bot's (nor ones an integration keeps, like the booster
				role). To move people off one, give everyone holding it one of the bot's roles here, then
				remove the old role by hand in Server Settings, Roles. Giving takes no role from anyone.
			</p>
			{#if given}
				<Alert variant="success">
					Gave {given.to} to {given.count}
					{given.count === 1 ? 'person' : 'people'} holding {given.from}.
				</Alert>
			{/if}
			{#if report.other_roles.length === 0}
				<p class="muted">None.</p>
			{:else}
				<table>
					<thead>
						<tr><th>Role</th><th>ID</th><th class="num">Holders</th><th>Give them</th></tr>
					</thead>
					<tbody>
						{#each report.other_roles as role (role.id)}
							{@const to = giveTo[role.id] ?? ''}
							{@const toName = givable.find((r) => r.key === to)?.name}
							<tr>
								<td>{role.name}</td>
								<td><code class="id">{role.id}</code></td>
								<td class="num">{role.holders}</td>
								<td class="give">
									<select
										aria-label={`Role to give everyone holding ${role.name}`}
										value={to}
										disabled={role.holders === 0 || givable.length === 0}
										onchange={(event) => (giveTo[role.id] = event.currentTarget.value)}
									>
										<option value="">Pick a role</option>
										{#each givable as ours (ours.key)}
											<option value={ours.key}>{ours.name}</option>
										{/each}
									</select>
									<ConfirmButton
										size="small"
										disabled={!to || fixing !== null}
										question={`Give ${toName} to everyone holding ${role.name}?`}
										confirmLabel="Give"
										confirmVariant="primary"
										onconfirm={() => give(role, to)}
									>
										{fixing === `give/${role.id}` ? 'Giving...' : 'Give'}
									</ConfirmButton>
								</td>
							</tr>
						{/each}
					</tbody>
				</table>
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
			<p class="muted">
				The bot's categories, top to bottom, as they should stand in the server. The bot makes none
				on its own: Create missing makes them, with the bot's permissions.
			</p>
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
			{#if report.categories.some((c) => !c.id) || !report.categories_in_order}
				<div class="actions">
					{#if report.categories.some((c) => !c.id)}{@render createMissing()}{/if}
					{#if !report.categories_in_order}
						<ConfirmButton
							disabled={fixing !== null}
							question="Put the bot's categories in order? Only they move; everything else keeps its place."
							confirmLabel="Fix"
							confirmVariant="primary"
							onconfirm={() => fix('categories')}
						>
							{fixing === 'categories' ? 'Fixing...' : 'Fix order'}
						</ConfirmButton>
					{/if}
				</div>
			{/if}
		</section>

		<section>
			<div class="head">
				<h2>Announcements</h2>
				{#if announcementsInPlace}
					<span class="tag ok">In place</span>
				{:else}
					<span class="tag warn">Out of place</span>
				{/if}
			</div>
			<p class="muted">
				At the top of the channel list, in no category, in this order, where only the leader and
				officers post. #public-announcements shows on the site's front page. The bot makes none on
				its own: Create missing makes them.
			</p>
			{#if fixed === 'announcements'}
				<Alert variant="success">The announcements channels are in place.</Alert>
			{/if}
			{#if !report.announcements_in_order}
				<Alert variant="info">The ones at the top of the list are out of order.</Alert>
			{/if}
			<table>
				<thead>
					<tr><th>Channel</th><th>ID</th><th></th></tr>
				</thead>
				<tbody>
					{#each report.announcements as channel (channel.name)}
						<tr>
							<td>#{channel.name}</td>
							<td>
								{#if channel.id}<code class="id">{channel.id}</code>{:else}<span class="tag warn"
										>Missing</span
									>{/if}
							</td>
							<td class="needs">
								{#if channel.named}<span class="tag warn">Named #{channel.named}</span>{/if}
								{#if channel.category}<span class="tag warn">In {channel.category}</span>{/if}
							</td>
						</tr>
					{/each}
				</tbody>
			</table>
			{#if report.announcements.some((c) => !c.id) || announcementsToFix.length > 0 || !report.announcements_in_order}
				<div class="actions">
					{#if report.announcements.some((c) => !c.id)}{@render createMissing()}{/if}
					{#if announcementsToFix.length > 0 || !report.announcements_in_order}
						<ConfirmButton
							disabled={fixing !== null}
							question="Move the announcements channels to the top of the list, out of any category, under their own names, in this order? Only they move."
							confirmLabel="Fix"
							confirmVariant="primary"
							onconfirm={() => fix('announcements')}
						>
							{fixing === 'announcements' ? 'Fixing...' : 'Fix'}
						</ConfirmButton>
					{/if}
				</div>
			{/if}
		</section>

		<section>
			<div class="head">
				<h2>Permissions</h2>
				{#if !report.permissions}
					<span class="tag warn">Roles missing</span>
				{:else if permissionsToFix.length === 0}
					<span class="tag ok">All set</span>
				{:else}
					<span class="tag warn">{permissionsToFix.length} to set</span>
				{/if}
			</div>
			<p class="muted">
				The bot's own permission entries on its categories and announcements channels: who may see
				each, and who may post announcements. Entries added by hand are left alone, and these are
				set only with the button.
			</p>
			{#if fixed === 'permissions'}
				<Alert variant="success">The bot's permissions are set.</Alert>
			{/if}
			{#if !report.permissions}
				<Alert variant="info">
					They name the bot's roles, so they are checked once the server has all of them.
				</Alert>
			{:else}
				<ul class="list">
					{#each report.permissions as channel (channel.id)}
						<li>
							<div class="who">
								<span class="name">{channel.category ? channel.name : `#${channel.name}`}</span>
								<span class="muted small">
									{channel.category ? 'Category' : 'Channel'} · <code class="id">{channel.id}</code>
								</span>
							</div>
							<div class="needs">
								{#if channel.differing.length === 0}
									<span class="tag ok">Set</span>
								{:else}
									{#each channel.differing as who (who)}
										<span class="tag warn">{who}</span>
									{/each}
								{/if}
							</div>
						</li>
					{/each}
				</ul>
				{#if permissionsToFix.length > 0}
					<div class="actions">
						<ConfirmButton
							disabled={fixing !== null}
							question="Set the bot's entries where they differ? Only its own entries change; those added by hand stay."
							confirmLabel="Set"
							confirmVariant="primary"
							onconfirm={() => fix('permissions')}
						>
							{fixing === 'permissions' ? 'Setting...' : 'Set permissions'}
						</ConfirmButton>
					</div>
				{/if}
			{/if}
		</section>

		<section>
			<div class="head">
				<h2>Synced channels</h2>
				{#if report.unsynced.length === 0}
					<span class="tag ok">All synced</span>
				{:else}
					<span class="tag warn">{report.unsynced.length} not synced</span>
				{/if}
			</div>
			<p class="muted">
				Channels in the bot's categories follow the category's permissions while synced with it.
				These have permissions of their own, so they no longer follow who may see the category.
			</p>
			{#if fixed?.startsWith('sync/')}
				<Alert variant="success">Synced with its category.</Alert>
			{/if}
			{#if report.unsynced.length > 0}
				<ul class="list">
					{#each report.unsynced as channel (channel.id)}
						<li>
							<div class="who">
								<span class="name">#{channel.name}</span>
								<span class="muted small">
									in {channel.category} · <code class="id">{channel.id}</code>
								</span>
							</div>
							<ConfirmButton
								size="small"
								disabled={fixing !== null}
								question={`Sync #${channel.name} with ${channel.category}? Its own permissions are replaced with the category's.`}
								confirmLabel="Sync"
								confirmVariant="primary"
								onconfirm={() => fix(`sync/${channel.id}`)}
							>
								{fixing === `sync/${channel.id}` ? 'Syncing...' : 'Sync'}
							</ConfirmButton>
						</li>
					{/each}
				</ul>
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

		<section>
			<h2>Everyone in the server</h2>
			<p class="muted">
				Everyone in the server, by how they stand with the site, with their roles top first.
			</p>
			{#each groups as group (group.title)}
				<details class="group">
					<summary>
						{group.title} <span class="muted">({group.people.length})</span>
					</summary>
					{#if group.people.length === 0}
						<p class="muted small">Nobody.</p>
					{:else}
						<ul class="list">
							{#each group.people as person (person.discord_id)}
								<li>
									<div class="who">
										<span class="name">{person.discord_name}</span>
										<span class="muted small">
											{#if person.username}{person.username} on the site ·
											{/if}<code class="id">{person.discord_id}</code>
										</span>
									</div>
									<div class="needs">
										{#each person.roles as role (role)}
											<span class="tag extra">{role}</span>
										{/each}
									</div>
								</li>
							{/each}
						</ul>
					{/if}
				</details>
			{/each}
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

	.group {
		margin-top: var(--space-3);
	}

	.group summary {
		cursor: pointer;
		font-weight: 600;
	}

	.give {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-2);
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
