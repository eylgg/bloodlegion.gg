<script lang="ts">
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { formatDateTime, formatInZone, formatWeekSpan, zoneCity, zoneName } from '$lib/guild';
	import Button from '$lib/components/Button.svelte';
	import RaidForm from './RaidForm.svelte';
	import type { Raid } from '$lib/types';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();

	let scheduling = $state(false);

	const calendar = $derived(data.calendar);
	const now = Date.now();
	const upcoming = $derived(data.raids.filter((r) => Date.parse(r.starts_at) >= now).reverse());
	const past = $derived(data.raids.filter((r) => Date.parse(r.starts_at) < now));
	const size = (zone: string) => data.zones.find((z) => z.slug === zone)?.size ?? 0;

	// Past raids by raiding week, latest first; raids before the release go last, on their own.
	const pastByWeek = $derived.by(() => {
		const groups: { key: string; label: string; span: string; raids: Raid[] }[] = [];
		for (const raid of past) {
			const key = raid.week ? String(raid.week.number) : 'before';
			let group = groups.find((g) => g.key === key);
			if (!group) {
				group = {
					key,
					label: raid.week ? `Week ${raid.week.number}` : 'Before the release',
					span: raid.week ? formatWeekSpan(raid.week) : '',
					raids: []
				};
				groups.push(group);
			}
			group.raids.push(raid);
		}
		return groups;
	});

	// The reset is fixed in UTC; the reader sees it in their own time, which moves by an hour when
	// daylight saving starts or ends. Shown at the current week's end, or the first week's before
	// the release.
	const nextReset = $derived(calendar.current_week?.ends_at ?? null);
	const released = $derived(calendar.current_week !== null);
	const releaseCity = $derived(zoneCity(calendar.release_time_zone));
</script>

<svelte:head>
	<title>Raids | Blood Legion</title>
</svelte:head>

<div class="page-head">
	<div>
		<p class="kicker">
			{calendar.current_week
				? `Week ${calendar.current_week.number} · ${formatWeekSpan(calendar.current_week)}`
				: 'The guild'}
		</p>
		<h1>Raids</h1>
	</div>
	<div class="head-actions">
		<Button variant="secondary" href={resolve('/raids/plan')}>Plan the week</Button>
		{#if data.officer && !scheduling}
			<Button variant="primary" onclick={() => (scheduling = true)}>Schedule a raid</Button>
		{/if}
	</div>
</div>

<div class="calendar panel">
	<p>
		{released ? 'The raids opened' : 'The raids open'}
		<strong>{formatInZone(calendar.release_at, calendar.release_time_zone)}</strong>
		in {releaseCity}<span class="muted">, {formatDateTime(calendar.release_at)} your time</span>.
	</p>
	<p>
		Lockouts reset every {calendar.reset_weekday.replace(/^./, (c) => c.toUpperCase())} at
		{calendar.reset_time_utc} UTC.
		{#if nextReset}
			The next is <strong>{formatDateTime(nextReset)}</strong> your time.
		{/if}
		Week 1 runs from the release to the first reset.
	</p>
	<p class="muted">
		Raids are scheduled on {zoneCity(data.settings.time_zone)} time ({data.settings.time_zone}),
		usually at {data.settings.default_raid_time}.
	</p>
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

{#snippet list(raids: Raid[])}
	<ul class="raids">
		{#each raids as raid (raid.id)}
			<li>
				<a href={resolve('/(guild)/raids/[id]', { id: String(raid.id) })}>
					<span class="zone-name">
						{zoneName(data.zones, raid.zone)}{raid.title ? ` · ${raid.title}` : ''}
					</span>
					<span class="muted">
						{formatInZone(raid.starts_at, raid.time_zone)}{raid.week
							? ` · Week ${raid.week.number}`
							: ''}
					</span>
					<span class="stats">
						<span title="Characters">{raid.attendee_count}/{size(raid.zone)}</span>
						<span title="Items won">{raid.loot_count} loot</span>
					</span>
				</a>
			</li>
		{/each}
	</ul>
{/snippet}

<section>
	<h2>Coming up</h2>
	{#if upcoming.length === 0}
		<p class="muted">Nothing scheduled.</p>
	{:else}
		{@render list(upcoming)}
	{/if}
</section>

<section>
	<h2>Past raids</h2>
	{#each pastByWeek as group (group.key)}
		<div class="week">
			<h3>
				{group.label}
				{#if group.span}<span class="muted">{group.span}</span>{/if}
			</h3>
			{@render list(group.raids)}
		</div>
	{:else}
		<p class="muted">No raids yet.</p>
	{/each}
</section>

<style>
	.head-actions {
		display: flex;
		gap: var(--space-2);
	}

	.calendar {
		gap: var(--space-2);
		padding: var(--space-4) var(--space-6);
		font-size: var(--text-md);
	}

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

	.week {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}

	.week h3 {
		margin: 0;
		font-size: var(--text-lg);
	}

	.week h3 .muted {
		margin-left: var(--space-2);
		font-weight: 400;
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
