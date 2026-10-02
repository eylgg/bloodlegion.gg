<script lang="ts">
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { formatDateTime, zoneName } from '$lib/guild';
	import Button from '$lib/components/Button.svelte';
	import RaidForm from './RaidForm.svelte';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();

	let scheduling = $state(false);

	const now = Date.now();
	const upcoming = $derived(data.raids.filter((r) => Date.parse(r.starts_at) >= now).reverse());
	const past = $derived(data.raids.filter((r) => Date.parse(r.starts_at) < now));
	const size = (zone: string) => data.zones.find((z) => z.slug === zone)?.size ?? 0;
</script>

<svelte:head>
	<title>Raids | Blood Legion</title>
</svelte:head>

<div class="page-head">
	<div>
		<p class="kicker">The guild</p>
		<h1>Raids</h1>
	</div>
	{#if data.officer && !scheduling}
		<Button variant="primary" onclick={() => (scheduling = true)}>Schedule a raid</Button>
	{/if}
</div>

{#if scheduling}
	<RaidForm
		onsaved={(raid) => goto(resolve('/(guild)/raids/[id]', { id: String(raid.id) }))}
		oncancel={() => (scheduling = false)}
	/>
{/if}

<ul class="zones" aria-label="Raids">
	{#each data.zones as zone (zone.slug)}
		<li>
			<span class="zone-name">{zone.name}</span>
			<span class="muted">{zone.size} players</span>
		</li>
	{/each}
</ul>

{#snippet list(raids: typeof data.raids, empty: string)}
	{#if raids.length === 0}
		<p class="muted">{empty}</p>
	{:else}
		<ul class="raids">
			{#each raids as raid (raid.id)}
				<li>
					<a href={resolve('/(guild)/raids/[id]', { id: String(raid.id) })}>
						<span class="zone-name">
							{zoneName(data.zones, raid.zone)}{raid.title ? ` · ${raid.title}` : ''}
						</span>
						<span class="muted">{formatDateTime(raid.starts_at)}</span>
						<span class="stats">
							<span title="Characters">{raid.attendee_count}/{size(raid.zone)}</span>
							<span title="Items won">{raid.loot_count} loot</span>
						</span>
					</a>
				</li>
			{/each}
		</ul>
	{/if}
{/snippet}

<section>
	<h2>Coming up</h2>
	{@render list(upcoming, 'Nothing scheduled.')}
</section>

<section>
	<h2>Past raids</h2>
	{@render list(past, 'No raids yet.')}
</section>

<style>
	.zones {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(12rem, 1fr));
		gap: var(--space-2);
		margin: 0;
		padding: 0;
		list-style: none;
	}

	.zones li {
		display: flex;
		flex-direction: column;
		padding: var(--space-3) var(--space-4);
		background-color: var(--grey-bg);
		border: 1px solid var(--grey-surface);
		border-left: 3px solid var(--red-solid);
		border-radius: var(--radius-lg);
	}

	.zone-name {
		font-weight: 700;
	}

	.raids {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		margin: 0;
		padding: 0;
		list-style: none;
	}

	.raids a {
		display: grid;
		grid-template-columns: 1fr auto;
		gap: 0 var(--space-4);
		padding: var(--space-3) var(--space-4);
		color: inherit;
		text-decoration: none;
		background-color: var(--grey-bg);
		border: 1px solid var(--grey-surface);
		border-radius: var(--radius-lg);
	}

	.raids a:hover {
		border-color: var(--grey-soft);
	}

	.stats {
		display: flex;
		grid-row: 1 / span 2;
		grid-column: 2;
		align-items: center;
		gap: var(--space-3);
		font-variant-numeric: tabular-nums;
	}
</style>
