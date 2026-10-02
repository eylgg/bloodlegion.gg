import type { Attendee, Effect, EffectProvider, Role, WowClass } from '$lib/types';

/** An attendee as the raid builder sees them that night: the spec they play and its talents. */
export type Member = {
	attendee: Attendee;
	spec: string | null;
	talents: string[];
	group: number | null;
};

export function member(attendee: Attendee): Member {
	const secondary = attendee.uses_secondary && attendee.secondary_spec !== null;
	return {
		attendee,
		spec: secondary ? attendee.secondary_spec : attendee.primary_spec,
		talents: secondary ? attendee.secondary_talents : attendee.primary_talents,
		group: attendee.group_number
	};
}

export function provides(provider: EffectProvider, member: Member): boolean {
	return (
		provider.class === member.attendee.class &&
		(provider.spec === null || provider.spec === member.spec) &&
		(provider.talent === null || member.talents.includes(provider.talent))
	);
}

/** How a composition covers one effect. */
export type Coverage = {
	effect: Effect;
	/** Everyone placed who brings it. */
	providers: Member[];
	/** Whether one of them has a talent that improves it. */
	improved: boolean;
	/** For a group-only buff: the occupied groups it reaches, and the ones it misses. */
	groups: { covered: number[]; missing: number[] };
};

/**
 * What the placed members bring, effect by effect. Only people in a group count: the bench is
 * standing by. A group-only buff covers only the groups its providers stand in.
 */
export function coverage(effects: Effect[], members: Member[]): Coverage[] {
	const placed = members.filter((m) => m.group !== null);
	const occupied = [...new Set(placed.map((m) => m.group as number))].sort((a, b) => a - b);
	return effects.map((effect) => {
		const providers = placed.filter((m) => effect.providers.some((p) => provides(p, m)));
		const improved = providers.some((m) => effect.improved_by.some((p) => provides(p, m)));
		const reached = new Set(providers.map((m) => m.group as number));
		const covered = effect.scope === 'party' ? occupied.filter((g) => reached.has(g)) : [];
		const missing = effect.scope === 'party' ? occupied.filter((g) => !reached.has(g)) : [];
		return { effect, providers, improved, groups: { covered, missing } };
	});
}

/**
 * Effects that share casters (a paladin keeps one blessing per class, one aura; a warlock one
 * curse): for each key, how many casters there are and how many effects want them.
 */
export function contention(
	effects: Effect[],
	members: Member[]
): { key: string; casters: number; wanted: number }[] {
	const placed = members.filter((m) => m.group !== null);
	const keys = [...new Set(effects.map((e) => e.exclusive).filter((k): k is string => !!k))];
	return keys.map((key) => {
		const shared = effects.filter((e) => e.exclusive === key);
		const casters = placed.filter((m) =>
			shared.some((e) => e.providers.some((p) => provides(p, m)))
		);
		const wanted = shared.filter((e) =>
			placed.some((m) => e.providers.some((p) => provides(p, m)))
		).length;
		return { key, casters: casters.length, wanted };
	});
}

/** The placed members by the roles of the spec they play; a spec with two roles counts in both. */
export function roles(classes: WowClass[], members: Member[]): Record<Role, number> {
	const counts: Record<Role, number> = { tank: 0, healer: 0, melee: 0, ranged: 0 };
	for (const m of members) {
		if (m.group === null || m.spec === null) continue;
		const spec = classes
			.find((c) => c.slug === m.attendee.class)
			?.specs.find((s) => s.slug === m.spec);
		for (const role of spec?.roles ?? []) counts[role] += 1;
	}
	return counts;
}
