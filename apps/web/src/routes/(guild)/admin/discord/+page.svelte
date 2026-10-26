<script lang="ts">
	import { invalidateAll } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { RANK_LABEL } from '$lib/guild';
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

	const inOrder = $derived(
		data.check !== null &&
			data.check.missing_roles.length === 0 &&
			data.check.members.length === 0 &&
			data.check.unlinked.length === 0
	);
</script>

<svelte:head>
	<title>Discord roles | Blood Legion</title>
</svelte:head>

<div class="page-head">
	<div>
		<p class="kicker"><a href={resolve('/admin')}>Administration</a></p>
		<h1>Discord roles</h1>
	</div>
	<Button variant="secondary" disabled={checking} onclick={recheck}>
		{checking ? 'Checking...' : 'Check again'}
	</Button>
</div>

<p class="muted intro">
	Where the Discord server's roles differ from the site's ranks. The bot gives and takes no roles:
	change them in Discord, then check again.
</p>

{#if data.failure}
	<Alert variant="error">{data.failure}</Alert>
{:else if data.check}
	{#if data.check.missing_roles.length > 0}
		<Alert variant="info">
			The server has no {data.check.missing_roles.join(', ')} role yet. The bot makes it when it next
			starts or checks its roles (hourly), so it is left out below.
		</Alert>
	{/if}

	{#if inOrder}
		<Alert variant="success">
			Everyone's roles match their ranks ({data.check.linked}
			{data.check.linked === 1 ? 'member has' : 'members have'} linked Discord).
		</Alert>
	{/if}

	{#if data.check.members.length > 0}
		<section>
			<h2>Linked members</h2>
			<p class="muted">
				{data.check.members.length} of the {data.check.linked} members who linked Discord.
			</p>
			<ul class="list">
				{#each data.check.members as member (member.discord_id)}
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
								<span class="tag give">Give {member.role}</span>
							{/if}
							{#each member.other_roles as role (role)}
								<span class="tag extra">Also has {role}</span>
							{/each}
						</div>
					</li>
				{/each}
			</ul>
		</section>
	{/if}

	{#if data.check.unlinked.length > 0}
		<section>
			<h2>Roles without a linked account</h2>
			<p class="muted">
				People in the server holding one of the bot's roles who have not linked Discord on the site.
			</p>
			<ul class="list">
				{#each data.check.unlinked as person (person.discord_id)}
					<li>
						<div class="who">
							<span class="name">{person.discord_name}</span>
						</div>
						<div class="needs">
							{#each person.roles as role (role)}
								<span class="tag extra">{role}</span>
							{/each}
						</div>
					</li>
				{/each}
			</ul>
		</section>
	{/if}
{/if}

<style>
	.intro {
		max-width: 44rem;
	}

	.kicker a {
		color: inherit;
	}

	.list {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		max-width: 44rem;
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
		background-color: var(--grey-surface);
		border-radius: var(--radius-md);
	}

	.tag.give {
		color: var(--red-text);
		background-color: var(--red-bg);
	}

	.tag.extra {
		color: var(--accent-text);
		background-color: var(--accent-bg);
	}
</style>
