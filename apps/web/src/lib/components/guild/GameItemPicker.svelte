<script lang="ts">
	import { api } from '$lib/api';
	import { QUALITY_COLOR } from '$lib/guild';
	import type { GameItemSummary, Quality } from '$lib/types';
	import { itemIcon } from '$lib/wow/items';

	/**
	 * An item name field that searches the item mirror as it is typed. Picking a suggestion sets
	 * `selected`; typing anything else clears it, leaving a plain name (an item the mirror does not
	 * know).
	 */
	let {
		name = $bindable(''),
		selected = $bindable(),
		id
	}: { name?: string; selected?: GameItemSummary | null; id?: string } = $props();

	const listbox = $props.id();

	let suggestions = $state<GameItemSummary[]>([]);
	let open = $state(false);
	let active = $state(-1);
	let timer: ReturnType<typeof setTimeout> | undefined;
	// Only the latest search's answer counts.
	let latest = 0;

	function input() {
		selected = null;
		clearTimeout(timer);
		const query = name.trim();
		if (query.length < 2) {
			suggestions = [];
			open = false;
			return;
		}
		timer = setTimeout(async () => {
			const ticket = ++latest;
			try {
				const found = await api.get<GameItemSummary[]>(
					`/api/game-items?q=${encodeURIComponent(query)}&limit=12`
				);
				if (ticket !== latest) return;
				suggestions = found;
				active = -1;
				open = found.length > 0;
			} catch {
				suggestions = [];
				open = false;
			}
		}, 150);
	}

	function pick(item: GameItemSummary) {
		selected = item;
		name = item.name;
		open = false;
	}

	function keydown(event: KeyboardEvent) {
		if (!open) return;
		if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
			event.preventDefault();
			const step = event.key === 'ArrowDown' ? 1 : -1;
			active = (active + step + suggestions.length) % suggestions.length;
		} else if (event.key === 'Enter' && active >= 0) {
			event.preventDefault();
			pick(suggestions[active]);
		} else if (event.key === 'Escape') {
			open = false;
		}
	}
</script>

<div class="picker" class:picked={selected}>
	<input
		{id}
		type="text"
		role="combobox"
		aria-expanded={open}
		aria-controls={listbox}
		aria-autocomplete="list"
		autocomplete="off"
		maxlength="128"
		required
		bind:value={name}
		oninput={input}
		onkeydown={keydown}
		onblur={() => setTimeout(() => (open = false), 150)}
	/>
	{#if selected?.icon}
		<img class="chosen" src={itemIcon(selected.icon)} alt="" width="22" height="22" />
	{/if}
	{#if open}
		<ul role="listbox" id={listbox}>
			{#each suggestions as item, i (item.id)}
				<li role="option" aria-selected={i === active}>
					<button type="button" class:active={i === active} onmousedown={() => pick(item)}>
						{#if item.icon}<img src={itemIcon(item.icon)} alt="" width="24" height="24" />{/if}
						<span class="name" style:color={QUALITY_COLOR[item.quality as Quality] ?? '#fff'}
							>{item.name}</span
						>
						<span class="meta">{item.slot} · {item.item_subclass} · {item.item_level}</span>
					</button>
				</li>
			{/each}
		</ul>
	{/if}
</div>

<style>
	.picker {
		position: relative;
	}

	/* A picked item shows its icon inside the field. */
	.picker.picked input {
		padding-left: calc(var(--space-3) + 1.75rem);
	}

	.chosen {
		position: absolute;
		top: 50%;
		left: var(--space-2);
		transform: translateY(-50%);
		pointer-events: none;
	}

	ul {
		position: absolute;
		top: calc(100% + 2px);
		right: 0;
		left: 0;
		z-index: 20;
		max-height: 20rem;
		margin: 0;
		padding: var(--space-1);
		overflow-y: auto;
		list-style: none;
		background-color: var(--background);
		border: 1px solid var(--grey-soft);
		border-radius: var(--radius-md);
		box-shadow: 0 8px 24px rgb(0 0 0 / 0.5);
	}

	button {
		display: grid;
		grid-template-columns: auto 1fr;
		grid-template-rows: auto auto;
		column-gap: var(--space-2);
		align-items: center;
		width: 100%;
		padding: var(--space-1) var(--space-2);
		font: inherit;
		text-align: left;
		background: none;
		border: 0;
		border-radius: var(--radius-sm);
		cursor: pointer;
	}

	button:hover,
	button.active {
		background-color: var(--grey-surface);
	}

	img {
		grid-row: 1 / span 2;
		border-radius: 3px;
	}

	.name {
		font-size: var(--text-md);
		font-weight: 600;
	}

	.meta {
		color: var(--grey-text);
		font-size: var(--text-xs);
	}
</style>
