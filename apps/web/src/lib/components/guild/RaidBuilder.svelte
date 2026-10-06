<script lang="ts">
	import { page } from '$app/state';
	import { api, errorMessage } from '$lib/api';
	import { ROLES, ROLE_LABEL } from '$lib/wow/icons';
	import {
		contention,
		coverage,
		member,
		playedSpec,
		roles,
		type Coverage
	} from '$lib/wow/composition';
	import RoleIcon from '$lib/components/RoleIcon.svelte';
	import Alert from '$lib/components/Alert.svelte';
	import CharacterSpecs from './CharacterSpecs.svelte';
	import type { Attendee, Effect, Placement, WowClass } from '$lib/types';

	/**
	 * A raid's groups of five, each with the group-only buffs it has, and what the whole raid
	 * covers: buffs, debuffs, utility, and roles. Officers move people by dragging, or by clicking
	 * one then where they go; moving onto someone swaps the two. People come from outside (the
	 * roster, another raid) through `onincoming`, and leave by × or by being dragged back.
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
		/** Takes someone off the raid. */
		onremove?: (attendee: Attendee) => void;
		/** A condensed coverage summary, for raids side by side. */
		compact?: boolean;
		/** A character picked outside this raid (the roster), to put down here. */
		incoming?: number | null;
		/** Someone not on this raid dropped or put down at a place. */
		onincoming?: (characterId: number, target: { group: number; slot: number }) => void;
	} = $props();

	const classes = $derived((page.data.classes as WowClass[] | undefined) ?? []);
	const color = (cls: string) => classes.find((c) => c.slug === cls)?.color;
	const groupNumbers = $derived(Array.from({ length: Math.floor(size / 5) }, (_, i) => i + 1));
	const SLOTS = [1, 2, 3, 4, 5];
	const at = (group: number, slot: number) =>
		attendees.find((a) => a.group_number === group && a.slot === slot);

	const members = $derived(attendees.map(member));
	const covered = $derived(coverage(effects, members));
	const roleCounts = $derived(roles(classes, members));
	const shared = $derived(contention(effects, members));
	const byKind = (kind: Effect['kind']) => covered.filter((c) => c.effect.kind === kind);
	/** The group-only buffs someone in `group` brings it. */
	const partyBuffs = (group: number) =>
		covered.filter(
			(c) =>
				c.effect.scope === 'party' &&
				c.effect.kind === 'buff' &&
				c.providers.some((m) => m.group === group)
		);

	const SHARED_LABEL: Record<string, (casters: number, wanted: number) => string> = {
		blessing: (n, w) =>
			`Blessings: ${n} ${n === 1 ? 'paladin' : 'paladins'}, so ${Math.min(n, w)} of the ${w} blessings below on each class.`,
		aura: (n) =>
			`Auras: each of the ${n} ${n === 1 ? 'paladin' : 'paladins'} keeps one aura up for their group.`,
		curse: (n, w) =>
			`Curses: ${n} ${n === 1 ? 'warlock' : 'warlocks'}, so ${Math.min(n, w)} of the ${w} curses below on the target.`,
		armor: () => "Sunder Armor and Expose Armor don't stack."
	};

	/** covered: everyone has it; partial: a group-only buff some groups miss; missing: nobody. */
	function status(c: Coverage): 'covered' | 'partial' | 'missing' {
		if (c.providers.length === 0) return 'missing';
		return c.groups.missing.length > 0 ? 'partial' : 'covered';
	}

	const providerNames = (c: Coverage) =>
		c.providers.map((m) => `${m.attendee.first_name} ${m.attendee.last_name}`).join(', ');

	/* --- moving people --- */

	let selected = $state<number | null>(null);
	let saving = $state(false);
	let error = $state('');

	type Target = { group: number; slot: number };

	/** Saves `next`, showing it at once and putting the old layout back on failure. */
	async function save(next: Attendee[]) {
		const previous = attendees;
		onchange(next);
		saving = true;
		error = '';
		const placements: Placement[] = next.map((a) => ({
			character_id: a.character_id,
			group_number: a.group_number,
			slot: a.slot,
			spec: a.spec
		}));
		try {
			onchange(await api.put<Attendee[]>(`/api/raids/${raidId}/layout`, { placements }));
		} catch (err) {
			onchange(previous);
			error = errorMessage(err, 'Saving the groups failed. Please try again.');
		} finally {
			saving = false;
		}
	}

	/** Moves a character to a slot, swapping with whoever is there. */
	function move(characterId: number, target: Target) {
		const mover = attendees.find((a) => a.character_id === characterId);
		if (!mover) return;
		const occupant = at(target.group, target.slot);
		if (occupant?.character_id === characterId) return;
		save(
			attendees.map((a) => {
				if (a.character_id === characterId) {
					return { ...a, group_number: target.group, slot: target.slot };
				}
				if (occupant && a.character_id === occupant.character_id) {
					return { ...a, group_number: mover.group_number, slot: mover.slot };
				}
				return a;
			})
		);
	}

	/** Moves someone on to the next spec they play, for this raid. */
	function nextSpec(attendee: Attendee) {
		const specs = attendee.specs;
		const now = specs.findIndex((s) => s.spec === playing(attendee));
		const next = specs[(now + 1) % specs.length].spec;
		save(
			attendees.map((a) => (a.character_id === attendee.character_id ? { ...a, spec: next } : a))
		);
	}

	const onRaid = (id: number) => attendees.some((a) => a.character_id === id);

	/** A click on a person: select them, or swap the selected one with them. */
	function choose(attendee: Attendee) {
		if (!editable) return;
		if (selected === attendee.character_id) {
			selected = null;
		} else if (selected !== null) {
			move(selected, { group: attendee.group_number, slot: attendee.slot });
			selected = null;
		} else if (incoming !== null && !onRaid(incoming)) {
			onincoming?.(incoming, { group: attendee.group_number, slot: attendee.slot });
		} else {
			selected = attendee.character_id;
		}
	}

	/** A click on a free slot: put the selected (or incoming) person there. */
	function put(target: Target) {
		if (!editable) return;
		if (selected !== null) {
			move(selected, target);
			selected = null;
		} else if (incoming !== null) {
			if (onRaid(incoming)) move(incoming, target);
			else onincoming?.(incoming, target);
		}
	}

	/** A drop: someone from this raid moves; anyone else (the roster, another raid) joins. */
	function ondrop(event: DragEvent, target: Target) {
		event.preventDefault();
		const id = Number(event.dataTransfer?.getData('text/plain'));
		if (!Number.isInteger(id) || id <= 0) return;
		if (onRaid(id)) move(id, target);
		else onincoming?.(id, target);
	}

	function ondragstart(event: DragEvent, attendee: Attendee) {
		event.dataTransfer?.setData('text/plain', String(attendee.character_id));
		event.dataTransfer?.setData('application/x-raid', String(raidId));
	}

	const playing = (a: Attendee) => playedSpec(a.specs, a.spec)?.spec ?? null;
	const picking = $derived(editable && (selected !== null || incoming !== null));
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
		onclick={() => choose(attendee)}
		onkeydown={(e) =>
			(e.key === 'Enter' || e.key === ' ') && (e.preventDefault(), choose(attendee))}
	>
		<span class="who">{attendee.first_name} {attendee.last_name}</span>
		{#if editable && attendee.specs.length > 1}
			<button
				type="button"
				class="plain"
				title="Switch the spec they play in this raid"
				onclick={(e) => {
					e.stopPropagation();
					nextSpec(attendee);
				}}
			>
				<CharacterSpecs
					cls={attendee.class}
					specs={attendee.specs}
					playing={playing(attendee)}
					size={18}
				/>
			</button>
		{:else}
			<CharacterSpecs
				cls={attendee.class}
				specs={attendee.specs}
				playing={playing(attendee)}
				size={18}
			/>
		{/if}
		{#if editable && onremove}
			<button
				type="button"
				class="plain remove"
				aria-label="Take {attendee.first_name} {attendee.last_name} off the raid"
				onclick={(e) => {
					e.stopPropagation();
					onremove(attendee);
				}}>×</button
			>
		{/if}
	</div>
{/snippet}

<div class="builder" class:compact>
	<div class="layout">
		{#if error}<Alert variant="error">{error}</Alert>{/if}
		{#if saving}<p class="muted small">Saving...</p>{/if}
		<div class="groups">
			{#each groupNumbers as group (group)}
				{@const buffs = partyBuffs(group)}
				<div class="group">
					<h4>Group {group}</h4>
					<ol>
						{#each SLOTS as slot (slot)}
							{@const occupant = at(group, slot)}
							<li
								class="slot"
								class:target={picking}
								ondragover={(e) => editable && e.preventDefault()}
								ondrop={(e) => ondrop(e, { group, slot })}
							>
								{#if occupant}
									{@render person(occupant)}
								{:else if editable}
									<button type="button" class="empty" onclick={() => put({ group, slot })}
										>Empty</button
									>
								{:else}
									<span class="empty">Empty</span>
								{/if}
							</li>
						{/each}
					</ol>
					<p class="party" title="Group-only buffs this group has">
						{#if buffs.length === 0}
							<span class="muted">No group buffs</span>
						{:else}
							{#each buffs as c (c.effect.slug)}
								<span class="buff" class:improved={c.improved}>{c.effect.name}</span>
							{/each}
						{/if}
					</p>
				</div>
			{/each}
		</div>
	</div>

	<aside class="coverage" aria-label="What the raid brings">
		<div class="roles">
			{#each ROLES as role (role)}
				<span title={ROLE_LABEL[role]}>
					<RoleIcon {role} size={18} decorative />
					<strong>{roleCounts[role]}</strong>
					<span class="muted small">{ROLE_LABEL[role]}</span>
				</span>
			{/each}
		</div>
		{#if attendees.length === 0}
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

	.layout {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
	}

	.small {
		font-size: var(--text-sm);
	}

	h4 {
		margin: 0;
		font-size: var(--text-md);
	}

	.groups {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(13rem, 1fr));
		gap: var(--space-3);
	}

	.compact .groups {
		grid-template-columns: repeat(auto-fill, minmax(11.5rem, 1fr));
	}

	.group,
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
		gap: var(--space-2);
		height: 2.25rem;
		padding: 0 var(--space-1) 0 var(--space-2);
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
		flex: 1;
		overflow: hidden;
		color: var(--class-color);
		font-size: var(--text-md);
		font-weight: 600;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.plain {
		padding: 0;
		color: var(--grey-text);
		font: inherit;
		background: none;
		border: 0;
		cursor: pointer;
	}

	.remove {
		padding: 0 var(--space-1);
		font-size: var(--text-lg);
		line-height: 1;
	}

	.remove:hover {
		color: var(--red-text);
	}

	.party {
		display: flex;
		flex-wrap: wrap;
		gap: 3px;
		margin: 0;
		font-size: var(--text-xs);
	}

	.buff {
		padding: 0 5px;
		color: var(--green-text);
		background-color: var(--green-bg);
		border: 1px solid var(--green-soft);
		border-radius: var(--radius-sm);
	}

	.buff.improved {
		font-weight: 600;
	}

	.coverage {
		position: sticky;
		top: var(--space-4);
		gap: var(--space-3);
	}

	.compact .coverage {
		position: static;
	}

	.roles {
		display: grid;
		grid-template-columns: repeat(2, 1fr);
		gap: var(--space-2);
	}

	.compact .roles {
		grid-template-columns: repeat(4, auto);
		justify-content: start;
		gap: var(--space-4);
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

	.partial .detail,
	.gaps .partial {
		color: #ffd100;
	}

	.gaps .missing {
		color: var(--grey-text);
	}
</style>
