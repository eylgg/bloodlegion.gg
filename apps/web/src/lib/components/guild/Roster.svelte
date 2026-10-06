<script module lang="ts">
	/** Where a character is placed, for its badges. */
	export type Place = { raidId: number; name: string; group: number };
</script>

<script lang="ts">
	import { page } from '$app/state';
	import CharacterSpecs from './CharacterSpecs.svelte';
	import type { GuildCharacter, WowClass } from '$lib/types';

	/**
	 * The characters to fill raids from: each member's, by username, then those no member plays.
	 * Officers drag a character onto a raid, or click one (`selected`) then a free slot. Dropping
	 * someone from a raid back here takes them off it (`onreturn`).
	 */
	let {
		characters,
		places,
		editable,
		selected = $bindable(null),
		onreturn
	}: {
		characters: GuildCharacter[];
		places: (characterId: number) => Place[];
		editable: boolean;
		selected?: number | null;
		onreturn?: (raidId: number, characterId: number) => void;
	} = $props();

	const classes = $derived((page.data.classes as WowClass[] | undefined) ?? []);
	const color = (cls: string) => classes.find((c) => c.slug === cls)?.color;
	const fullName = (c: GuildCharacter) => `${c.first_name} ${c.last_name}`;
	const byName = (a: string, b: string) => a.localeCompare(b, undefined, { sensitivity: 'base' });

	let query = $state('');
	let unplacedOnly = $state(false);

	const matches = (c: GuildCharacter) => {
		const text = query.trim().toLowerCase();
		return (
			(!text ||
				fullName(c).toLowerCase().includes(text) ||
				(c.username ?? '').toLowerCase().includes(text)) &&
			(!unplacedOnly || places(c.id).length === 0)
		);
	};

	const shown = $derived(characters.filter(matches));

	// Members first, by username, each with their characters (main first); then the characters no
	// member plays, by name.
	const players = $derived.by(() => {
		const byUser: Record<string, GuildCharacter[]> = {};
		for (const c of shown) if (c.username !== null) (byUser[c.username] ??= []).push(c);
		return Object.entries(byUser)
			.sort(([a], [b]) => byName(a, b))
			.map(([username, list]) => ({
				username,
				characters: list.sort(
					(a, b) => Number(b.is_main) - Number(a.is_main) || byName(fullName(a), fullName(b))
				)
			}));
	});
	const others = $derived(
		shown.filter((c) => c.username === null).sort((a, b) => byName(fullName(a), fullName(b)))
	);

	function pick(character: GuildCharacter) {
		if (!editable) return;
		selected = selected === character.id ? null : character.id;
	}

	function ondrop(event: DragEvent) {
		event.preventDefault();
		const raidId = Number(event.dataTransfer?.getData('application/x-raid'));
		const characterId = Number(event.dataTransfer?.getData('text/plain'));
		if (raidId > 0 && characterId > 0) onreturn?.(raidId, characterId);
	}
</script>

{#snippet chip(character: GuildCharacter)}
	{@const here = places(character.id)}
	<li>
		<button
			type="button"
			class="char"
			class:selected={selected === character.id}
			class:placed={here.length > 0}
			style:--class-color={color(character.class) ?? 'var(--foreground)'}
			draggable={editable}
			disabled={!editable}
			ondragstart={(e) => e.dataTransfer?.setData('text/plain', String(character.id))}
			onclick={() => pick(character)}
		>
			<span class="name">{fullName(character)}</span>
			<CharacterSpecs cls={character.class} specs={character.specs} size={16} />
		</button>
		{#each here as place (place.raidId)}
			<span class="badge" title="In {place.name}, group {place.group}">
				{place.name} · {place.group}
			</span>
		{/each}
	</li>
{/snippet}

<aside
	class="roster"
	aria-label="Roster"
	ondragover={(e) => editable && e.preventDefault()}
	{ondrop}
>
	<h2>Roster</h2>
	<input type="search" bind:value={query} placeholder="Player or character" aria-label="Find" />
	<label class="toggle">
		<input type="checkbox" bind:checked={unplacedOnly} />
		Only characters not in a raid here
	</label>
	{#if editable}
		<p class="muted small">
			Drag characters onto a group, or click one then a free slot. Drag someone back here to take
			them off the raid.
		</p>
	{/if}
	<ul class="players">
		{#each players as player (player.username)}
			<li>
				<span class="username">{player.username}</span>
				<ul class="chars">
					{#each player.characters as character (character.id)}
						{@render chip(character)}
					{/each}
				</ul>
			</li>
		{/each}
		{#if others.length > 0}
			<li class="others">
				<ul class="chars">
					{#each others as character (character.id)}
						{@render chip(character)}
					{/each}
				</ul>
			</li>
		{/if}
		{#if players.length === 0 && others.length === 0}
			<li class="muted small">Nobody matches.</li>
		{/if}
	</ul>
</aside>

<style>
	.roster {
		position: sticky;
		top: var(--space-4);
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		max-height: calc(100vh - var(--space-8));
		padding: var(--space-3);
		overflow-y: auto;
		background-color: var(--grey-bg);
		border: 1px solid var(--grey-surface);
		border-radius: var(--radius-lg);
	}

	@media (max-width: 52rem) {
		.roster {
			position: static;
			max-height: 24rem;
		}
	}

	h2 {
		font-size: var(--text-lg);
	}

	.small {
		font-size: var(--text-sm);
	}

	.toggle {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		font-size: var(--text-sm);
		cursor: pointer;
	}

	.players,
	.chars {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		margin: 0;
		padding: 0;
		list-style: none;
	}

	.players > li {
		padding: var(--space-1) 0;
		border-top: 1px solid var(--grey-surface);
	}

	.players > li.others {
		margin-top: var(--space-2);
		border-top-width: 2px;
	}

	.username {
		color: var(--grey-text);
		font-family: var(--font-mono);
		font-size: var(--text-xs);
	}

	.chars li {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-1);
	}

	.char {
		display: flex;
		flex: 1;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-2);
		min-width: 0;
		padding: 2px var(--space-2);
		font: inherit;
		text-align: left;
		background-color: var(--background);
		border: 1px solid var(--grey-surface);
		border-left: 3px solid var(--class-color);
		border-radius: var(--radius-md);
	}

	.char:not(:disabled) {
		cursor: grab;
	}

	.char.placed {
		opacity: 0.55;
	}

	.char.selected {
		opacity: 1;
		border-color: var(--accent-solid);
		box-shadow: 0 0 0 2px var(--accent-bg);
	}

	.name {
		overflow: hidden;
		color: var(--class-color);
		font-size: var(--text-sm);
		font-weight: 600;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.badge {
		padding: 0 4px;
		color: var(--grey-text);
		font-size: var(--text-xs);
		white-space: nowrap;
		border: 1px solid var(--grey-surface);
		border-radius: var(--radius-sm);
	}
</style>
