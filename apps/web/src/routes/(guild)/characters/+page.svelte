<script lang="ts">
	import { classIcon } from '$lib/wow/icons';
	import Button from '$lib/components/Button.svelte';
	import CharacterForm from '$lib/components/guild/CharacterForm.svelte';
	import CharacterLink from '$lib/components/guild/CharacterLink.svelte';
	import CharacterSpecs from '$lib/components/guild/CharacterSpecs.svelte';
	import type { GuildCharacter } from '$lib/types';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();

	// svelte-ignore state_referenced_locally
	let characters = $state<GuildCharacter[]>(data.characters);
	let query = $state('');
	let classFilter = $state('');
	let adding = $state(false);

	const shown = $derived(
		characters.filter(
			(c) =>
				(!classFilter || c.class === classFilter) &&
				`${c.first_name} ${c.last_name} ${c.username ?? ''}`
					.toLowerCase()
					.includes(query.trim().toLowerCase())
		)
	);

	function added(character: GuildCharacter) {
		characters = [...characters, character].sort((a, b) =>
			`${a.first_name} ${a.last_name}`.localeCompare(`${b.first_name} ${b.last_name}`)
		);
		adding = false;
	}
</script>

<svelte:head>
	<title>Characters | Blood Legion</title>
</svelte:head>

<div class="page-head">
	<div>
		<p class="kicker">The guild</p>
		<h1>Characters</h1>
	</div>
	{#if data.officer && !adding}
		<Button variant="primary" onclick={() => (adding = true)}>Add a character</Button>
	{/if}
</div>

{#if adding}
	<CharacterForm owners={data.owners} onsaved={added} oncancel={() => (adding = false)} />
{/if}

<div class="row">
	<label class="field">
		Search
		<input type="search" bind:value={query} placeholder="Name or player" />
	</label>
	<label class="field">
		Class
		<select bind:value={classFilter}>
			<option value="">Every class</option>
			{#each data.classes as wowClass (wowClass.slug)}
				<option value={wowClass.slug}>{wowClass.name}</option>
			{/each}
		</select>
	</label>
</div>

<p class="muted">{shown.length} of {characters.length}</p>

<ul class="characters">
	{#each shown as character (character.id)}
		<li>
			<img src={classIcon(character.class)} alt="" width="32" height="32" />
			<div>
				<CharacterLink
					id={character.id}
					firstName={character.first_name}
					lastName={character.last_name}
					cls={character.class}
					icon={false}
				/>
				<span class="muted small">
					<CharacterSpecs cls={character.class} specs={character.specs} size={16} />
					{[character.username, character.is_main && 'main'].filter(Boolean).join(' · ')}
				</span>
			</div>
		</li>
	{/each}
</ul>

<style>
	.characters {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(15rem, 1fr));
		gap: var(--space-2);
		margin: 0;
		padding: 0;
		list-style: none;
	}

	li {
		display: flex;
		align-items: center;
		gap: var(--space-3);
		padding: var(--space-2) var(--space-3);
		background-color: var(--grey-bg);
		border: 1px solid var(--grey-surface);
		border-radius: var(--radius-lg);
	}

	li > div {
		display: flex;
		flex-direction: column;
		min-width: 0;
	}

	img {
		display: block;
		border-radius: var(--radius-md);
	}

	.small {
		font-size: var(--text-sm);
	}
</style>
