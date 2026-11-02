<script lang="ts">
	import { page } from '$app/state';
	import { formatDateTime } from '$lib/guild';
	import { classIcon } from '$lib/wow/icons';
	import CharacterLink from './CharacterLink.svelte';
	import type { GuildCharacter, WowClass } from '$lib/types';

	// The launch's race to 60, for as long as it runs: the two highest of each class, and anyone
	// tied with the second, in one table.
	let { characters }: { characters: GuildCharacter[] } = $props();

	const classes = $derived((page.data.classes as WowClass[] | undefined) ?? []);
	const findClass = (slug: string) => classes.find((c) => c.slug === slug);

	// Members' characters only (the admin also sees pugs), past level 1.
	const racing = $derived(characters.filter((c) => c.user_id !== null && c.level > 1));

	const leaders = $derived(
		classes
			.flatMap((wowClass) => {
				const ofClass = racing.filter((c) => c.class === wowClass.slug);
				const levels = ofClass.map((c) => c.level).toSorted((a, b) => b - a);
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
					<td>{character.username}</td>
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
