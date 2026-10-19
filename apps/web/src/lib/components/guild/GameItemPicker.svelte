<script lang="ts">
	import { api } from '$lib/api';
	import { QUALITY_COLOR } from '$lib/guild';
	import type { BossDrop, GameItemPage, GameItemSummary, Quality } from '$lib/types';
	import { itemIcon } from '$lib/wow/items';

	/**
	 * An item name field that searches the item mirror as it is typed. Picking a suggestion sets
	 * `selected`; typing anything else clears it, leaving a plain name (an item the mirror does not
	 * know).
	 *
	 * With a `boss`, it suggests only what that boss is known to drop (recorded from it, or planned
	 * under it), from the first focus, and picking one names the guild's item; a name it does not
	 * know is a new drop.
	 */
	let {
		name = $bindable(''),
		selected = $bindable(),
		id,
		label,
		placeholder,
		boss = null
	}: {
		name?: string;
		selected?: GameItemSummary | null;
		id?: string;
		/** The field's name for assistive tech, when no `<label>` names it. */
		label?: string;
		placeholder?: string;
		/** The boss the item comes from; null searches every item. */
		boss?: { id: number; name: string } | null;
	} = $props();

	const listbox = $props.id();

	let suggestions = $state<GameItemSummary[]>([]);
	let open = $state(false);
	let active = $state(-1);
	let timer: ReturnType<typeof setTimeout> | undefined;
	// Only the latest search's answer counts.
	let latest = 0;

	/* --- a boss's drops --- */

	let drops = $state<BossDrop[]>([]);
	// The icon of the boss's item last picked, until the name changes.
	let pickedIcon = $state<string | null>(null);

	// The id alone, so a new object for the same boss does not fetch again.
	const bossId = $derived(boss?.id);

	$effect(() => {
		const id = bossId;
		drops = [];
		if (id === undefined) return;
		let current = true;
		api
			.get<BossDrop[]>(`/api/bosses/${id}/items`)
			.then((found) => current && (drops = found))
			.catch(() => {});
		return () => (current = false);
	});

	const query = $derived(name.trim().toLowerCase());
	const matches = $derived(
		drops.filter((d) => d.item_name.toLowerCase().includes(query)).slice(0, 12)
	);
	// A typed name that matches none of the boss's items: it will be a new one.
	const newDrop = $derived(query.length > 0 && matches.length === 0);
	const count = $derived(boss ? matches.length : suggestions.length);

	function pickDrop(drop: BossDrop) {
		selected = null;
		name = drop.item_name;
		pickedIcon = drop.item_icon;
		open = false;
	}

	function input() {
		selected = null;
		pickedIcon = null;
		if (boss) {
			active = -1;
			open = true;
			return;
		}
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
				const { items: found } = await api.get<GameItemPage>(
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
		if (!open) {
			if (boss && event.key === 'ArrowDown') open = true;
			return;
		}
		if ((event.key === 'ArrowDown' || event.key === 'ArrowUp') && count > 0) {
			event.preventDefault();
			const step = event.key === 'ArrowDown' ? 1 : -1;
			active = (active + step + count) % count;
		} else if (event.key === 'Enter' && active >= 0) {
			event.preventDefault();
			if (boss) pickDrop(matches[active]);
			else pick(suggestions[active]);
		} else if (event.key === 'Escape') {
			open = false;
		}
	}
</script>

{#snippet itemName(name: string, quality: string)}
	<span class="name" style:color={QUALITY_COLOR[quality as Quality] ?? '#fff'}>{name}</span>
{/snippet}

<div class="picker" class:picked={selected?.icon || pickedIcon}>
	<input
		{id}
		{placeholder}
		aria-label={label}
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
		onfocus={() => boss && ((active = -1), (open = true))}
		onblur={() => setTimeout(() => (open = false), 150)}
	/>
	{#if selected?.icon || pickedIcon}
		<img
			class="chosen"
			src={itemIcon((selected?.icon ?? pickedIcon) as string)}
			alt=""
			width="22"
			height="22"
		/>
	{/if}
	{#if open && boss}
		<ul role="listbox" id={listbox}>
			{#each matches as drop, i (drop.item_id)}
				<li role="option" aria-selected={i === active}>
					<button type="button" class:active={i === active} onmousedown={() => pickDrop(drop)}>
						{#if drop.item_icon}
							<img src={itemIcon(drop.item_icon)} alt="" width="24" height="24" />
						{/if}
						{@render itemName(drop.item_name, drop.item_quality)}
						<span class="meta">
							{drop.count > 0 ? `Dropped ${drop.count}×` : 'In the loot plan'}
						</span>
					</button>
				</li>
			{/each}
			{#if drops.length === 0}
				<li class="hint">
					Nothing recorded from {boss.name} yet. Type the item's name to add its first drop.
				</li>
			{:else if newDrop}
				<li class="hint">“{name.trim()}” is new: it will be added as a {boss.name} drop.</li>
			{/if}
		</ul>
	{:else if open}
		<ul role="listbox" id={listbox}>
			{#each suggestions as item, i (item.id)}
				<li role="option" aria-selected={i === active}>
					<button type="button" class:active={i === active} onmousedown={() => pick(item)}>
						{#if item.icon}<img src={itemIcon(item.icon)} alt="" width="24" height="24" />{/if}
						{@render itemName(item.name, item.quality)}
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

	.hint {
		padding: var(--space-1) var(--space-2);
		color: var(--grey-text);
		font-size: var(--text-sm);
	}
</style>
