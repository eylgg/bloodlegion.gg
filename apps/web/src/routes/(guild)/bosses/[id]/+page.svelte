<script lang="ts">
	import { zoneName } from '$lib/guild';
	import ItemLink from '$lib/components/guild/ItemLink.svelte';
	import LootTable from '$lib/components/guild/LootTable.svelte';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();

	const detail = $derived(data.detail);
	const percent = (count: number) =>
		detail.kills === 0 ? '' : `${Math.round((count / detail.kills) * 100)}%`;
</script>

<svelte:head>
	<title>{detail.boss.name} | Blood Legion</title>
</svelte:head>

<div class="page-head">
	<div>
		<p class="kicker">{zoneName(data.zones, detail.boss.zone)}</p>
		<h1>{detail.boss.name}</h1>
		<p class="muted">
			{detail.kills}
			{detail.kills === 1 ? 'kill' : 'kills'} with loot recorded
		</p>
	</div>
</div>

<section>
	<h2>Drops</h2>
	{#if detail.drops.length === 0}
		<p class="muted">Nothing recorded yet.</p>
	{:else}
		<ul class="drops">
			{#each detail.drops as drop (drop.item_id)}
				<li>
					<ItemLink id={drop.item_id} name={drop.item_name} quality={drop.item_quality} />
					<span class="bar" style:--share={drop.count / Math.max(detail.kills, 1)}></span>
					<span class="rate">{percent(drop.count)}</span>
					<span class="muted">{drop.count}×</span>
				</li>
			{/each}
		</ul>
	{/if}
</section>

<section>
	<h2>History</h2>
	<LootTable loot={detail.loot} boss={false} />
</section>

<style>
	.drops {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		max-width: 44rem;
		margin: 0;
		padding: 0;
		list-style: none;
	}

	.drops li {
		display: grid;
		grid-template-columns: minmax(0, 1fr) 8rem 3rem 2.5rem;
		align-items: center;
		gap: var(--space-3);
	}

	.bar {
		height: 0.5rem;
		background: linear-gradient(
			90deg,
			var(--red-solid) calc(var(--share) * 100%),
			var(--grey-surface) 0
		);
		border-radius: 999px;
	}

	.rate {
		font-variant-numeric: tabular-nums;
		font-weight: 600;
		text-align: right;
	}

	@media (max-width: 34rem) {
		.drops li {
			grid-template-columns: minmax(0, 1fr) 3rem 2.5rem;
		}

		.bar {
			display: none;
		}
	}
</style>
