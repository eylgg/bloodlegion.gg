<script lang="ts">
	import { page } from '$app/state';
	import { api, errorMessage } from '$lib/api';
	import { ROLES, ROLE_LABEL } from '$lib/wow/icons';
	import { contention, coverage, member, roles, type Coverage } from '$lib/wow/composition';
	import RoleIcon from '$lib/components/RoleIcon.svelte';
	import Alert from '$lib/components/Alert.svelte';
	import CharacterSpecs from './CharacterSpecs.svelte';
	import type { Attendee, Effect, Placement, WowClass } from '$lib/types';

	/**
	 * The raid's groups of five and its bench, with what the composition covers: buffs (raid-wide,
	 * and group-only ones per group), debuffs, and utility. Officers move people by dragging, or by
	 * clicking a person then where they go; moving onto someone swaps the two. Every change saves
	 * the whole layout.
	 */
	let {
		raidId,
		size,
		attendees,
		onchange,
		effects,
		editable,
		onremove,
		compact = false,
		incoming = null,
		onincoming
	}: {
		raidId: number;
		/** The zone's size: five per group. */
		size: number;
		attendees: Attendee[];
		/** The attendees with a new layout: shown at once, then as saved. */
		onchange: (attendees: Attendee[]) => void;
		effects: Effect[];
		editable: boolean;
		/** Takes someone off the raid altogether (from the bench). */
		onremove?: (attendee: Attendee) => void;
		/** A condensed coverage summary, for raids side by side. */
		compact?: boolean;
		/** A character picked outside this raid (the planner's roster), to put down here. */
		incoming?: number | null;
		/** Someone not on this raid dropped or put down here: the planner adds them. */
		onincoming?: (characterId: number, target: { group: number; slot: number } | null) => void;
	} = $props();

	const classes = $derived((page.data.classes as WowClass[] | undefined) ?? []);
	const color = (cls: string) => classes.find((c) => c.slug === cls)?.color;
	const groupNumbers = $derived(Array.from({ length: Math.floor(size / 5) }, (_, i) => i + 1));
	const SLOTS = [1, 2, 3, 4, 5];
	const at = (group: number, slot: number) =>
		attendees.find((a) => a.group_number === group && a.slot === slot);
	const bench = $derived(
		attendees
			.filter((a) => a.group_number === null)
			.sort((a, b) => a.class.localeCompare(b.class) || a.first_name.localeCompare(b.first_name))
	);

	const members = $derived(attendees.map(member));
	const placedCount = $derived(members.filter((m) => m.group !== null).length);
	const covered = $derived(coverage(effects, members));
	const roleCounts = $derived(roles(classes, members));
	const shared = $derived(contention(effects, members));
	const byKind = (kind: Effect['kind']) => covered.filter((c) => c.effect.kind === kind);

	const SHARED_LABEL: Record<string, (casters: number, wanted: number) => string> = {
		blessing: (n, w) =>
			`Blessings: ${n} ${n === 1 ? 'paladin' : 'paladins'}, so ${Math.min(n, w)} of the ${w} blessings below on each class.`,
		aura: (n) =>
			`Auras: each of the ${n} ${n === 1 ? 'paladin' : 'paladins'} keeps one aura up for their group.`,
		curse: (n, w) =>
			`Curses: ${n} ${n === 1 ? 'warlock' : 'warlocks'}, so ${Math.min(n, w)} of the ${w} curses below on the target.`,
		armor: () => "Sunder Armor and Expose Armor don't stack."
	};

	/** green: everyone has it; partial: a group-only buff some groups miss; missing: nobody. */
	function status(c: Coverage): 'covered' | 'partial' | 'missing' {
		if (c.providers.length === 0) return 'missing';
		return c.groups.missing.length > 0 ? 'partial' : 'covered';
	}

	const providerNames = (c: Coverage) =>
		c.providers.map((m) => `${m.attendee.first_name} ${m.attendee.last_name}`).join(', ');

	/* --- moving people --- */

	let selected = $state<number | null>(null);
	let dragging = $state<number | null>(null);
	let saving = $state(false);
	let error = $state('');

	type Target = { group: number; slot: number } | null;

	function placements(list: Attendee[]): Placement[] {
		return list.map((a) => ({
			character_id: a.character_id,
			group_number: a.group_number,
			slot: a.slot,
			uses_secondary: a.uses_secondary
		}));
	}

	/** Saves `next` as the layout, showing it at once and putting the old one back on failure. */
	async function save(next: Attendee[]) {
		const previous = attendees;
		onchange(next);
		saving = true;
		error = '';
		try {
			onchange(
				await api.put<Attendee[]>(`/api/raids/${raidId}/layout`, {
					placements: placements(next)
				})
			);
		} catch (err) {
			onchange(previous);
			error = errorMessage(err, 'Saving the groups failed. Please try again.');
		} finally {
			saving = false;
		}
	}

	/** Moves a character to a slot (swapping with whoever is there) or to the bench (`null`). */
	function move(characterId: number, target: Target) {
		const mover = attendees.find((a) => a.character_id === characterId);
		if (!mover) return;
		const from = { group: mover.group_number, slot: mover.slot };
		const occupant = target ? at(target.group, target.slot) : undefined;
		if (occupant?.character_id === characterId) return;
		save(
			attendees.map((a) => {
				if (a.character_id === characterId) {
					return { ...a, group_number: target?.group ?? null, slot: target?.slot ?? null };
				}
				if (occupant && a.character_id === occupant.character_id) {
					return { ...a, group_number: from.group, slot: from.slot };
				}
				return a;
			})
		);
	}

	function toggleSpec(attendee: Attendee) {
		save(
			attendees.map((a) =>
				a.character_id === attendee.character_id ? { ...a, uses_secondary: !a.uses_secondary } : a
			)
		);
	}

	/** A click on a person: select them, or put them down where the selected one was headed. */
	function choose(attendee: Attendee) {
		if (!editable) return;
		if (selected === null) {
			selected = attendee.character_id;
		} else if (selected === attendee.character_id) {
			selected = null;
		} else {
			const target =
				attendee.group_number !== null && attendee.slot !== null
					? { group: attendee.group_number, slot: attendee.slot }
					: null;
			move(selected, target);
			selected = null;
		}
	}

	/** A click on an empty slot or the bench: put the selected (or incoming) person there. */
	function drop(target: Target) {
		if (!editable) return;
		if (selected !== null) {
			move(selected, target);
			selected = null;
		} else if (incoming !== null) {
			onincoming?.(incoming, target);
		}
	}

	const onRaid = (id: number) => attendees.some((a) => a.character_id === id);

	/** A drop: someone from this raid moves; anyone else (another raid, the roster) joins. */
	function ondrop(event: DragEvent, target: Target) {
		event.preventDefault();
		const id = Number(event.dataTransfer?.getData('text/plain'));
		dragging = null;
		if (!Number.isInteger(id) || id <= 0) return;
		if (onRaid(id)) move(id, target);
		else onincoming?.(id, target);
	}

	function ondragstart(event: DragEvent, attendee: Attendee) {
		dragging = attendee.character_id;
		event.dataTransfer?.setData('text/plain', String(attendee.character_id));
	}

	const playing = (a: Attendee) =>
		a.uses_secondary && a.secondary_spec ? a.secondary_spec : a.primary_spec;
</script>

{#snippet person(attendee: Attendee)}
	<div
		class="person"
		class:selected={selected === attendee.character_id}
		style:--class-color={color(attendee.class) ?? 'var(--foreground)'}
		draggable={editable}
		role="button"
		tabindex={editable ? 0 : -1}
		aria-disabled={!editable}
		aria-pressed={selected === attendee.character_id}
		ondragstart={(e) => ondragstart(e, attendee)}
		ondragend={() => (dragging = null)}
		onclick={() => choose(attendee)}
		onkeydown={(e) =>
			(e.key === 'Enter' || e.key === ' ') && (e.preventDefault(), choose(attendee))}
	>
		<span class="who">{attendee.first_name} {attendee.last_name}</span>
		{#if editable && attendee.secondary_spec}
			<button
				type="button"
				class="spec-toggle"
				title="Switch spec for this raid"
				onclick={(e) => {
					e.stopPropagation();
					toggleSpec(attendee);
				}}
			>
				<CharacterSpecs
					cls={attendee.class}
					primary={attendee.primary_spec}
					secondary={attendee.secondary_spec}
					playing={playing(attendee)}
					size={18}
				/>
			</button>
		{:else}
			<CharacterSpecs
				cls={attendee.class}
				primary={attendee.primary_spec}
				secondary={attendee.secondary_spec}
				playing={playing(attendee)}
				size={18}
			/>
		{/if}
	</div>
{/snippet}

<div class="builder" class:compact>
	<div class="layout">
		{#if error}<Alert variant="error">{error}</Alert>{/if}
		{#if editable && !compact}
			<p class="muted small">
				Drag people into groups, or click someone then click where they go. Moving onto someone
				swaps the two. Click spec icons to switch someone's spec for this raid.
				{saving ? 'Saving...' : ''}
			</p>
		{/if}

		<div class="groups">
			{#each groupNumbers as group (group)}
				<div class="group">
					<h4>Group {group}</h4>
					<ol>
						{#each SLOTS as slot (slot)}
							{@const occupant = at(group, slot)}
							<li
								class="slot"
								class:target={editable &&
									(selected !== null || dragging !== null || incoming !== null)}
								ondragover={(e) => editable && e.preventDefault()}
								ondrop={(e) => ondrop(e, { group, slot })}
							>
								{#if occupant}
									{@render person(occupant)}
								{:else if editable}
									<button type="button" class="empty" onclick={() => drop({ group, slot })}
										>Empty</button
									>
								{:else}
									<span class="empty">Empty</span>
								{/if}
							</li>
						{/each}
					</ol>
				</div>
			{/each}
		</div>

		<div
			class="bench"
			role="list"
			ondragover={(e) => editable && e.preventDefault()}
			ondrop={(e) => ondrop(e, null)}
		>
			<h4>
				Bench <span class="muted">{bench.length}</span>
				{#if editable && (selected !== null || incoming !== null)}
					<button type="button" class="link" onclick={() => drop(null)}>Move here</button>
				{/if}
			</h4>
			{#if bench.length === 0}
				<p class="muted small">Everyone is in a group.</p>
			{:else}
				<ul>
					{#each bench as attendee (attendee.character_id)}
						<li role="listitem">
							{@render person(attendee)}
							{#if editable && onremove}
								<button
									type="button"
									class="remove"
									aria-label="Take {attendee.first_name} {attendee.last_name} off the raid"
									onclick={() => onremove(attendee)}>×</button
								>
							{/if}
						</li>
					{/each}
				</ul>
			{/if}
		</div>
	</div>

	<aside class="coverage" aria-label="What the groups bring">
		<div class="roles">
			{#each ROLES as role (role)}
				<span title={ROLE_LABEL[role]}>
					<RoleIcon {role} size={18} decorative />
					<strong>{roleCounts[role]}</strong>
					<span class="muted small">{ROLE_LABEL[role]}</span>
				</span>
			{/each}
		</div>
		{#if placedCount === 0}
			<p class="muted small">Put people in groups to see what the raid brings.</p>
		{:else if compact}
			{@const gaps = covered.filter((c) => status(c) !== 'covered')}
			<p class="small gaps">
				{#if gaps.length === 0}
					Everything covered.
				{:else}
					<span class="muted">Missing:</span>
					{#each gaps as c, i (c.effect.slug)}
						<span
							class={status(c)}
							title={c.groups.missing.length
								? `Missing in group ${c.groups.missing.join(', ')}`
								: 'Nobody brings this'}
							>{c.effect.name}{c.groups.missing.length
								? ` (${c.groups.missing.join(', ')})`
								: ''}</span
						>{i < gaps.length - 1 ? ', ' : ''}
					{/each}
				{/if}
			</p>
		{:else}
			{#each shared.filter((s) => s.casters > 0 || s.key === 'armor') as s (s.key)}
				{#if SHARED_LABEL[s.key] && s.wanted > 0}
					<p class="muted small">{SHARED_LABEL[s.key](s.casters, s.wanted)}</p>
				{/if}
			{/each}
			{#each [['buff', 'Buffs'], ['debuff', 'Debuffs'], ['utility', 'Utility']] as const as [kind, title] (kind)}
				<section>
					<h4>{title}</h4>
					<ul>
						{#each byKind(kind) as c (c.effect.slug)}
							<li
								class={status(c)}
								title={c.providers.length ? providerNames(c) : 'Nobody brings this'}
							>
								<span class="dot" aria-hidden="true"></span>
								<span class="name">
									{c.effect.name}
									{#if c.improved}<span class="tag">improved</span>{/if}
									{#if c.effect.scope === 'party'}<span class="tag">group</span>{/if}
								</span>
								<span class="detail">
									{#if c.providers.length === 0}
										none
									{:else if c.groups.missing.length > 0}
										missing in {c.groups.missing.join(', ')}
									{:else}
										{c.providers.length}
									{/if}
								</span>
							</li>
						{/each}
					</ul>
				</section>
			{/each}
		{/if}
	</aside>
</div>

<style>
	.builder {
		display: grid;
		grid-template-columns: minmax(0, 1fr) 19rem;
		gap: var(--space-4);
		align-items: start;
	}

	@media (max-width: 60rem) {
		.builder {
			grid-template-columns: 1fr;
		}
	}

	/* Side by side with other raids: the summary goes under the groups, and less is said. */
	.builder.compact {
		grid-template-columns: 1fr;
	}

	.compact .coverage {
		position: static;
	}

	.compact .groups {
		grid-template-columns: repeat(auto-fill, minmax(11rem, 1fr));
	}

	.gaps .missing {
		color: var(--grey-text);
	}

	.gaps .partial {
		color: #ffd100;
	}

	.layout {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
	}

	.small {
		font-size: var(--text-sm);
	}

	h4 {
		display: flex;
		align-items: baseline;
		gap: var(--space-2);
		margin: 0;
		font-size: var(--text-md);
	}

	.groups {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(13rem, 1fr));
		gap: var(--space-3);
	}

	.group,
	.bench,
	.coverage {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		padding: var(--space-3);
		background-color: var(--grey-bg);
		border: 1px solid var(--grey-surface);
		border-radius: var(--radius-lg);
	}

	ol,
	ul {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		margin: 0;
		padding: 0;
		list-style: none;
	}

	.slot {
		min-height: 2.25rem;
		border-radius: var(--radius-md);
	}

	.slot.target {
		outline: 1px dashed var(--grey-soft);
	}

	.empty {
		display: flex;
		align-items: center;
		width: 100%;
		height: 2.25rem;
		padding: 0 var(--space-2);
		color: var(--grey-text);
		font: inherit;
		font-size: var(--text-sm);
		background: none;
		border: 1px dashed var(--grey-surface);
		border-radius: var(--radius-md);
	}

	button.empty {
		cursor: pointer;
	}

	button.empty:hover {
		border-color: var(--grey-soft);
	}

	.person {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-2);
		height: 2.25rem;
		padding: 0 var(--space-2);
		background-color: var(--background);
		border: 1px solid var(--grey-surface);
		border-left: 3px solid var(--class-color);
		border-radius: var(--radius-md);
	}

	.person[draggable='true'] {
		cursor: grab;
	}

	.person.selected {
		border-color: var(--accent-solid);
		box-shadow: 0 0 0 2px var(--accent-bg);
	}

	.who {
		overflow: hidden;
		color: var(--class-color);
		font-size: var(--text-md);
		font-weight: 600;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.spec-toggle {
		padding: 0;
		background: none;
		border: 0;
		cursor: pointer;
	}

	.bench ul {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(13rem, 1fr));
	}

	.bench li {
		display: flex;
		align-items: center;
		gap: var(--space-1);
	}

	.bench li > :global(.person) {
		flex: 1;
		min-width: 0;
	}

	.remove,
	.link {
		color: var(--grey-text);
		font: inherit;
		background: none;
		border: 0;
		cursor: pointer;
	}

	.remove {
		font-size: var(--text-lg);
		line-height: 1;
	}

	.remove:hover {
		color: var(--red-text);
	}

	.link {
		margin-left: auto;
		font-size: var(--text-sm);
		text-decoration: underline;
	}

	.coverage {
		position: sticky;
		top: var(--space-4);
		gap: var(--space-3);
	}

	.roles {
		display: grid;
		grid-template-columns: repeat(2, 1fr);
		gap: var(--space-2);
	}

	.roles > span {
		display: flex;
		align-items: center;
		gap: var(--space-1);
	}

	.coverage section {
		gap: var(--space-1);
	}

	.coverage li {
		display: grid;
		grid-template-columns: auto 1fr auto;
		align-items: center;
		gap: var(--space-2);
		font-size: var(--text-sm);
	}

	.dot {
		width: 0.5rem;
		height: 0.5rem;
		border-radius: 50%;
	}

	.covered .dot {
		background-color: var(--green-text);
	}

	.partial .dot {
		background-color: #ffd100;
	}

	.missing .dot {
		background-color: var(--grey-soft);
	}

	.missing .name {
		color: var(--grey-text);
	}

	.tag {
		margin-left: var(--space-1);
		padding: 0 4px;
		color: var(--grey-text);
		font-size: var(--text-xs);
		border: 1px solid var(--grey-surface);
		border-radius: var(--radius-sm);
	}

	.detail {
		color: var(--grey-text);
		font-size: var(--text-xs);
		font-variant-numeric: tabular-nums;
	}

	.partial .detail {
		color: #ffd100;
	}
</style>
