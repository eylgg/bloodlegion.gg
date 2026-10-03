<script lang="ts">
	import { goto, invalidateAll } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { api, errorMessage } from '$lib/api';
	import { RAID_LENGTH_HOURS, formatInZone, formatWeekSpan, zoneName } from '$lib/guild';
	import Button from '$lib/components/Button.svelte';
	import Alert from '$lib/components/Alert.svelte';
	import RaidBuilder from '$lib/components/guild/RaidBuilder.svelte';
	import CharacterSpecs from '$lib/components/guild/CharacterSpecs.svelte';
	import RaidForm from '../RaidForm.svelte';
	import type { Attendee, GuildCharacter, RaidDetail } from '$lib/types';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();

	// Each raid's attendees change in place as people move; a new load (another week) resets.
	let raids = $derived<RaidDetail[]>(data.raids);
	let selected = $state<number | null>(null);
	let scheduling = $state(false);
	let error = $state('');
	let query = $state('');
	let unplacedOnly = $state(false);

	const size = (zone: string) => data.zones.find((z) => z.slug === zone)?.size ?? 0;
	const color = (cls: string) => data.classes.find((c) => c.slug === cls)?.color;
	const fullName = (c: { first_name: string; last_name: string }) =>
		`${c.first_name} ${c.last_name}`;

	/* --- the roster, by player --- */

	// One player per member account; a character no member plays stands alone.
	type Player = { key: string; username: string | null; characters: GuildCharacter[] };
	const players = $derived.by(() => {
		const byKey: Record<string, Player> = {};
		for (const c of data.characters) {
			const key = c.user_id !== null ? `u${c.user_id}` : `c${c.id}`;
			(byKey[key] ??= { key, username: c.username, characters: [] }).characters.push(c);
		}
		return Object.values(byKey)
			.map((p) => ({
				...p,
				characters: p.characters.sort((a, b) => Number(b.is_main) - Number(a.is_main))
			}))
			.sort((a, b) =>
				(a.username ?? fullName(a.characters[0])).localeCompare(
					b.username ?? fullName(b.characters[0]),
					undefined,
					{ sensitivity: 'base' }
				)
			);
	});

	/** Where a character is on this screen: each raid they are on, and their group there. */
	function places(characterId: number) {
		return raids.flatMap((r) => {
			const a = r.attendees.find((a) => a.character_id === characterId);
			return a ? [{ raid: r.raid, group: a.group_number }] : [];
		});
	}

	const shownPlayers = $derived(
		players.filter((p) => {
			const text = query.trim().toLowerCase();
			const matches =
				!text ||
				(p.username ?? '').toLowerCase().includes(text) ||
				p.characters.some((c) => fullName(c).toLowerCase().includes(text));
			const placed = p.characters.some((c) => places(c.id).length > 0);
			return matches && (!unplacedOnly || !placed);
		})
	);

	function pick(character: GuildCharacter) {
		if (!data.officer) return;
		selected = selected === character.id ? null : character.id;
	}

	/* --- adding to a raid --- */

	const overlaps = (a: string, b: string) =>
		Math.abs(Date.parse(a) - Date.parse(b)) < RAID_LENGTH_HOURS * 3600 * 1000;

	function setAttendees(raidId: number, attendees: Attendee[]) {
		raids = raids.map((r) =>
			r.raid.id === raidId
				? { ...r, raid: { ...r.raid, attendee_count: attendees.length }, attendees }
				: r
		);
	}

	/**
	 * Puts a character on `raid` at `target`. If their player is already on another raid here at
	 * the same time, or the character is saved to that zone this week, they are moved out of it
	 * (after asking) rather than refused.
	 */
	async function add(
		raid: RaidDetail,
		characterId: number,
		target: { group: number; slot: number } | null
	) {
		const character = data.characters.find((c) => c.id === characterId);
		if (!character) return;
		const samePlayer = (a: Attendee) =>
			a.character_id === characterId ||
			(character.user_id !== null && a.user_id === character.user_id);
		const conflicts = raids.flatMap((other) => {
			if (other.raid.id === raid.raid.id) return [];
			const atOnce = overlaps(other.raid.starts_at, raid.raid.starts_at);
			const saved =
				other.raid.zone === raid.raid.zone &&
				raid.raid.week !== null &&
				other.raid.week?.number === raid.raid.week.number;
			return other.attendees
				.filter((a) => (atOnce && samePlayer(a)) || (saved && a.character_id === characterId))
				.map((a) => ({ other, attendee: a, atOnce }));
		});
		error = '';
		if (conflicts.length > 0) {
			const player = character.username ?? fullName(character);
			const reasons = conflicts.map(({ other, attendee, atOnce }) => {
				const zone = zoneName(data.zones, other.raid.zone);
				return atOnce
					? `${player} is in ${zone} at the same time (on ${fullName(attendee)}).`
					: `${fullName(attendee)} is already saved to ${zone} this week.`;
			});
			if (!confirm(`${reasons.join(' ')} Move them here?`)) {
				selected = null;
				return;
			}
		}
		try {
			for (const { other, attendee } of conflicts) {
				await api.del(`/api/raids/${other.raid.id}/attendees/${attendee.character_id}`);
				setAttendees(
					other.raid.id,
					other.attendees.filter((a) => a.character_id !== attendee.character_id)
				);
			}
			const attendees = await api.put<Attendee[]>(
				`/api/raids/${raid.raid.id}/attendees/${characterId}`,
				{ group_number: target?.group ?? null, slot: target?.slot ?? null, uses_secondary: false }
			);
			setAttendees(raid.raid.id, attendees);
			selected = null;
		} catch (err) {
			error = errorMessage(err, 'Adding them failed. Please try again.');
			await invalidateAll();
		}
	}

	async function remove(raid: RaidDetail, attendee: Attendee) {
		error = '';
		try {
			await api.del(`/api/raids/${raid.raid.id}/attendees/${attendee.character_id}`);
			setAttendees(
				raid.raid.id,
				raid.attendees.filter((a) => a.character_id !== attendee.character_id)
			);
		} catch (err) {
			error = errorMessage(err, 'Removing them failed. Please try again.');
		}
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
		{#if data.officer && !scheduling}
			<Button size="small" variant="primary" onclick={() => (scheduling = true)}
				>Schedule a raid</Button
			>
		{/if}
	</div>
</div>

{#if scheduling}
	<RaidForm
		onsaved={async (raid) => {
			scheduling = false;
			const week = raid.week ? String(raid.week.number) : 'before';
			if (week !== (data.week === null ? 'before' : String(data.week))) goToWeek(week);
			else await invalidateAll();
		}}
		oncancel={() => (scheduling = false)}
	/>
{/if}

{#if error}<Alert variant="error">{error}</Alert>{/if}

<div class="planner">
	<aside class="roster" aria-label="Roster">
		<h2>Roster</h2>
		<input type="search" bind:value={query} placeholder="Player or character" aria-label="Find" />
		<label class="toggle">
			<input type="checkbox" bind:checked={unplacedOnly} />
			Only players not in a raid this week
		</label>
		{#if data.officer}
			<p class="muted small">
				Drag characters onto a raid, or click one then an empty slot. A player is in one raid at a
				time.
			</p>
		{/if}
		<ul class="players">
			{#each shownPlayers as player (player.key)}
				<li>
					{#if player.username}<span class="username">{player.username}</span>{/if}
					<ul class="chars">
						{#each player.characters as character (character.id)}
							{@const here = places(character.id)}
							<li>
								<button
									type="button"
									class="char"
									class:selected={selected === character.id}
									class:placed={here.length > 0}
									style:--class-color={color(character.class) ?? 'var(--foreground)'}
									draggable={data.officer}
									disabled={!data.officer}
									ondragstart={(e) => e.dataTransfer?.setData('text/plain', String(character.id))}
									onclick={() => pick(character)}
								>
									<span class="name">{fullName(character)}</span>
									<CharacterSpecs
										cls={character.class}
										primary={character.primary_spec}
										secondary={character.secondary_spec}
										size={16}
									/>
								</button>
								{#each here as place (place.raid.id)}
									<span class="badge" title="In {zoneName(data.zones, place.raid.zone)}">
										{zoneName(data.zones, place.raid.zone).split(' ')[0]}{place.group
											? ` ${place.group}`
											: ' bench'}
									</span>
								{/each}
							</li>
						{/each}
					</ul>
				</li>
			{:else}
				<li class="muted small">Nobody matches.</li>
			{/each}
		</ul>
	</aside>

	<div class="raids">
		{#each raids as raid (raid.raid.id)}
			<section class="raid">
				<header>
					<a href={resolve('/(guild)/raids/[id]', { id: String(raid.raid.id) })}>
						<h2>
							{zoneName(data.zones, raid.raid.zone)}{raid.raid.title ? ` · ${raid.raid.title}` : ''}
						</h2>
					</a>
					<span class="muted">
						{formatInZone(raid.raid.starts_at, raid.raid.time_zone)} · {raid.attendees
							.length}/{size(raid.raid.zone)}
					</span>
				</header>
				<RaidBuilder
					raidId={raid.raid.id}
					size={size(raid.raid.zone)}
					attendees={raid.attendees}
					onchange={(next) => setAttendees(raid.raid.id, next)}
					effects={data.effects}
					editable={data.officer}
					onremove={(attendee) => remove(raid, attendee)}
					compact
					incoming={selected}
					onincoming={(characterId, target) => add(raid, characterId, target)}
				/>
			</section>
		{:else}
			<p class="muted">
				No raids scheduled {data.week === null ? 'before the release' : 'this week'}.
				{#if data.officer}Schedule one above.{/if}
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

	.week-nav {
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

	.roster {
		position: sticky;
		top: var(--space-4);
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		max-height: calc(100vh - var(--space-8));
		padding: var(--space-3);
		overflow-y: auto;
		background-color: var(--grey-bg);
		border: 1px solid var(--grey-surface);
		border-radius: var(--radius-lg);
	}

	@media (max-width: 52rem) {
		.roster {
			position: static;
			max-height: 24rem;
		}
	}

	.roster h2 {
		font-size: var(--text-lg);
	}

	.toggle {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		font-size: var(--text-sm);
		cursor: pointer;
	}

	.players,
	.chars {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		margin: 0;
		padding: 0;
		list-style: none;
	}

	.players > li {
		padding: var(--space-1) 0;
		border-top: 1px solid var(--grey-surface);
	}

	.username {
		color: var(--grey-text);
		font-family: var(--font-mono);
		font-size: var(--text-xs);
	}

	.chars li {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-1);
	}

	.char {
		display: flex;
		flex: 1;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-2);
		min-width: 0;
		padding: 2px var(--space-2);
		font: inherit;
		text-align: left;
		background-color: var(--background);
		border: 1px solid var(--grey-surface);
		border-left: 3px solid var(--class-color);
		border-radius: var(--radius-md);
	}

	.char:not(:disabled) {
		cursor: grab;
	}

	.char.placed {
		opacity: 0.55;
	}

	.char.selected {
		opacity: 1;
		border-color: var(--accent-solid);
		box-shadow: 0 0 0 2px var(--accent-bg);
	}

	.char .name {
		overflow: hidden;
		color: var(--class-color);
		font-size: var(--text-sm);
		font-weight: 600;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.badge {
		padding: 0 4px;
		color: var(--grey-text);
		font-size: var(--text-xs);
		white-space: nowrap;
		border: 1px solid var(--grey-surface);
		border-radius: var(--radius-sm);
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
</style>
