<script lang="ts">
	import { resolve } from '$app/paths';
	import { page } from '$app/state';
	import CharacterLink from './CharacterLink.svelte';
	import ItemLink from './ItemLink.svelte';
	import Button from '$lib/components/Button.svelte';
	import { formatDateInZone, zoneName } from '$lib/guild';
	import type { LootEntry, Zone } from '$lib/types';

	// Which columns to show: a raid's own page needs no raid column, a character's no winner.
	let {
		loot,
		raid = true,
		boss = true,
		winner = true,
		empty = 'No loot yet.',
		onremove
	}: {
		loot: LootEntry[];
		raid?: boolean;
		boss?: boolean;
		winner?: boolean;
		empty?: string;
		/** Offers a remove button per row (officers). */
		onremove?: (entry: LootEntry) => void;
	} = $props();

	const zones = $derived((page.data.zones as Zone[] | undefined) ?? []);
</script>

{#if loot.length === 0}
	<p class="empty">{empty}</p>
{:else}
	<div class="scroll">
		<table>
			<thead>
				<tr>
					{#if raid}<th>Raid</th>{/if}
					{#if boss}<th>Boss</th>{/if}
					<th>Item</th>
					{#if winner}<th>Winner</th>{/if}
					{#if onremove}<th><span class="visually-hidden">Actions</span></th>{/if}
				</tr>
			</thead>
			<tbody>
				{#each loot as entry (entry.id)}
					<tr>
						{#if raid}
							<td class="raid">
								<a href={resolve('/(guild)/raids/[id]', { id: String(entry.raid_id) })}>
									{zoneName(zones, entry.zone)}
								</a>
								<span class="date">
									{formatDateInZone(entry.raid_starts_at, entry.raid_time_zone)}{entry.raid_week
										? ` · Week ${entry.raid_week}`
										: ''}
								</span>
							</td>
						{/if}
						{#if boss}
							<td>
								{#if entry.boss_id !== null}
									<a href={resolve('/(guild)/bosses/[id]', { id: String(entry.boss_id) })}
										>{entry.boss_name}</a
									>
								{:else}
									<span class="muted">Trash</span>
								{/if}
							</td>
						{/if}
						<td
							><ItemLink
								id={entry.item_id}
								name={entry.item_name}
								quality={entry.item_quality}
								icon={entry.item_icon}
								gameItemId={entry.game_item_id}
							/></td
						>
						{#if winner}
							<td>
								{#if entry.character_id !== null && entry.class}
									<CharacterLink
										id={entry.character_id}
										firstName={entry.first_name ?? ''}
										lastName={entry.last_name ?? ''}
										cls={entry.class}
									/>
								{:else}
									<span class="muted">Nobody (disenchanted or banked)</span>
								{/if}
							</td>
						{/if}
						{#if onremove}
							<td class="actions">
								<Button size="small" variant="danger" onclick={() => onremove(entry)}>Remove</Button
								>
							</td>
						{/if}
					</tr>
				{/each}
			</tbody>
		</table>
	</div>
{/if}

<style>
	.scroll {
		overflow-x: auto;
	}

	table {
		width: 100%;
		border-collapse: collapse;
		font-size: var(--text-md);
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

	tr:hover td {
		background-color: var(--grey-bg);
	}

	a {
		color: inherit;
	}

	.raid {
		display: flex;
		flex-direction: column;
	}

	.date,
	.muted,
	.empty {
		color: var(--grey-text);
		font-size: var(--text-sm);
	}

	.actions {
		text-align: right;
	}

	.visually-hidden {
		position: absolute;
		width: 1px;
		height: 1px;
		overflow: hidden;
		clip-path: inset(50%);
	}
</style>
