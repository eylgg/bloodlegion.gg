<script lang="ts">
	import { page } from '$app/state';
	import { formatDateTime } from '$lib/guild';
	import { classIcon } from '$lib/wow/icons';
	import CharacterLink from './CharacterLink.svelte';
	import type { GuildCharacter, Rank, WowClass } from '$lib/types';

	/**
	 * The launch's race to 60, for as long as it runs: the two highest of each class, and anyone
	 * tied with the second, in one table. Only raiders, trials, and members race: the leader and
	 * officers show when they are up there with them, marked as not counting, and the public not
	 * at all.
	 */
	let {
		characters,
		ranks
	}: { characters: GuildCharacter[]; ranks: { id: number; guild_rank: Rank }[] } = $props();

	const classes = $derived((page.data.classes as WowClass[] | undefined) ?? []);
	const findClass = (slug: string) => classes.find((c) => c.slug === slug);

	const rankOf = $derived(new Map(ranks.map((m) => [m.id, m.guild_rank])));
	const RACING: Rank[] = ['raider', 'trial', 'member'];
	const STAFF: Rank[] = ['leader', 'officer'];
	const counts = (c: GuildCharacter) => RACING.includes(rankOf.get(c.user_id ?? 0) ?? 'public');

	// Members' characters only (the admin also sees pugs), past level 1, and not the public's.
	const shown = $derived(
		characters.filter((c) => {
			const rank = c.user_id === null ? undefined : rankOf.get(c.user_id);
			return rank && (RACING.includes(rank) || STAFF.includes(rank)) && c.level > 1;
		})
	);

	const leaders = $derived(
		classes
			.flatMap((wowClass) => {
				const ofClass = shown.filter((c) => c.class === wowClass.slug);
				// The cutoff is the second racer's level, so two who count always show; the leader
				// and officers show above it too, without taking their places.
				const racers = ofClass.filter(counts);
				const levels = (racers.length ? racers : ofClass)
					.map((c) => c.level)
					.toSorted((a, b) => b - a);
				const cutoff = levels[1] ?? levels[0];
				return ofClass.filter((c) => c.level >= cutoff);
			})
			// Highest first; at 60, whoever got there first.
			.toSorted(
				(a, b) =>
					b.level - a.level ||
					(a.reached_60_at ?? '').localeCompare(b.reached_60_at ?? '') ||
					`${a.first_name} ${a.last_name}`.localeCompare(`${b.first_name} ${b.last_name}`)
			)
	);
</script>

<div class="scroll">
	<table>
		<thead>
			<tr>
				<th>Level</th>
				<th>Name</th>
				<th>Class</th>
				<th>Player</th>
				<th>Reached 60</th>
			</tr>
		</thead>
		<tbody>
			{#each leaders as character (character.id)}
				{@const wowClass = findClass(character.class)}
				<tr>
					<td class="level">{character.level}</td>
					<td>
						<CharacterLink
							id={character.id}
							firstName={character.first_name}
							lastName={character.last_name}
							cls={character.class}
							icon={false}
						/>
					</td>
					<td>
						<span class="with-icon" style:color={wowClass?.color}>
							<img src={classIcon(character.class)} alt="" width="20" height="20" />
							{wowClass?.name ?? character.class}
						</span>
					</td>
					<td>
						{character.username}
						{#if !counts(character)}<span class="tag" title="The leader and officers don't race"
								>not racing</span
							>{/if}
					</td>
					<td>
						{#if character.reached_60_at}
							{formatDateTime(character.reached_60_at)}
						{:else}
							<span class="muted">—</span>
						{/if}
					</td>
				</tr>
			{:else}
				<tr><td colspan="5" class="muted">No one has leveled yet.</td></tr>
			{/each}
		</tbody>
	</table>
</div>

<style>
	/* On a phone the table scrolls sideways rather than squeezing its columns. */
	.scroll {
		overflow-x: auto;
	}

	table {
		width: 100%;
		border-collapse: collapse;
		font-size: var(--text-md);
		white-space: nowrap;
	}

	th {
		padding: var(--space-2) var(--space-3);
		color: var(--grey-text);
		font-size: var(--text-xs);
		font-weight: 600;
		letter-spacing: 0.06em;
		text-align: left;
		text-transform: uppercase;
		border-bottom: 1px solid var(--grey-surface);
	}

	td {
		padding: var(--space-2) var(--space-3);
		border-bottom: 1px solid var(--grey-surface);
		vertical-align: middle;
	}

	.level {
		font-weight: 700;
		font-variant-numeric: tabular-nums;
	}

	.tag {
		margin-left: var(--space-1);
		padding: 0 4px;
		color: var(--grey-text);
		font-size: var(--text-xs);
		border: 1px solid var(--grey-surface);
		border-radius: var(--radius-sm);
	}

	.with-icon {
		display: inline-flex;
		align-items: center;
		gap: var(--space-2);
	}

	img {
		display: block;
		border-radius: var(--radius-sm);
	}
</style>
