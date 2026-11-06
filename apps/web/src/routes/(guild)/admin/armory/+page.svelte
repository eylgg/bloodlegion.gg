<script lang="ts">
	import { api, errorMessage } from '$lib/api';
	import { armoryUrl } from '$lib/guild';
	import Alert from '$lib/components/Alert.svelte';
	import Button from '$lib/components/Button.svelte';
	import CharacterLink from '$lib/components/guild/CharacterLink.svelte';
	import type { ArmoryChecked } from '$lib/types';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();

	let checked = $state<ArmoryChecked[] | null>(null);
	let running = $state<'check' | 'fill' | null>(null);
	let error = $state('');

	async function run(fill: boolean) {
		running = fill ? 'fill' : 'check';
		error = '';
		try {
			checked = await api.post<ArmoryChecked[]>('/api/admin/armory-check', { fill });
		} catch (err) {
			error = errorMessage(err, 'The check failed. Please try again.');
		} finally {
			running = null;
		}
	}

	const findRace = (slug: string | null) => data.races.find((r) => r.slug === slug)?.name ?? slug;
	const findClass = (slug: string | null) =>
		data.classes.find((c) => c.slug === slug)?.name ?? slug;
	/** A value as the page shows it: a race or class by name, the rest as it is. */
	const shown = (field: string, value: string) =>
		field === 'race' ? findRace(value) : field === 'class' ? findClass(value) : value;

	const differing = $derived(checked?.filter((c) => c.differences.length > 0) ?? []);
	const filled = $derived(checked?.filter((c) => c.filled.length > 0) ?? []);
	// What filling in would fix: a sex that differs (a race the site lacks shows no difference).
	const fillable = $derived(
		checked?.filter((c) => c.filled.length === 0 && c.differences.some((d) => d.field === 'sex')) ??
			[]
	);
	const missing = $derived(checked?.filter((c) => !c.found && !c.error) ?? []);
	const failed = $derived(checked?.filter((c) => c.error) ?? []);
</script>

<svelte:head>
	<title>Armory check | Blood Legion</title>
</svelte:head>

<div class="page-head">
	<div>
		<p class="kicker">Superusers only</p>
		<h1>Armory check</h1>
	</div>
</div>

<p class="muted intro">
	Looks every character on the roster up in Blizzard's armory by its name. Checking only lists where
	the site and the armory disagree; filling in also takes from the armory a race nobody recorded,
	the sex (most recorded so far are the old default, female), and a level the site has lower. A
	level the site has higher is left alone: the armory catches up later. Anything else that differs
	(a class, a ruleset, a race already recorded) is for you to fix on the character's page.
</p>

<div class="actions">
	<Button variant="secondary" disabled={running !== null} onclick={() => run(false)}>
		{running === 'check' ? 'Checking...' : 'Check'}
	</Button>
	<Button variant="primary" disabled={running !== null} onclick={() => run(true)}>
		{running === 'fill' ? 'Filling in...' : 'Check and fill in'}
	</Button>
</div>

{#if error}<Alert variant="error">{error}</Alert>{/if}

{#if checked}
	<p class="muted">
		{checked.length} characters, {checked.length - missing.length - failed.length} in the armory.
	</p>

	{#if filled.length > 0}
		<section class="panel">
			<h2>Filled in <span class="count">{filled.length}</span></h2>
			<ul>
				{#each filled as character (character.id)}
					<li>
						<CharacterLink
							firstName={character.first_name}
							lastName={character.last_name}
							cls={character.class}
						/>
						<span class="muted">{character.filled.join(', ')}</span>
					</li>
				{/each}
			</ul>
		</section>
	{/if}

	{#if fillable.length > 0 && filled.length === 0}
		<p class="muted">
			{fillable.length}
			{fillable.length === 1 ? 'character has' : 'characters have'} a different sex in the armory: Check
			and fill in takes the armory's.
		</p>
	{/if}

	<section class="panel">
		<h2>Different in the armory <span class="count">{differing.length}</span></h2>
		{#if differing.length > 0}
			<div class="scroll">
				<table>
					<thead>
						<tr>
							<th>Character</th>
							<th>What</th>
							<th>Here</th>
							<th>Armory</th>
						</tr>
					</thead>
					<tbody>
						{#each differing as character (character.id)}
							{#each character.differences as difference, i (difference.field)}
								<tr>
									<td>
										{#if i === 0}
											<CharacterLink
												firstName={character.first_name}
												lastName={character.last_name}
												cls={character.class}
											/>
											<!-- eslint-disable svelte/no-navigation-without-resolve -- Blizzard's site -->
											<a
												class="small"
												href={armoryUrl(character.first_name, character.last_name)}
												target="_blank"
												rel="noreferrer">Armory</a
											>
											<!-- eslint-enable svelte/no-navigation-without-resolve -->
										{/if}
									</td>
									<td>{difference.field}</td>
									<td>{shown(difference.field, difference.site)}</td>
									<td>{shown(difference.field, difference.armory)}</td>
								</tr>
							{/each}
						{/each}
					</tbody>
				</table>
			</div>
		{:else}
			<p class="muted">Nothing: the site agrees with the armory.</p>
		{/if}
	</section>

	{#if missing.length > 0}
		<section class="panel">
			<h2>Not in the armory <span class="count">{missing.length}</span></h2>
			<p class="muted">No character of the name: a typo, a rename, or a pug from elsewhere.</p>
			<ul>
				{#each missing as character (character.id)}
					<li>
						<CharacterLink
							firstName={character.first_name}
							lastName={character.last_name}
							cls={character.class}
						/>
					</li>
				{/each}
			</ul>
		</section>
	{/if}

	{#if failed.length > 0}
		<section class="panel">
			<h2>Lookups that failed <span class="count">{failed.length}</span></h2>
			<ul>
				{#each failed as character (character.id)}
					<li>
						{character.first_name}
						{character.last_name}: <span class="muted">{character.error}</span>
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

	.actions {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-2);
	}

	h2 {
		font-size: var(--text-lg);
	}

	.count {
		color: var(--grey-text);
		font-weight: 400;
	}

	ul {
		margin: 0;
		padding-left: var(--space-4);
	}

	.small {
		margin-left: var(--space-2);
		font-size: var(--text-sm);
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
</style>
