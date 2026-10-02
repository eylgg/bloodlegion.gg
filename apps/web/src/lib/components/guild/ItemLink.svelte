<script lang="ts">
	import { resolve } from '$app/paths';
	import { QUALITY_COLOR } from '$lib/guild';
	import type { GameItem, Quality } from '$lib/types';
	import { gameItem, itemIcon } from '$lib/wow/items';
	import ItemTooltip from './ItemTooltip.svelte';

	/**
	 * An item's name in its quality color, linking to its page. An item the mirror knows shows
	 * its icon, and its tooltip on hover or focus.
	 */
	let {
		id,
		name,
		quality,
		icon = null,
		gameItemId = null
	}: {
		id: number;
		name: string;
		quality: Quality;
		icon?: string | null;
		gameItemId?: number | null;
	} = $props();

	let open = $state(false);
	let item = $state<GameItem | null>(null);
	let anchor = $state<HTMLElement>();
	// Fixed to the window, so a scrolling table cannot clip it: below the link, or above it when
	// the link sits low on the screen, and never past the right edge.
	let position = $state({ left: 0, top: 'auto', bottom: 'auto' });

	async function show() {
		if (gameItemId === null || !anchor) return;
		const box = anchor.getBoundingClientRect();
		const gap = 6;
		const above = box.top > window.innerHeight * 0.55;
		position = {
			left: Math.max(8, Math.min(box.left, window.innerWidth - 360)),
			top: above ? 'auto' : `${box.bottom + gap}px`,
			bottom: above ? `${window.innerHeight - box.top + gap}px` : 'auto'
		};
		open = true;
		item ??= await gameItem(gameItemId);
	}
</script>

<span class="wrap" bind:this={anchor}>
	<a
		class="item"
		href={resolve('/(guild)/items/[id]', { id: String(id) })}
		style:color={QUALITY_COLOR[quality]}
		onmouseenter={show}
		onmouseleave={() => (open = false)}
		onfocus={show}
		onblur={() => (open = false)}
	>
		{#if icon}<img src={itemIcon(icon)} alt="" width="18" height="18" />{/if}<span>[{name}]</span>
	</a>
	{#if open && item?.preview}
		<div
			class="popover"
			role="tooltip"
			style:left="{position.left}px"
			style:top={position.top}
			style:bottom={position.bottom}
		>
			<ItemTooltip preview={item.preview} {name} quality={item.quality} icon={item.icon} />
		</div>
	{/if}
</span>

<style>
	.wrap {
		position: relative;
		display: inline-block;
	}

	.item {
		display: inline-flex;
		align-items: center;
		gap: var(--space-1);
		font-weight: 600;
		text-decoration: none;
	}

	.item:hover span {
		text-decoration: underline;
		text-underline-offset: 0.2em;
	}

	img {
		display: block;
		border-radius: 3px;
	}

	.popover {
		position: fixed;
		z-index: 30;
		width: max-content;
		pointer-events: none;
	}
</style>
