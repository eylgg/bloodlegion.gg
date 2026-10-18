<script lang="ts">
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import CharacterLink from '$lib/components/guild/CharacterLink.svelte';
	import Alert from '$lib/components/Alert.svelte';
	import Button from '$lib/components/Button.svelte';
	import {
		buttonCommand,
		commandLabel,
		defaultBindings,
		keyFromEvent,
		keyFromMouse,
		keyFromWheel,
		shortKey
	} from '$lib/wow/bindings';
	import {
		BAR_LABEL,
		SLOTS,
		emptyKeybinds,
		exportString,
		loadKeybinds,
		rowKey,
		saveKeybinds,
		stancesOf,
		type Bar,
		type Keybinds
	} from '$lib/wow/keybinds';
	import { spellIcon, spellsOf } from '$lib/wow/spells';
	import KeyBindings from './KeyBindings.svelte';
	import SpellBook from './SpellBook.svelte';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();

	const character = $derived(data.character);
	const characterId = $derived(character?.id ?? 0);
	const classSlug = $derived(character?.class ?? '');
	const stances = $derived(stancesOf(classSlug));
	const spells = $derived(spellsOf(classSlug));
	const spellByName = $derived(new Map(spells.map((s) => [s.name, s])));
	const specs = $derived(data.classes.find((c) => c.slug === classSlug)?.specs ?? []);

	let plan = $state<Keybinds>(emptyKeybinds());
	let loadedFor = $state(0);
	/** The key being set, waiting for a key press: a command's first or second key. */
	let capturing = $state<{ command: string; index: number } | null>(null);
	let notice = $state('');
	let exported = $state('');

	// The plan lives in this browser, one per character: load on a switch, save on any edit.
	$effect(() => {
		plan = loadKeybinds(characterId, classSlug);
		loadedFor = characterId;
		capturing = null;
		exported = '';
	});
	$effect(() => {
		const snapshot = $state.snapshot(plan);
		if (characterId && loadedFor === characterId) saveKeybinds(characterId, snapshot);
	});

	const row = (key: string) => (plan.rows[key] ??= Array(SLOTS).fill(null));
	const placed = $derived(
		new Set(
			Object.values(plan.rows)
				.flat()
				.filter((n) => n !== null)
		)
	);

	function addCustom(name: string) {
		if (!plan.custom.includes(name) && !spellByName.has(name)) plan.custom.push(name);
	}

	function removeCustom(name: string) {
		plan.custom = plan.custom.filter((n) => n !== name);
		for (const slots of Object.values(plan.rows))
			slots.forEach((n, i) => n === name && (slots[i] = null));
	}

	/* --- dragging: from the spellbook onto a button, between buttons, and off a bar to clear --- */

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

	function dropOnBook(event: DragEvent) {
		event.preventDefault();
		const payload = dragging;
		dragging = null;
		if (payload && 'from' in payload) row(payload.from)[payload.slot] = null;
	}

	/* --- binding: pick a key, then press a key, a mouse button, or scroll --- */

	function capture(command: string, index: number) {
		const same = capturing?.command === command && capturing.index === index;
		capturing = same ? null : { command, index };
		notice = '';
	}

	/** Binds the key being set; as in the game, a key moves off whatever had it before. */
	function bind(key: string) {
		if (!capturing) return;
		const { command, index } = capturing;
		capturing = null;
		for (const [other, keys] of Object.entries(plan.bindings))
			keys.forEach((k, i) => {
				if (k !== key || (other === command && i === index)) return;
				keys[i] = '';
				notice = `${key} was unbound from ${commandLabel(other)}.`;
			});
		(plan.bindings[command] ??= ['', ''])[index] = key;
	}

	function onKeydown(event: KeyboardEvent) {
		if (!capturing) return;
		event.preventDefault();
		if (event.key === 'Escape') capturing = null;
		else if (event.key === 'Backspace' || event.key === 'Delete') {
			const { command, index } = capturing;
			(plan.bindings[command] ??= ['', ''])[index] = '';
			capturing = null;
		} else {
			const key = keyFromEvent(event);
			if (key) bind(key);
		}
	}

	function onMousedown(event: MouseEvent) {
		if (!capturing) return;
		const key = keyFromMouse(event);
		if (key) {
			event.preventDefault();
			bind(key);
		}
	}

	function onWheel(event: WheelEvent) {
		if (!capturing) return;
		event.preventDefault();
		bind(keyFromWheel(event));
	}

	function clearBars() {
		if (confirm(`Clear every button on ${character?.first_name}'s bars?`)) plan.rows = {};
	}

	function resetBindings() {
		if (confirm('Put every key binding back to the default?')) plan.bindings = defaultBindings();
	}

	function makeExport() {
		if (!character) return;
		const name = `${character.first_name} ${character.last_name}`;
		exported = exportString($state.snapshot(plan), classSlug, name, stances);
	}

	const pickCharacter = (id: number) =>
		// eslint-disable-next-line svelte/no-navigation-without-resolve -- same page, new query
		goto(`${resolve('/profile/keybinds')}?character=${id}`, {
			replaceState: true,
			noScroll: true
		});
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
		Drag spells onto the bars; drag a button onto another to swap, or back to the spellbook to clear
		it. Click a button's key, or any key in the bindings below, then press a key, mouse button, or
		scroll to bind it (Backspace unbinds, Escape cancels). Kept in this browser, one plan per
		character; export it to load it in game with the Blood Legion addon.
	</p>

	{#if notice}<Alert variant="info">{notice}</Alert>{/if}

	{#snippet button(key: string, bar: Bar, slot: number)}
		{@const ability = plan.rows[key]?.[slot] ?? null}
		{@const spell = ability ? spellByName.get(ability) : undefined}
		{@const command = buttonCommand(bar, slot)}
		{@const bound = plan.bindings[command]?.[0] ?? ''}
		{@const active = capturing?.command === command && capturing.index === 0}
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
			{#if spell}
				<img src={spellIcon(spell.icon)} alt={ability} draggable="false" />
			{:else if ability}
				<span class="name">{ability}</span>
			{/if}
			<button
				type="button"
				class="key"
				class:binding={active}
				aria-label="Key for {BAR_LABEL[bar]} button {slot + 1}{bound ? `: ${bound}` : ''}"
				onclick={(e) => {
					e.stopPropagation();
					capture(command, 0);
				}}
				onwheel={onWheel}>{active ? '…' : shortKey(bound)}</button
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
			aria-label="Spellbook"
			ondragover={(e) => e.preventDefault()}
			ondrop={dropOnBook}
		>
			<SpellBook
				{spells}
				{specs}
				custom={plan.custom}
				{placed}
				ondrag={(e, name) => dragStart(e, { ability: name })}
				onadd={addCustom}
				onremove={removeCustom}
			/>
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

	<div class="tools">
		<Button size="small" onclick={makeExport}>Export for the addon</Button>
		<Button size="small" variant="secondary" onclick={clearBars}>Clear bars</Button>
		<Button size="small" variant="secondary" onclick={resetBindings}>Default bindings</Button>
	</div>

	{#if exported}
		<div class="export">
			<p class="small">
				In game, type <code>/bl import</code>, paste this, and press Import. It places each spell at
				the highest rank {character.first_name} knows and saves the bindings for that character.
			</p>
			<textarea readonly rows="4" onfocus={(e) => e.currentTarget.select()}>{exported}</textarea>
			<div>
				<Button
					size="small"
					variant="secondary"
					onclick={() => navigator.clipboard.writeText(exported)}>Copy</Button
				>
			</div>
		</div>
	{/if}

	<h2>Key bindings</h2>
	<KeyBindings bindings={plan.bindings} {capturing} oncapture={capture} />
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
		/* As tall as bars 4 and 5 beside it, scrolling past that. */
		height: calc(12 * var(--slot) + 11 * 3px + 1.5rem);
		min-height: 16rem;
		overflow-y: auto;
		padding: var(--space-4);
		background: var(--grey-bg);
		border: 1px dashed var(--grey-surface);
		border-radius: var(--radius-xl);
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
		padding: 1px;
		background: var(--black);
		border: 1px solid var(--grey-surface);
		border-radius: var(--radius-sm);
	}

	.slot.filled {
		background: linear-gradient(160deg, var(--grey-solid), var(--black));
		border-color: var(--grey-text);
		cursor: grab;
	}

	.slot img {
		width: 100%;
		height: 100%;
		border-radius: 2px;
		pointer-events: none;
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
		text-shadow:
			0 0 2px var(--black),
			0 0 2px var(--black);
		background: none;
		border: 0;
		cursor: pointer;
	}

	.key:hover {
		color: var(--white);
	}

	.key.binding {
		inset: 0;
		color: var(--accent-text);
		background: color-mix(in srgb, var(--accent-solid) 30%, transparent);
		border-radius: var(--radius-sm);
	}

	.tools {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-2);
		margin-top: var(--space-4);
	}

	.export {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		margin-top: var(--space-3);
	}

	.export p {
		margin: 0;
	}

	.export textarea {
		width: 100%;
		font-family: var(--font-mono);
		font-size: var(--text-xs);
		word-break: break-all;
	}

	h2 {
		margin-top: var(--space-8);
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
