<script lang="ts">
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import CharacterLink from '$lib/components/guild/CharacterLink.svelte';
	import {
		BAR_LABEL,
		SLOTS,
		emptyKeybinds,
		keyLabel,
		loadKeybinds,
		mouseLabel,
		rowKey,
		saveKeybinds,
		stancesOf,
		wheelLabel,
		type Bar,
		type Keybinds
	} from '$lib/wow/keybinds';
	import Button from '$lib/components/Button.svelte';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();

	const character = $derived(data.character);
	const characterId = $derived(character?.id ?? 0);
	const stances = $derived(stancesOf(character?.class ?? ''));

	let plan = $state<Keybinds>(emptyKeybinds());
	let loadedFor = $state(0);
	let newAbility = $state('');
	/** The button whose key is being set, waiting for a key press. */
	let binding = $state<{ bar: Bar; slot: number } | null>(null);

	// The plan lives in this browser, one per character: load on a switch, save on any edit.
	$effect(() => {
		plan = loadKeybinds(characterId);
		loadedFor = characterId;
		binding = null;
	});
	$effect(() => {
		const snapshot = $state.snapshot(plan);
		if (characterId && loadedFor === characterId) saveKeybinds(characterId, snapshot);
	});

	const row = (key: string) => (plan.rows[key] ??= Array(SLOTS).fill(null));
	const placed = $derived(new Set(Object.values(plan.rows).flat().filter(Boolean)));

	/** Keys bound to more than one button: in the game the last one bound wins. */
	const clashes = $derived.by(() => {
		const seen: Record<string, number> = {};
		for (const keys of Object.values(plan.keys))
			for (const key of keys) if (key) seen[key] = (seen[key] ?? 0) + 1;
		return new Set(Object.keys(seen).filter((key) => seen[key] > 1));
	});

	function addAbility(event: SubmitEvent) {
		event.preventDefault();
		const names = newAbility
			.split(',')
			.map((n) => n.trim())
			.filter((n) => n && !plan.pool.includes(n));
		plan.pool.push(...new Set(names));
		newAbility = '';
	}

	function removeAbility(name: string) {
		plan.pool = plan.pool.filter((n) => n !== name);
		for (const slots of Object.values(plan.rows))
			slots.forEach((n, i) => n === name && (slots[i] = null));
	}

	/* --- dragging: from the pool onto a button, between buttons, and off a bar to clear --- */

	type Payload = { ability: string } | { from: string; slot: number };
	let dragging = $state<Payload | null>(null);

	function dragStart(event: DragEvent, payload: Payload) {
		window.getSelection()?.removeAllRanges();
		dragging = payload;
		if (event.dataTransfer) {
			event.dataTransfer.effectAllowed = 'move';
			event.dataTransfer.setData('text/plain', 'ability' in payload ? payload.ability : '');
		}
	}

	function dropOnSlot(event: DragEvent, key: string, slot: number) {
		event.preventDefault();
		const payload = dragging;
		dragging = null;
		if (!payload) return;
		const target = row(key);
		if ('ability' in payload) {
			target[slot] = payload.ability;
		} else {
			// Swap, as the game does when one button is dropped on another.
			const source = row(payload.from);
			[source[payload.slot], target[slot]] = [target[slot], source[payload.slot]];
		}
	}

	function dropOnPool(event: DragEvent) {
		event.preventDefault();
		const payload = dragging;
		dragging = null;
		if (payload && 'from' in payload) row(payload.from)[payload.slot] = null;
	}

	/* --- binding: click a button's key, then press a key, a mouse button, or the wheel --- */

	function setKey(label: string | null) {
		if (!binding || !label) return;
		plan.keys[binding.bar][binding.slot] = label;
		binding = null;
	}

	function onKeydown(event: KeyboardEvent) {
		if (!binding) return;
		event.preventDefault();
		if (event.key === 'Escape') binding = null;
		else if (event.key === 'Backspace' || event.key === 'Delete') {
			plan.keys[binding.bar][binding.slot] = '';
			binding = null;
		} else setKey(keyLabel(event));
	}

	function onMousedown(event: MouseEvent) {
		if (!binding) return;
		const label = mouseLabel(event);
		if (label) {
			event.preventDefault();
			setKey(label);
		}
	}

	function onWheel(event: WheelEvent) {
		if (!binding) return;
		event.preventDefault();
		setKey(wheelLabel(event));
	}

	function reset() {
		if (confirm(`Clear every ability and key for ${character?.first_name}?`))
			plan = emptyKeybinds();
	}

	const pickCharacter = (id: number) =>
		// eslint-disable-next-line svelte/no-navigation-without-resolve -- same page, new query
		goto(`${resolve('/profile/keybinds')}?character=${id}`, {
			replaceState: true,
			noScroll: true
		});

	/** A short name for a button, as text fits on an icon: initials, or a long word cut. */
	function short(name: string) {
		if (name.length <= 8) return name;
		const words = name.split(/\s+/);
		return words.length > 1
			? words
					.map((w) => w[0])
					.join('')
					.toUpperCase()
			: name.slice(0, 5);
	}
</script>

<svelte:window onkeydown={onKeydown} onmousedown={onMousedown} />

<div class="page-head">
	<div>
		<p class="kicker"><a href={resolve('/profile')}>Profile</a> · Keybinds</p>
		<h1>
			{#if character}
				<CharacterLink
					id={character.id}
					firstName={character.first_name}
					lastName={character.last_name}
					cls={character.class}
				/>
			{:else}Keybinds{/if}
		</h1>
	</div>
	{#if data.characters.length > 1}
		<div class="characters" role="tablist" aria-label="Character">
			{#each data.characters as c (c.id)}
				<button
					type="button"
					role="tab"
					aria-selected={c.id === characterId}
					onclick={() => pickCharacter(c.id)}>{c.first_name}</button
				>
			{/each}
		</div>
	{/if}
</div>

{#if !character}
	<p class="muted">
		Add a character on <a href={resolve('/profile')}>your profile</a> to plan its keybinds.
	</p>
{:else}
	<p class="muted small">
		Add abilities to the pool, then drag them onto the bars; drag a button onto another to swap, or
		back to the pool to clear it. Click a button's key and press a key, mouse button, or scroll to
		bind it (Backspace unbinds). Kept in this browser, one plan per character.
	</p>

	{#snippet button(key: string, bar: Bar, slot: number)}
		{@const ability = plan.rows[key]?.[slot] ?? null}
		{@const bound = plan.keys[bar][slot]}
		{@const isBinding = binding?.bar === bar && binding.slot === slot}
		<div
			class="slot"
			class:filled={ability}
			role="listitem"
			title={ability ?? `Empty (${BAR_LABEL[bar]} ${slot + 1})`}
			draggable={ability ? 'true' : 'false'}
			ondragstart={(e) => ability && dragStart(e, { from: key, slot })}
			ondragend={() => (dragging = null)}
			ondragover={(e) => e.preventDefault()}
			ondrop={(e) => dropOnSlot(e, key, slot)}
		>
			{#if ability}<span class="name">{short(ability)}</span>{/if}
			<button
				type="button"
				class="key"
				class:binding={isBinding}
				class:clash={bound && clashes.has(bound)}
				aria-label="Key for {BAR_LABEL[bar]} button {slot + 1}{bound ? `: ${bound}` : ''}"
				onclick={(e) => {
					e.stopPropagation();
					binding = isBinding ? null : { bar, slot };
				}}
				onwheel={onWheel}>{isBinding ? '…' : bound}</button
			>
		</div>
	{/snippet}

	{#snippet bar(bar: Bar, label: string, key: string, vertical = false)}
		<div class="bar" class:vertical>
			<span class="bar-label">{label}</span>
			<div class="slots" role="list" aria-label={label}>
				{#each { length: SLOTS }, slot (slot)}
					{@render button(key, bar, slot)}
				{/each}
			</div>
		</div>
	{/snippet}

	<div class="ui">
		<section
			class="pool"
			aria-label="Abilities"
			ondragover={(e) => e.preventDefault()}
			ondrop={dropOnPool}
		>
			<form onsubmit={addAbility}>
				<input
					bind:value={newAbility}
					placeholder="Add abilities, comma separated (Heroic Strike, Charge, …)"
					aria-label="Ability names"
				/>
				<Button type="submit" size="small">Add</Button>
			</form>
			<ul>
				{#each plan.pool as name (name)}
					<li
						class:placed={placed.has(name)}
						draggable="true"
						ondragstart={(e) => dragStart(e, { ability: name })}
						ondragend={() => (dragging = null)}
					>
						{name}
						<button type="button" aria-label="Remove {name}" onclick={() => removeAbility(name)}
							>×</button
						>
					</li>
				{:else}
					<li class="empty muted">No abilities yet.</li>
				{/each}
			</ul>
			<div class="pool-foot">
				<Button variant="secondary" size="small" onclick={reset}>Clear all</Button>
			</div>
		</section>

		<div class="right">
			{@render bar(4, '4', '4', true)}
			{@render bar(5, '5', '5', true)}
		</div>

		<div class="bottom">
			<div class="pair">
				{@render bar(2, '2', '2')}
				{@render bar(3, '3', '3')}
			</div>
			{#each stances as stance (stance.slug)}
				{@render bar(1, stance.name ? `1 · ${stance.name}` : '1', rowKey(1, stance.slug))}
			{/each}
		</div>
	</div>
{/if}

<style>
	.small {
		font-size: var(--text-sm);
	}

	.characters {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-1);
	}

	.characters button {
		padding: var(--space-1) var(--space-3);
		font: inherit;
		font-size: var(--text-sm);
		background: var(--grey-bg);
		border: 1px solid var(--grey-surface);
		border-radius: var(--radius-md);
		cursor: pointer;
	}

	.characters button[aria-selected='true'] {
		border-color: var(--accent-solid);
		box-shadow: 0 0 0 1px var(--accent-solid);
	}

	/* The default UI: the pool where the screen is, bars 4 and 5 down the right edge, and bars
	   2 and 3 above bar 1 (one row per stance) along the bottom. */
	.ui {
		/* Small enough that bars 2 and 3 fit side by side: 24 buttons plus the labels. */
		--slot: clamp(1.75rem, calc((100cqw - 14rem) / 24), 2.75rem);
		container-type: inline-size;
		display: grid;
		grid-template-columns: minmax(0, 1fr) auto;
		grid-template-areas: 'pool right' 'bottom bottom';
		gap: var(--space-4);
		margin-top: var(--space-4);
	}

	.pool {
		grid-area: pool;
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
		min-height: 16rem;
		padding: var(--space-4);
		background: var(--grey-bg);
		border: 1px dashed var(--grey-surface);
		border-radius: var(--radius-xl);
	}

	.pool form {
		display: flex;
		gap: var(--space-2);
	}

	.pool input {
		flex: 1;
		min-width: 0;
	}

	.pool ul {
		display: flex;
		flex-wrap: wrap;
		align-content: flex-start;
		gap: var(--space-2);
		flex: 1;
		margin: 0;
		padding: 0;
		list-style: none;
	}

	.pool li:not(.empty) {
		display: flex;
		align-items: center;
		gap: var(--space-1);
		padding: var(--space-1) var(--space-1) var(--space-1) var(--space-3);
		font-size: var(--text-sm);
		background: var(--grey-surface);
		border-radius: var(--radius-md);
		cursor: grab;
	}

	.pool li.placed {
		opacity: 0.55;
	}

	.pool li button {
		padding: 0 var(--space-1);
		font: inherit;
		color: var(--grey-text);
		background: none;
		border: 0;
		cursor: pointer;
	}

	.pool-foot {
		display: flex;
		justify-content: flex-end;
	}

	.right {
		grid-area: right;
		display: flex;
		gap: var(--space-1);
	}

	.bottom {
		grid-area: bottom;
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}

	.pair {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-4);
	}

	.bar {
		display: flex;
		align-items: center;
		gap: var(--space-2);
	}

	.bar.vertical {
		flex-direction: column;
	}

	.bar-label {
		min-width: 1.5rem;
		font-size: var(--text-xs);
		font-weight: 600;
		color: var(--grey-text);
		white-space: nowrap;
	}

	.bar:not(.vertical) .bar-label {
		width: 5.5rem;
		overflow: hidden;
		text-overflow: ellipsis;
		text-align: right;
	}

	.pair .bar + .bar .bar-label {
		width: 1rem;
	}

	.slots {
		display: flex;
		flex-wrap: wrap;
		gap: 3px;
	}

	.vertical .slots {
		flex-direction: column;
	}

	.slot {
		position: relative;
		display: grid;
		place-items: center;
		width: var(--slot);
		height: var(--slot);
		padding: 2px;
		background: var(--black);
		border: 1px solid var(--grey-surface);
		border-radius: var(--radius-sm);
	}

	.slot.filled {
		background: linear-gradient(160deg, var(--grey-solid), var(--black));
		border-color: var(--grey-text);
		cursor: grab;
	}

	.name {
		font-size: 0.6rem;
		line-height: 1.1;
		text-align: center;
		color: var(--white);
		overflow-wrap: normal;
		overflow: hidden;
		pointer-events: none;
	}

	.key {
		position: absolute;
		top: 1px;
		right: 2px;
		min-width: 1rem;
		min-height: 0.9rem;
		padding: 0;
		font: inherit;
		font-size: 0.6rem;
		font-weight: 700;
		color: var(--grey-text-active);
		text-shadow: 0 0 2px var(--black);
		background: none;
		border: 0;
		cursor: pointer;
	}

	.key:hover {
		color: var(--white);
	}

	.key.clash {
		color: var(--red-text);
	}

	.key.binding {
		inset: 0;
		color: var(--accent-text);
		background: color-mix(in srgb, var(--accent-solid) 30%, transparent);
		border-radius: var(--radius-sm);
	}

	@media (max-width: 48rem) {
		.ui {
			--slot: clamp(1.75rem, calc((100cqw - 6rem) / 12), 2.25rem);
			grid-template-columns: minmax(0, 1fr);
			grid-template-areas: 'pool' 'bottom' 'right';
		}

		.right,
		.bar.vertical,
		.vertical .slots {
			flex-direction: row;
		}

		.right {
			flex-direction: column;
		}

		.bar:not(.vertical) .bar-label,
		.bar-label,
		.pair .bar + .bar .bar-label {
			width: 4rem;
			text-align: left;
		}
	}
</style>
