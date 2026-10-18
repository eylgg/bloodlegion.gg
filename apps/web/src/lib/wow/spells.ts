import catalog from './spells.json';

/**
 * Each class's Classic spells: what its trainers teach, the talents that are buttons rather than
 * passives, and the attacks every character has (Attack, Shoot, Throw). Generated from Wowhead's
 * Classic ability and talent listings, without Season of Discovery's runes; `id` is the first
 * rank, as the addon places the highest rank its character knows by name. The tooltip is the
 * highest rank's, from Wowhead's tooltips with their formulas worked out. The icons are
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
	/** The highest rank: its spell, and its name in the game (`Rank 9`; empty when unranked). */
	max: number;
	rank: string;
	/** The tooltip's lines above the description, a pair split left and right. */
	tooltip: (string | [string, string])[];
	description: string[];
};

const SPELLS = catalog as Record<string, Spell[]>;

export const spellsOf = (classSlug: string): Spell[] => SPELLS[classSlug] ?? [];

export const spellIcon = (icon: string) => `/wow/spells/${icon}.jpg`;
