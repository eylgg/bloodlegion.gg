import { buttonCommand, defaultBindings } from './bindings';
import { spellsOf } from './spells';

/**
 * The keybind planner's model: a character's spells, placed on the default UI's five action bars,
 * and the game's key bindings. Bar 1 pages with stances and forms, so it holds one row per stance;
 * the buttons (and so their keys) are the same in every stance, only what they cast changes.
 */

export const SLOTS = 12;
export const BARS = [1, 2, 3, 4, 5] as const;
export type Bar = (typeof BARS)[number];

export const BAR_LABEL: Record<Bar, string> = {
	1: 'Action Bar',
	2: 'Bottom Left',
	3: 'Bottom Right',
	4: 'Right',
	5: 'Right 2'
};

/** Each bar's first action slot in the game (`PlaceAction`); bar 1's depends on the stance. */
const BAR_SLOT: Record<Exclude<Bar, 1>, number> = { 2: 61, 3: 49, 4: 25, 5: 37 };

/** A stance or form, and where its bar 1 lives: 1 with none, 73 onward for the bonus bars. */
export type Stance = { slug: string; name: string; slot: number };

/** The stances and forms that page bar 1 in Classic; every other class has the one bar. */
export const STANCES: Record<string, Stance[]> = {
	warrior: [
		{ slug: 'battle', name: 'Battle', slot: 73 },
		{ slug: 'defensive', name: 'Defensive', slot: 85 },
		{ slug: 'berserker', name: 'Berserker', slot: 97 }
	],
	druid: [
		{ slug: 'caster', name: 'Caster', slot: 1 },
		{ slug: 'cat', name: 'Cat', slot: 73 },
		{ slug: 'prowl', name: 'Prowl', slot: 85 },
		{ slug: 'bear', name: 'Bear', slot: 97 }
	],
	rogue: [
		{ slug: 'normal', name: 'Normal', slot: 1 },
		{ slug: 'stealth', name: 'Stealth', slot: 73 }
	]
};

export const stancesOf = (classSlug: string): Stance[] =>
	STANCES[classSlug] ?? [{ slug: 'normal', name: '', slot: 1 }];

/** Where an ability sits: bar 1's rows are `1:<stance>`, the other bars their number. */
export const rowKey = (bar: Bar, stance?: string) => (bar === 1 ? `1:${stance}` : String(bar));

export type Keybinds = {
	/** Macros and items to place, by name: anything that is not one of the class's spells. */
	custom: string[];
	/** What each row's twelve buttons cast, by name. */
	rows: Record<string, (string | null)[]>;
	/** Each binding command's two keys. */
	bindings: Record<string, [string, string]>;
};

export function emptyKeybinds(): Keybinds {
	return { custom: [], rows: {}, bindings: defaultBindings() };
}

const storageKey = (characterId: number) => `bloodlegion:keybinds:${characterId}`;

/** A character's plan, kept in this browser; a missing or unreadable one is the default. */
export function loadKeybinds(characterId: number, classSlug: string): Keybinds {
	const fresh = emptyKeybinds();
	try {
		const saved = JSON.parse(localStorage.getItem(storageKey(characterId)) ?? 'null');
		if (!saved) return fresh;
		const bindings = { ...fresh.bindings, ...saved.bindings };
		// The first plans kept each bar's keys and a pool of typed names.
		if (saved.keys)
			for (const [bar, keys] of Object.entries(saved.keys as Record<string, string[]>))
				keys.forEach((key, slot) => (bindings[buttonCommand(Number(bar), slot)] = [key, '']));
		const known = new Set(spellsOf(classSlug).map((s) => s.name));
		const custom: string[] =
			saved.custom ?? (saved.pool ?? []).filter((n: string) => !known.has(n));
		return { custom, rows: saved.rows ?? {}, bindings };
	} catch {
		return fresh;
	}
}

export function saveKeybinds(characterId: number, keybinds: Keybinds) {
	try {
		localStorage.setItem(storageKey(characterId), JSON.stringify(keybinds));
	} catch {
		// Private windows and full storage: the plan still works, it just isn't kept.
	}
}

/**
 * The plan as the addon imports it: `BLK1:` and the base64 of tab-separated lines.
 *
 * - `C class`, `N name`: whose plan it is, shown before importing.
 * - `R first last`: action slots the plan covers, cleared before placing.
 * - `S slot id name`: a spell, placed at its highest rank the character knows.
 * - `M slot name`: a macro by name, else an item.
 * - `K command key1 key2`: a binding; the action buttons always, other commands when changed.
 */
export function exportString(
	plan: Keybinds,
	classSlug: string,
	name: string,
	stances: Stance[]
): string {
	const spells = new Map(spellsOf(classSlug).map((s) => [s.name, s]));
	const lines = [`C\t${classSlug}`, `N\t${name}`];
	const place = (row: (string | null)[] | undefined, first: number) => {
		lines.push(`R\t${first}\t${first + SLOTS - 1}`);
		row?.forEach((ability, i) => {
			if (!ability) return;
			const spell = spells.get(ability);
			lines.push(
				spell ? `S\t${first + i}\t${spell.id}\t${ability}` : `M\t${first + i}\t${ability}`
			);
		});
	};
	for (const stance of stances) place(plan.rows[rowKey(1, stance.slug)], stance.slot);
	for (const bar of [2, 3, 4, 5] as const) place(plan.rows[rowKey(bar)], BAR_SLOT[bar]);

	const defaults = defaultBindings();
	for (const [command, keys] of Object.entries(plan.bindings)) {
		const isButton = /^(ACTIONBUTTON|MULTIACTIONBAR)/.test(command);
		const changed = keys.join('\t') !== defaults[command]?.join('\t');
		if (isButton || changed) lines.push(`K\t${command}\t${keys[0]}\t${keys[1]}`);
	}
	const bytes = new TextEncoder().encode(lines.join('\n'));
	return 'BLK1:' + btoa(String.fromCharCode(...bytes));
}
