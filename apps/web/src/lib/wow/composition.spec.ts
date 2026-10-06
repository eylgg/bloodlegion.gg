import { describe, expect, it } from 'vitest';
import { contention, coverage, member, playedSpec, roles } from './composition';
import type { Attendee, Effect, WowClass } from '$lib/types';

function attendee(
	id: number,
	cls: string,
	group: number,
	spec: string,
	talents: string[] = []
): Attendee {
	return {
		character_id: id,
		user_id: null,
		username: null,
		first_name: `A${id}`,
		last_name: 'Test',
		class: cls,
		is_main: true,
		specs: [{ spec, talents, is_main: true }],
		group_number: group,
		slot: 1,
		spec: null
	};
}

const by = (cls: string, talent: string | null = null) => ({ class: cls, spec: null, talent });
const effect = (
	slug: string,
	scope: 'raid' | 'party',
	providers: Effect['providers'],
	extra: Partial<Effect> = {}
): Effect => ({
	slug,
	name: slug,
	kind: 'buff',
	scope,
	category: 'Stats',
	providers,
	improved_by: [],
	exclusive: null,
	note: null,
	...extra
});

describe('raid composition', () => {
	const effects = [
		effect('arcane-intellect', 'raid', [by('mage')]),
		effect('battle-shout', 'party', [by('warrior')], {
			improved_by: [by('warrior', 'improved-battle-shout')]
		}),
		effect('shadow-weaving', 'raid', [by('priest', 'shadow-weaving')]),
		effect('blessing-of-might', 'raid', [by('paladin')], { exclusive: 'blessing' }),
		effect('blessing-of-kings', 'raid', [by('paladin', 'blessing-of-kings')], {
			exclusive: 'blessing'
		})
	];
	const members = [
		attendee(1, 'mage', 1, 'frost'),
		attendee(2, 'warrior', 1, 'fury', ['improved-battle-shout']),
		attendee(3, 'priest', 2, 'holy'),
		attendee(4, 'paladin', 2, 'holy', ['blessing-of-kings'])
	].map(member);
	const covered = Object.fromEntries(coverage(effects, members).map((c) => [c.effect.slug, c]));

	it('needs someone who brings it', () => {
		expect(covered['arcane-intellect'].providers).toHaveLength(1);
		expect(covered['shadow-weaving'].providers).toHaveLength(0);
	});

	it('reaches only the groups a group-only buff stands in', () => {
		expect(covered['battle-shout'].groups).toEqual({ covered: [1], missing: [2] });
		expect(covered['battle-shout'].improved).toBe(true);
	});

	it('needs the talent when a provider names one', () => {
		expect(covered['blessing-of-kings'].providers).toHaveLength(1);
		expect(contention(effects, members)).toEqual([{ key: 'blessing', casters: 1, wanted: 2 }]);
	});

	it('plays the spec chosen for the night, else the main, else the only one', () => {
		const priest = {
			...attendee(6, 'priest', 1, 'holy'),
			specs: [
				{ spec: 'holy', talents: [], is_main: true },
				{ spec: 'shadow', talents: ['shadow-weaving'], is_main: false }
			]
		};
		expect(coverage([effects[2]], [member(priest)])[0].providers).toHaveLength(0);
		const shadow = { ...priest, spec: 'shadow' };
		expect(coverage([effects[2]], [member(shadow)])[0].providers).toHaveLength(1);
		expect(playedSpec(priest.specs, null)?.spec).toBe('holy');
		// No main among several: undecided.
		const noMain = priest.specs.map((s) => ({ ...s, is_main: false }));
		expect(playedSpec(noMain, null)).toBeNull();
		expect(playedSpec([noMain[1]], null)?.spec).toBe('shadow');
	});

	it('counts roles from the spec played', () => {
		const classes: WowClass[] = [
			{
				slug: 'warrior',
				name: 'Warrior',
				color: '#c69b6d',
				talents: [],
				specs: [{ slug: 'fury', name: 'Fury', roles: ['melee'] }]
			},
			{
				slug: 'paladin',
				name: 'Paladin',
				color: '#f48cba',
				talents: [],
				specs: [{ slug: 'holy', name: 'Holy', roles: ['healer'] }]
			}
		];
		expect(roles(classes, members)).toMatchObject({ melee: 1, healer: 1, tank: 0 });
	});
});
