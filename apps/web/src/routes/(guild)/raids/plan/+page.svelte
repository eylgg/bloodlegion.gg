<script lang="ts">
	import { goto, invalidateAll } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { api, errorMessage } from '$lib/api';
	import { formatInZone, formatWeekSpan, raidNames } from '$lib/guild';
	import { putOn, takeOff, type Target } from '$lib/wow/planning';
	import Button from '$lib/components/Button.svelte';
	import Alert from '$lib/components/Alert.svelte';
	import RaidBuilder from '$lib/components/guild/RaidBuilder.svelte';
	import Roster from '$lib/components/guild/Roster.svelte';
	import RaidForm from '../RaidForm.svelte';
	import type { Raid, RaidDetail } from '$lib/types';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();

	// Each raid's attendees change in place as people move; a new load (another week) resets.
	let raids = $derived<RaidDetail[]>(data.raids);
	let selected = $state<number | null>(null);
	let scheduling = $state<string | null>(null);
	let error = $state('');
	let adding = $state(false);

	const size = (zone: string) => data.zones.find((z) => z.slug === zone)?.size ?? 0;
	const names = $derived(
		raidNames(
			data.zones,
			raids.map((r) => r.raid)
		)
	);
	const nameOf = (raidId: number) => names.get(raidId) ?? '';

	/** Where each character is placed this week, for the roster's badges. */
	const places = (characterId: number) =>
		raids.flatMap((r) => {
			const a = r.attendees.find((a) => a.character_id === characterId);
			return a ? [{ raidId: r.raid.id, name: nameOf(r.raid.id), group: a.group_number }] : [];
		});

	async function add(raid: RaidDetail, characterId: number, target: Target) {
		const character = data.characters.find((c) => c.id === characterId);
		if (!character) return;
		error = '';
		try {
			const next = await putOn(
				raids,
				raid,
				character,
				target,
				size(raid.raid.zone),
				(message) => confirm(message),
				nameOf
			);
			if (next) raids = next;
		} catch (err) {
			error = err instanceof Error ? errorMessage(err, err.message) : 'Adding them failed.';
			await invalidateAll();
		} finally {
			selected = null;
		}
	}

	async function remove(raidId: number, characterId: number) {
		error = '';
		try {
			raids = await takeOff(raids, raidId, characterId);
		} catch (err) {
			error = errorMessage(err, 'Taking them off failed. Please try again.');
		}
	}

	/** Another raid of `zone` at `startsLocal` (the guild's clock), then the week again. */
	async function addRaid(zone: string, startsLocal: string) {
		adding = true;
		error = '';
		try {
			await api.post<Raid>('/api/raids', { zone, starts_local: startsLocal });
			await invalidateAll();
		} catch (err) {
			error = errorMessage(err, 'Adding the raid failed. Please try again.');
		} finally {
			adding = false;
		}
	}

	/** A zone's quick add: the week's first raid night, or the schedule form when there is none. */
	function quickAdd(zone: string) {
		const first = raids[0]?.raid;
		if (first) addRaid(zone, first.starts_local);
		else scheduling = zone;
	}

	function goToWeek(value: string) {
		// eslint-disable-next-line svelte/no-navigation-without-resolve -- same page, new query
		goto(`${resolve('/raids/plan')}?week=${value}`);
	}

	// The week's dates: the calendar knows the weeks so far, and every raid carries its own.
	const span = $derived(
		data.week === null
			? null
			: (data.calendar.weeks.find((w) => w.number === data.week) ??
					raids.find((r) => r.raid.week)?.raid.week ??
					null)
	);
</script>

<svelte:head>
	<title>Raid planner | Blood Legion</title>
</svelte:head>

<svelte:window onkeydown={(e) => e.key === 'Escape' && (selected = null)} />

<div class="page-head">
	<div>
		<p class="kicker">Raid planner</p>
		<h1>
			{data.week === null ? 'Before the release' : `Week ${data.week}`}
			{#if span}<span class="muted span">{formatWeekSpan(span)}</span>{/if}
		</h1>
	</div>
	<div class="week-nav">
		<Button
			size="small"
			variant="secondary"
			disabled={data.week === null || (data.week === 1 && !data.hasBefore)}
			onclick={() => goToWeek(data.week === 1 ? 'before' : String((data.week ?? 1) - 1))}
			>Previous week</Button
		>
		<select
			aria-label="Week"
			value={data.week === null ? 'before' : String(data.week)}
			onchange={(e) => goToWeek(e.currentTarget.value)}
		>
			{#if data.hasBefore}<option value="before">Before the release</option>{/if}
			{#each data.weeks as week (week)}
				<option value={String(week)}>Week {week}</option>
			{/each}
		</select>
		<Button
			size="small"
			variant="secondary"
			onclick={() => goToWeek(data.week === null ? '1' : String(data.week + 1))}>Next week</Button
		>
	</div>
</div>

{#if data.officer}
	<div class="quick-add">
		<span class="muted small">Add a raid this week:</span>
		{#each data.zones as zone (zone.slug)}
			<Button size="small" variant="secondary" disabled={adding} onclick={() => quickAdd(zone.slug)}
				>+ {zone.name}</Button
			>
		{/each}
		{#if scheduling === null}
			<Button size="small" variant="secondary" onclick={() => (scheduling = '')}>
				At another time...
			</Button>
		{/if}
	</div>
{/if}

{#if scheduling !== null}
	<RaidForm
		zone={scheduling}
		onsaved={async (raid) => {
			scheduling = null;
			const week = raid.week ? String(raid.week.number) : 'before';
			if (week !== (data.week === null ? 'before' : String(data.week))) goToWeek(week);
			else await invalidateAll();
		}}
		oncancel={() => (scheduling = null)}
	/>
{/if}

{#if error}<Alert variant="error">{error}</Alert>{/if}

<div class="planner">
	<Roster
		characters={data.characters}
		{places}
		editable={data.officer}
		bind:selected
		onreturn={remove}
	/>

	<div class="raids">
		{#each raids as raid (raid.raid.id)}
			<section class="raid">
				<header>
					<a href={resolve('/(guild)/raids/[id]', { id: String(raid.raid.id) })}>
						<h2>{nameOf(raid.raid.id)}</h2>
					</a>
					<span class="muted">
						{formatInZone(raid.raid.starts_at, raid.raid.time_zone)} · {raid.attendees
							.length}/{size(raid.raid.zone)}
					</span>
					{#if data.officer}
						<span class="add-group">
							<Button
								size="small"
								variant="secondary"
								disabled={adding}
								title="Another {nameOf(raid.raid.id)} at the same time"
								onclick={() => addRaid(raid.raid.zone, raid.raid.starts_local)}>Add a group</Button
							>
						</span>
					{/if}
				</header>
				<RaidBuilder
					raidId={raid.raid.id}
					size={size(raid.raid.zone)}
					attendees={raid.attendees}
					onchange={(next) =>
						(raids = raids.map((r) =>
							r.raid.id === raid.raid.id ? { ...r, attendees: next } : r
						))}
					effects={data.effects}
					editable={data.officer}
					onremove={(attendee) => remove(raid.raid.id, attendee.character_id)}
					compact
					incoming={selected}
					onincoming={(characterId, target) => add(raid, characterId, target)}
				/>
			</section>
		{:else}
			<p class="muted">
				No raids {data.week === null ? 'before the release' : 'this week'} yet.
				{#if data.officer}Add one above.{/if}
			</p>
		{/each}
	</div>
</div>

<style>
	.span {
		margin-left: var(--space-2);
		font-size: var(--text-lg);
		font-weight: 400;
	}

	.week-nav,
	.quick-add {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-2);
	}

	.week-nav select {
		width: auto;
	}

	.small {
		font-size: var(--text-sm);
	}

	.planner {
		display: grid;
		grid-template-columns: 17rem minmax(0, 1fr);
		gap: var(--space-4);
		align-items: start;
	}

	@media (max-width: 52rem) {
		.planner {
			grid-template-columns: 1fr;
		}
	}

	.raids {
		display: flex;
		flex-direction: column;
		gap: var(--space-6);
	}

	.raid {
		gap: var(--space-2);
	}

	.raid header {
		display: flex;
		flex-wrap: wrap;
		align-items: baseline;
		gap: var(--space-1) var(--space-3);
	}

	.raid header a {
		color: inherit;
		text-decoration: none;
	}

	.raid header a:hover h2 {
		text-decoration: underline;
		text-underline-offset: 0.2em;
	}

	.raid h2 {
		font-size: var(--text-lg);
	}

	.add-group {
		margin-left: auto;
	}
</style>
