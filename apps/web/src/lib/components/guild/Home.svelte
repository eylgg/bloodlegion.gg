<script lang="ts">
	import { resolve } from '$app/paths';
	import Announcements from '$lib/components/Announcements.svelte';
	import LootTable from '$lib/components/guild/LootTable.svelte';
	import { formatInZone, formatWeekSpan, raidNames } from '$lib/guild';
	import type { Announcement, Calendar, LootEntry, Raid, User, Zone } from '$lib/types';

	// A member's front page, signed in: the mark with a welcome, the next raids, the latest loot,
	// and the Discord announcements.
	let {
		user,
		zones,
		calendar,
		raids,
		loot,
		announcements
	}: {
		user: User;
		zones: Zone[];
		calendar: Calendar;
		/** Latest first, as `GET /api/raids` lists them. */
		raids: Raid[];
		loot: LootEntry[];
		announcements: Announcement[];
	} = $props();

	/** How many of the coming raids it lists. */
	const COMING = 4;

	const names = $derived(raidNames(zones, raids));
	const now = Date.now();
	// The raids come latest first; the coming ones read soonest first.
	const coming = $derived(
		raids
			.filter((r) => Date.parse(r.starts_at) >= now)
			.reverse()
			.slice(0, COMING)
	);
	const size = (zone: string) => zones.find((z) => z.slug === zone)?.size ?? 0;
	const week = $derived(calendar.current_week);
</script>

<!-- The mark, as on the front page, with who is here and where the week stands. -->
<header class="hero">
	<img src="/logo.svg" alt="Blood Legion" width="1024" height="1024" fetchpriority="high" />
	<div class="greeting">
		<p class="kicker">
			{week ? `Week ${week.number} · ${formatWeekSpan(week)}` : 'Before the release'}
		</p>
		<h1>Welcome back, {user.username}</h1>
		<a class="launch" href={resolve('/launch')}>WoW: Forever launch sign-ups</a>
	</div>
</header>

<div class="columns">
	<div class="main">
		<section>
			<div class="head">
				<h2>Coming up</h2>
				<a href={resolve('/raids')}>All raids</a>
			</div>
			{#if coming.length === 0}
				<p class="muted">Nothing scheduled.</p>
			{:else}
				<ul class="raids">
					{#each coming as raid (raid.id)}
						<li>
							<a href={resolve('/(guild)/raids/[id]', { id: String(raid.id) })}>
								<span class="zone-name">{names.get(raid.id)}</span>
								<span class="muted">{formatInZone(raid.starts_at, raid.time_zone)}</span>
								<span class="stats" title="Characters signed up">
									{raid.attendee_count}/{size(raid.zone)}
								</span>
							</a>
						</li>
					{/each}
				</ul>
			{/if}
		</section>

		<section>
			<div class="head">
				<h2>Recent loot</h2>
				<a href={resolve('/loot')}>All loot</a>
			</div>
			<LootTable {loot} empty="No loot yet." />
		</section>
	</div>

	{#if announcements.length > 0}
		<aside>
			<Announcements {announcements} fill />
		</aside>
	{/if}
</div>

<style>
	.hero {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		justify-content: center;
		gap: var(--space-2) var(--space-6);
		text-align: center;
	}

	/* The square canvas carries its own clear space, so it can sit tight against the greeting. */
	.hero img {
		display: block;
		width: min(60vw, 16rem);
		height: auto;
		margin-block: calc(var(--space-6) * -1);
		animation: reveal 600ms ease-out both;
	}

	.greeting {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
	}

	.launch {
		color: var(--grey-text);
		font-size: var(--text-sm);
	}

	.launch:hover {
		color: var(--foreground);
	}

	@media (min-width: 48rem) {
		.hero {
			justify-content: flex-start;
			text-align: left;
		}
	}

	@keyframes reveal {
		from {
			opacity: 0;
			transform: scale(0.97);
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.hero img {
			animation: none;
		}
	}

	.columns {
		display: grid;
		gap: var(--space-8);
	}

	@media (min-width: 64rem) {
		.columns:has(aside) {
			grid-template-columns: minmax(0, 3fr) minmax(0, 2fr);
		}
	}

	.main {
		display: flex;
		flex-direction: column;
		gap: var(--space-8);
		min-width: 0;
	}

	.head {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: var(--space-3);
	}

	.head a {
		color: var(--grey-text);
		font-size: var(--text-sm);
	}

	.head a:hover {
		color: var(--foreground);
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
		border-left: 3px solid var(--red-solid);
		border-radius: var(--radius-lg);
	}

	.raids a:hover {
		border-color: var(--grey-soft);
		border-left-color: var(--red-solid);
	}

	.zone-name {
		font-weight: 700;
	}

	.stats {
		grid-row: 1 / span 2;
		grid-column: 2;
		align-self: center;
		font-variant-numeric: tabular-nums;
	}
</style>
