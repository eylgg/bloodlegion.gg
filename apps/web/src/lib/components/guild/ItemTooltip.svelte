<script lang="ts">
	import { QUALITY_COLOR } from '$lib/guild';
	import type { ItemPreview, Quality } from '$lib/types';
	import { itemIcon } from '$lib/wow/items';

	/**
	 * An item's tooltip, drawn the way the game draws it, from the mirror's copy of Blizzard's
	 * tooltip data. Every line is the game's own display string.
	 */
	let {
		preview,
		name,
		quality,
		icon = null
	}: { preview: ItemPreview; name: string; quality: string; icon?: string | null } = $props();

	const color = $derived(QUALITY_COLOR[quality as Quality] ?? '#ffffff');
	const rgb = (c?: { r: number; g: number; b: number }) =>
		c ? `rgb(${c.r}, ${c.g}, ${c.b})` : undefined;
	// The order the game lists requirements in.
	const requirements = $derived(
		Object.values(preview.requirements ?? {})
			.map((r) => r?.display_string)
			.filter((s): s is string => Boolean(s))
	);
	const price = $derived(preview.sell_price?.display_strings);
</script>

<div class="tooltip">
	{#if icon}<img class="icon" src={itemIcon(icon)} alt="" width="40" height="40" />{/if}
	<div class="body">
		<p class="name" style:color>{preview.name ?? name}</p>
		{#if preview.binding}<p>{preview.binding.name}</p>{/if}
		{#if preview.unique_equipped}<p>{preview.unique_equipped}</p>{/if}
		{#if preview.inventory_type && preview.inventory_type.type !== 'NON_EQUIP'}
			<p class="split">
				<span>{preview.inventory_type.name}</span>
				{#if preview.item_subclass && !preview.is_subclass_hidden}
					<span>{preview.item_subclass.name}</span>
				{/if}
			</p>
		{/if}
		{#if preview.weapon?.damage}
			<p class="split">
				<span>{preview.weapon.damage.display_string}</span>
				<span>{preview.weapon.attack_speed?.display_string ?? ''}</span>
			</p>
			{#if preview.weapon.dps}<p>{preview.weapon.dps.display_string}</p>{/if}
		{/if}
		{#if preview.armor}<p>{preview.armor.display.display_string}</p>{/if}
		{#if preview.shield_block}<p>{preview.shield_block.display.display_string}</p>{/if}
		{#each preview.stats ?? [] as stat, i (i)}
			<p style:color={rgb(stat.display.color)}>{stat.display.display_string}</p>
		{/each}
		{#if preview.durability}<p>{preview.durability.display_string}</p>{/if}
		{#each requirements as requirement (requirement)}<p>{requirement}</p>{/each}
		{#each preview.spells ?? [] as spell, i (i)}
			{#if spell.description}<p class="green">{spell.description}</p>{/if}
		{/each}
		{#if preview.set}
			<p class="gold set">{preview.set.display_string}</p>
			{#each preview.set.items as member (member.item.id)}
				<p class="muted indent">{member.item.name}</p>
			{/each}
			{#each preview.set.effects as effect, i (effect.required_count)}
				<p class="muted" class:gap={i === 0}>{effect.display_string}</p>
			{/each}
		{/if}
		{#if preview.description}<p class="gold">"{preview.description}"</p>{/if}
		{#if price && (price.gold !== '0' || price.silver !== '0' || price.copper !== '0')}
			<p class="price">
				{price.header}
				{#if price.gold !== '0'}<span class="coin gold-coin">{price.gold}</span>{/if}
				{#if price.silver !== '0'}<span class="coin silver-coin">{price.silver}</span>{/if}
				{#if price.copper !== '0'}<span class="coin copper-coin">{price.copper}</span>{/if}
			</p>
		{/if}
	</div>
</div>

<style>
	/* The game's own tooltip look: a dark navy panel with a grey border, white text. */
	.tooltip {
		display: flex;
		align-items: flex-start;
		gap: var(--space-2);
		max-width: 22rem;
		color: #fff;
		font-size: 0.8125rem;
		line-height: 1.35;
		text-align: left;
	}

	.icon {
		flex: none;
		border: 1px solid #555;
		border-radius: 4px;
	}

	.body {
		padding: var(--space-2) var(--space-3);
		background-color: rgb(9 11 31 / 0.96);
		border: 1px solid #5a6177;
		border-radius: 4px;
		box-shadow: 0 4px 16px rgb(0 0 0 / 0.5);
	}

	p {
		margin: 0;
	}

	.name {
		font-size: 0.9375rem;
		font-weight: 600;
	}

	.split {
		display: flex;
		justify-content: space-between;
		gap: var(--space-4);
	}

	.green {
		color: #1eff00;
	}

	.gold {
		color: #ffd100;
	}

	.muted {
		color: #9d9d9d;
	}

	.set {
		margin-top: var(--space-2);
	}

	.indent {
		padding-left: var(--space-3);
	}

	.gap {
		margin-top: var(--space-2);
	}

	.price {
		margin-top: var(--space-1);
	}

	.coin {
		margin-left: 0.3em;
	}

	.coin::after {
		content: '';
		display: inline-block;
		width: 0.6em;
		height: 0.6em;
		margin-left: 0.15em;
		border-radius: 50%;
	}

	.gold-coin::after {
		background-color: #ffd100;
	}

	.silver-coin::after {
		background-color: #c7c7cf;
	}

	.copper-coin::after {
		background-color: #b87333;
	}
</style>
