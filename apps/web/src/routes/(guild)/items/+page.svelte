<script lang="ts">
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { page as current } from '$app/state';
	import { QUALITIES, QUALITY_COLOR, QUALITY_LABEL } from '$lib/guild';
	import Button from '$lib/components/Button.svelte';
	import ItemLink from '$lib/components/guild/ItemLink.svelte';
	import { gameItem, itemIcon } from '$lib/wow/items';
	import ItemForm from './ItemForm.svelte';
	import ItemTooltip from '$lib/components/guild/ItemTooltip.svelte';
	import { _PAGE_SIZE } from './+page';
	import type { GameItem, GameItemSummary, Quality } from '$lib/types';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();

	let adding = $state(false);
	// The search box follows the URL, and updates it a moment after typing stops.
	const param = (key: string) => current.url.searchParams.get(key) ?? '';
	let query = $state(param('q'));
	let timer: ReturnType<typeof setTimeout> | undefined;

	const pages = $derived(Math.max(1, Math.ceil(data.results.total / _PAGE_SIZE)));
	const first = $derived(data.results.total === 0 ? 0 : (data.page - 1) * _PAGE_SIZE + 1);
	const last = $derived(Math.min(data.page * _PAGE_SIZE, data.results.total));

	/** Navigates to the same page with `changes` applied; a new filter starts over at page 1. */
	function navigate(changes: Record<string, string>) {
		const paging = 'page' in changes;
		const params: Record<string, string> = Object.fromEntries(current.url.searchParams);
		if (!paging) delete params.page;
		Object.assign(params, changes);
		const kept = Object.entries(params).filter(
			([key, value]) => value && !(key === 'page' && value === '1')
		);
		const search = new URLSearchParams(kept).toString();
		// eslint-disable-next-line svelte/no-navigation-without-resolve -- same page, new query
		goto(`${current.url.pathname}${search ? `?${search}` : ''}`, {
			keepFocus: true,
			noScroll: !paging,
			replaceState: !paging
		});
	}

	function typed() {
		clearTimeout(timer);
		timer = setTimeout(() => navigate({ q: query.trim() }), 250);
	}

	const href = (item: GameItemSummary) =>
		item.guild_item_id !== null
			? resolve('/(guild)/items/[id]', { id: String(item.guild_item_id) })
			: resolve('/(guild)/items/game/[id]', { id: String(item.id) });

	// The tooltip of the row under the pointer, fetched on first hover.
	let hovered = $state<{ item: GameItem; top: number; left: number } | null>(null);
	// The row the pointer is on now, so a slow fetch for one it already left shows nothing.
	let pointing: number | null = null;
	async function hover(event: MouseEvent, id: number) {
		pointing = id;
		const box = (event.currentTarget as HTMLElement).getBoundingClientRect();
		const found = await gameItem(id);
		if (found?.preview && pointing === id) {
			const above = box.top > window.innerHeight * 0.55;
			hovered = {
				item: found,
				left: Math.max(8, Math.min(box.left, window.innerWidth - 360)),
				top: above ? -(window.innerHeight - box.top + 6) : box.bottom + 6
			};
		}
	}
</script>

<svelte:head>
	<title>Items | Blood Legion</title>
</svelte:head>

<div class="page-head">
	<div>
		<p class="kicker">The game's item database</p>
		<h1>Items</h1>
	</div>
	{#if data.officer && !adding}
		<Button variant="secondary" onclick={() => (adding = true)}>Add an item not listed</Button>
	{/if}
</div>

<p class="muted">
	Every epic and legendary item, synced from Blizzard: Classic Era until WoW: Forever's is
	available. Items the guild has won show how often.
</p>

{#if adding}
	<ItemForm
		onsaved={(item) => goto(resolve('/(guild)/items/[id]', { id: String(item.id) }))}
		oncancel={() => (adding = false)}
	/>
{/if}

<div class="row">
	<label class="field">
		Search
		<input type="search" bind:value={query} oninput={typed} placeholder="Item name" />
	</label>
	<label class="field">
		Quality
		<select value={param('quality')} onchange={(e) => navigate({ quality: e.currentTarget.value })}>
			<option value="">Any quality</option>
			{#each QUALITIES as q (q)}
				<option value={q}>{QUALITY_LABEL[q]}</option>
			{/each}
		</select>
	</label>
	<label class="field">
		Slot
		<select value={param('slot')} onchange={(e) => navigate({ slot: e.currentTarget.value })}>
			<option value="">Any slot</option>
			{#each data.slots as slot (slot)}
				<option value={slot}>{slot}</option>
			{/each}
		</select>
	</label>
</div>

<p class="muted">
	{#if data.results.total === 0}
		No items match.
	{:else}
		{first.toLocaleString()}–{last.toLocaleString()} of {data.results.total.toLocaleString()}
	{/if}
</p>

{#if data.results.items.length > 0}
	<ul class="items">
		{#each data.results.items as item (item.id)}
			<li>
				<a
					href={href(item)}
					onmouseenter={(e) => hover(e, item.id)}
					onmouseleave={() => {
						pointing = null;
						hovered = null;
					}}
				>
					{#if item.icon}
						<img src={itemIcon(item.icon)} alt="" width="32" height="32" loading="lazy" />
					{:else}
						<span class="no-icon"></span>
					{/if}
					<span class="name" style:color={QUALITY_COLOR[item.quality as Quality] ?? '#fff'}
						>{item.name}</span
					>
					<span class="meta">
						{item.slot}{item.item_subclass && item.item_subclass !== item.slot
							? ` · ${item.item_subclass}`
							: ''} · {item.item_level}
					</span>
				</a>
				{#if item.drops > 0}<span class="drops" title="Times the guild won it">{item.drops}×</span
					>{/if}
			</li>
		{/each}
	</ul>
{/if}

{#if hovered?.item.preview}
	<div
		class="popover"
		role="tooltip"
		style:left="{hovered.left}px"
		style:top={hovered.top >= 0 ? `${hovered.top}px` : 'auto'}
		style:bottom={hovered.top < 0 ? `${-hovered.top}px` : 'auto'}
	>
		<ItemTooltip
			preview={hovered.item.preview}
			name={hovered.item.name}
			quality={hovered.item.quality}
			icon={hovered.item.icon}
		/>
	</div>
{/if}

{#if pages > 1}
	<nav class="pager" aria-label="Pages">
		<Button
			variant="secondary"
			size="small"
			disabled={data.page <= 1}
			onclick={() => navigate({ page: String(data.page - 1) })}>Previous</Button
		>
		<span class="muted">Page {data.page} of {pages.toLocaleString()}</span>
		<Button
			variant="secondary"
			size="small"
			disabled={data.page >= pages}
			onclick={() => navigate({ page: String(data.page + 1) })}>Next</Button
		>
	</nav>
{/if}

{#if data.unlisted.length > 0}
	<section>
		<h2>Not in the database</h2>
		<p class="muted">The guild's own items the game's database does not have yet.</p>
		<ul class="unlisted">
			{#each data.unlisted as item (item.id)}
				<li>
					<ItemLink id={item.id} name={item.name} quality={item.quality} />
					<span class="muted">{item.drops}×</span>
				</li>
			{/each}
		</ul>
	</section>
{/if}

<style>
	.items {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(20rem, 1fr));
		gap: var(--space-1) var(--space-4);
		margin: 0;
		padding: 0;
		list-style: none;
	}

	.items li {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		border-bottom: 1px solid var(--grey-surface);
	}

	.items a {
		display: grid;
		flex: 1;
		grid-template-columns: auto 1fr;
		column-gap: var(--space-3);
		min-width: 0;
		padding: var(--space-2) var(--space-1);
		color: inherit;
		text-decoration: none;
		border-radius: var(--radius-md);
	}

	.items a:hover {
		background-color: var(--grey-bg);
	}

	.items img,
	.no-icon {
		grid-row: 1 / span 2;
		width: 32px;
		height: 32px;
		border-radius: var(--radius-sm);
	}

	.no-icon {
		background-color: var(--grey-surface);
	}

	.name {
		overflow: hidden;
		font-weight: 600;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.meta {
		color: var(--grey-text);
		font-size: var(--text-xs);
	}

	.drops {
		flex: none;
		color: var(--grey-text);
		font-size: var(--text-sm);
		font-variant-numeric: tabular-nums;
	}

	.popover {
		position: fixed;
		z-index: 30;
		width: max-content;
		pointer-events: none;
	}

	.pager {
		display: flex;
		align-items: center;
		justify-content: center;
		gap: var(--space-4);
	}

	.unlisted {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		margin: 0;
		padding: 0;
		list-style: none;
	}

	.unlisted li {
		display: flex;
		justify-content: space-between;
		max-width: 28rem;
	}
</style>
