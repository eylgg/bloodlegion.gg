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
	import RaidForm from '../raids/RaidForm.svelte';
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

	/** A zone's quick add: on the week's first raid night, or its first evening when it has none. */
	function quickAdd(zone: string) {
		addRaid(zone, raids[0]?.raid.starts_local ?? data.defaultStart);
	}

	/** Deletes a raid nobody is on and nothing was won in. */
	async function deleteRaid(raidId: number) {
		error = '';
		try {
			await api.del(`/api/raids/${raidId}`);
			raids = raids.filter((r) => r.raid.id !== raidId);
		} catch (err) {
			error = errorMessage(err, 'Deleting the raid failed. Please try again.');
		}
	}

	function goToWeek(value: string) {
		// eslint-disable-next-line svelte/no-navigation-without-resolve -- same page, new query
		goto(`${resolve('/planner')}?week=${value}`);
	}

	const span = $derived(data.span);
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
	<Roster characters={data.characters} {places} editable bind:selected onreturn={remove} />

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
					<span class="add-group">
						{#if raid.attendees.length === 0 && raid.raid.loot_count === 0}
							<Button
								size="small"
								variant="danger"
								title="Delete this empty raid"
								onclick={() => deleteRaid(raid.raid.id)}>Delete</Button
							>
						{/if}
						<Button
							size="small"
							variant="secondary"
							disabled={adding}
							title="Another {nameOf(raid.raid.id)} at the same time"
							onclick={() => addRaid(raid.raid.zone, raid.raid.starts_local)}>Add a group</Button
						>
					</span>
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
					editable
					onremove={(attendee) => remove(raid.raid.id, attendee.character_id)}
					compact
					incoming={selected}
					onincoming={(characterId, target) => add(raid, characterId, target)}
				/>
			</section>
		{:else}
			<p class="muted">
				No raids {data.week === null ? 'before the release' : 'this week'} yet. Add one above.
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
		display: flex;
		gap: var(--space-2);
		margin-left: auto;
	}
</style>
