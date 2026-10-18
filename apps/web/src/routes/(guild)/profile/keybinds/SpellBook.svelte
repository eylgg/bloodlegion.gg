<script lang="ts">
	import { spellIcon, type Spell } from '$lib/wow/spells';
	import Button from '$lib/components/Button.svelte';

	/**
	 * The class's spells to drag onto the bars, as the spellbook groups them: the general attacks,
	 * then each talent tree. Macros and items are added by name below them.
	 */
	let {
		spells,
		specs,
		custom,
		placed,
		ondrag,
		onadd,
		onremove
	}: {
		spells: Spell[];
		specs: { slug: string; name: string }[];
		custom: string[];
		placed: Set<string>;
		ondrag: (event: DragEvent, name: string) => void;
		onadd: (name: string) => void;
		onremove: (name: string) => void;
	} = $props();

	let filter = $state('');
	let newName = $state('');

	const groups = $derived.by(() => {
		const needle = filter.trim().toLowerCase();
		const shown = spells.filter((s) => s.name.toLowerCase().includes(needle));
		return [
			{ name: 'General', spells: shown.filter((s) => !s.tree) },
			...specs.map((spec) => ({
				name: spec.name,
				spells: shown.filter((s) => s.tree === spec.slug)
			}))
		].filter((g) => g.spells.length > 0);
	});

	function add(event: SubmitEvent) {
		event.preventDefault();
		const name = newName.trim();
		if (name) onadd(name);
		newName = '';
	}
</script>

<div class="book">
	<input type="search" bind:value={filter} placeholder="Find a spell" aria-label="Find a spell" />
	{#each groups as group (group.name)}
		<section>
			<h3>{group.name}</h3>
			<ul>
				{#each group.spells as spell (spell.name)}
					<li
						class:placed={placed.has(spell.name)}
						draggable="true"
						ondragstart={(e) => ondrag(e, spell.name)}
						title="{spell.name}{spell.talent
							? ' (talent)'
							: ` (level ${spell.level})`}{spell.ranks > 1 ? `, ${spell.ranks} ranks` : ''}"
					>
						<img src={spellIcon(spell.icon)} alt="" width="28" height="28" draggable="false" />
						<span>{spell.name}</span>
						{#if spell.talent}<span class="talent">T</span>{/if}
					</li>
				{/each}
			</ul>
		</section>
	{/each}

	<section>
		<h3>Macros and items</h3>
		<form onsubmit={add}>
			<input bind:value={newName} placeholder="A macro or item, by name" aria-label="Name" />
			<Button type="submit" size="small" variant="secondary">Add</Button>
		</form>
		<ul>
			{#each custom as name (name)}
				<li
					class="custom"
					class:placed={placed.has(name)}
					draggable="true"
					ondragstart={(e) => ondrag(e, name)}
				>
					<span class="blank">?</span>
					<span>{name}</span>
					<button type="button" aria-label="Remove {name}" onclick={() => onremove(name)}>×</button>
				</li>
			{/each}
		</ul>
	</section>
</div>

<style>
	.book {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
		min-height: 0;
	}

	h3 {
		margin: 0 0 var(--space-1);
		font-size: var(--text-xs);
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.04em;
		color: var(--grey-text);
	}

	ul {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(10.5rem, 1fr));
		gap: 2px var(--space-2);
		margin: 0;
		padding: 0;
		list-style: none;
	}

	li {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		min-width: 0;
		padding: 2px;
		font-size: var(--text-sm);
		border-radius: var(--radius-sm);
		cursor: grab;
	}

	li:hover {
		background: var(--grey-surface);
	}

	li span:not(.talent, .blank) {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	li.placed {
		opacity: 0.45;
	}

	img,
	.blank {
		flex: none;
		border: 1px solid var(--black);
		border-radius: var(--radius-sm);
	}

	.blank {
		display: grid;
		place-items: center;
		width: 28px;
		height: 28px;
		color: var(--grey-text);
		background: var(--black);
	}

	.talent {
		margin-left: auto;
		padding: 0 var(--space-1);
		font-size: var(--text-xs);
		color: var(--accent-text);
	}

	form {
		display: flex;
		gap: var(--space-2);
		margin-bottom: var(--space-2);
	}

	form input {
		flex: 1;
		min-width: 0;
	}

	li button {
		margin-left: auto;
		padding: 0 var(--space-1);
		font: inherit;
		color: var(--grey-text);
		background: none;
		border: 0;
		cursor: pointer;
	}
</style>
