import catalog from './spells.json';

/**
 * Each class's Classic spells: what its trainers teach, the talents that are buttons rather than
 * passives, and the attacks every character has (Attack, Shoot, Throw). Generated from Wowhead's
 * Classic ability and talent listings, without Season of Discovery's runes; `id` is the first
 * rank, as the addon places the highest rank its character knows by name. The icons are
 * Blizzard's, vendored under `static/wow/spells` from its render service.
 */
export type Spell = {
	id: number;
	name: string;
	icon: string;
	/** The level its first rank is trained at; 0 for a talent. */
	level: number;
	/** The talent tree's spec slug, or null for the general attacks. */
	tree: string | null;
	talent: boolean;
	ranks: number;
};

const SPELLS = catalog as Record<string, Spell[]>;

export const spellsOf = (classSlug: string): Spell[] => SPELLS[classSlug] ?? [];

export const spellIcon = (icon: string) => `/wow/spells/${icon}.jpg`;
