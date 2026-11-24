<script module lang="ts">
	/** Where a character is placed, for its badges. */
	export type Place = { raidId: number; name: string; group: number };
</script>

<script lang="ts">
	import { page } from '$app/state';
	import CharacterSpecs from './CharacterSpecs.svelte';
	import { drag, dragCharacter, hover } from '$lib/wow/drag.svelte';
	import type { GuildCharacter, WowClass } from '$lib/types';

	/**
	 * The characters to fill raids from: the guild's, at 60 unless asked for the rest, one line
	 * each with its player beside it, by player (their main first), then those nobody plays.
	 * Officers drag a character onto a raid, or click one (`selected`) then a free slot. Dropping
	 * someone from a raid back here takes them off it (`onreturn`).
	 */
	let {
		characters,
		places,
		editable,
		selected = $bindable(null),
		onreturn,
		hint = 'Drag characters onto a group, or click one then a free slot. Drag someone back here to take them off the raid.'
	}: {
		characters: GuildCharacter[];
		places: (characterId: number) => Place[];
		editable: boolean;
		selected?: number | null;
		onreturn?: (raidId: number, characterId: number) => void;
		/** How to use it, for officers. */
		hint?: string;
	} = $props();

	const classes = $derived((page.data.classes as WowClass[] | undefined) ?? []);
	const color = (cls: string) => classes.find((c) => c.slug === cls)?.color;
	const fullName = (c: GuildCharacter) => `${c.first_name} ${c.last_name}`;
	const byName = (a: string, b: string) => a.localeCompare(b, undefined, { sensitivity: 'base' });

	let query = $state('');
	let unplacedOnly = $state(false);
	let below60 = $state(false);

	const matches = (c: GuildCharacter) => {
		const text = query.trim().toLowerCase();
		return (
			c.in_guild &&
			(below60 || c.level === 60) &&
			(!text ||
				fullName(c).toLowerCase().includes(text) ||
				(c.username ?? '').toLowerCase().includes(text)) &&
			(!unplacedOnly || places(c.id).length === 0)
		);
	};

	// By player, each one's main first; then the characters nobody plays, by name.
	const shown = $derived(
		characters
			.filter(matches)
			.sort(
				(a, b) =>
					Number(a.username === null) - Number(b.username === null) ||
					byName(a.username ?? '', b.username ?? '') ||
					Number(b.is_main) - Number(a.is_main) ||
					byName(fullName(a), fullName(b))
			)
	);

	function pick(character: GuildCharacter) {
		if (!editable) return;
		selected = selected === character.id ? null : character.id;
	}

	/** Someone dragged off a raid, over the roster: dropping takes them off. */
	const returning = $derived(
		drag.over?.kind === 'roster' && drag.current !== null && drag.current.raidId !== null
	);

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
			class:origin={drag.current?.characterId === character.id && drag.current.raidId === null}
			style:--class-color={color(character.class) ?? 'var(--foreground)'}
			draggable={editable}
			disabled={!editable}
			ondragstart={(e) => dragCharacter(e, { characterId: character.id, ...character })}
			onclick={() => pick(character)}
		>
			<span class="name">{fullName(character)}</span>
			{#if below60 && character.level < 60}<span class="level">{character.level}</span>{/if}
			<span class="player">{character.username ?? '—'}</span>
			<CharacterSpecs cls={character.class} specs={character.specs} size={16} />
		</button>
		{#each here as place (place.raidId)}
			<span class="badge" title="In {place.name}, group {place.group}">
				{place.name}
			</span>
		{/each}
	</li>
{/snippet}

<aside
	class="roster"
	aria-label="Roster"
	class:returning
	ondragover={(e) => {
		if (!editable) return;
		e.preventDefault();
		if (onreturn) hover({ kind: 'roster' });
	}}
	{ondrop}
>
	<h2>Roster</h2>
	<input type="search" bind:value={query} placeholder="Player or character" aria-label="Find" />
	<label class="toggle">
		<input type="checkbox" bind:checked={unplacedOnly} />
		Only characters not in a raid here
	</label>
	<label class="toggle">
		<input type="checkbox" bind:checked={below60} />
		Below level 60 too
	</label>
	{#if returning && drag.current}
		<p class="small drop-hint">
			Drop to take {drag.current.first_name}
			{drag.current.last_name} off the raid
		</p>
	{:else if editable}
		<p class="muted small">{hint}</p>
	{/if}
	<ul class="chars">
		{#each shown as character (character.id)}
			{@render chip(character)}
		{:else}
			<li class="muted small">Nobody matches.</li>
		{/each}
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

	.roster.returning {
		border-color: var(--red-text);
		border-style: dashed;
	}

	.drop-hint {
		color: var(--red-text);
		font-weight: 600;
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

	/* Names are picked up whole, never text-selected (a selection would be dragged along). */
	.chars {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		margin: 0;
		padding: 0;
		list-style: none;
		user-select: none;
		-webkit-user-select: none;
	}

	/* The player, beside the name: taking the room between it and the specs. */
	.player {
		flex: 1;
		overflow: hidden;
		color: var(--grey-text);
		font-family: var(--font-mono);
		font-size: var(--text-xs);
		text-align: right;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.level {
		color: var(--grey-text);
		font-size: var(--text-xs);
		font-variant-numeric: tabular-nums;
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

	/* Picked up and on the move. */
	.char.origin {
		border-style: dashed;
		opacity: 0.4;
	}

	.char.selected {
		opacity: 1;
		border-color: var(--accent-solid);
		box-shadow: 0 0 0 2px var(--accent-bg);
	}

	.name {
		flex-shrink: 0;
		max-width: 60%;
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
