/**
 * The keybind planner's model: a character's abilities, placed on the default UI's five action bars
 * and bound to keys. Bar 1 pages with stances and forms, so it holds one row per stance; the
 * buttons (and so their keys) are the same in every stance, only what they cast changes.
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

export type Stance = { slug: string; name: string };

/** The stances and forms that page bar 1 in Classic; every other class has the one bar. */
export const STANCES: Record<string, Stance[]> = {
	warrior: [
		{ slug: 'battle', name: 'Battle' },
		{ slug: 'defensive', name: 'Defensive' },
		{ slug: 'berserker', name: 'Berserker' }
	],
	druid: [
		{ slug: 'caster', name: 'Caster' },
		{ slug: 'bear', name: 'Bear' },
		{ slug: 'cat', name: 'Cat' }
	],
	rogue: [
		{ slug: 'normal', name: 'Normal' },
		{ slug: 'stealth', name: 'Stealth' }
	]
};

export const stancesOf = (classSlug: string): Stance[] =>
	STANCES[classSlug] ?? [{ slug: 'normal', name: '' }];

/** Where an ability sits: bar 1's rows are `1:<stance>`, the other bars their number. */
export const rowKey = (bar: Bar, stance?: string) => (bar === 1 ? `1:${stance}` : String(bar));

export type Keybinds = {
	/** The abilities to place, in the order they were added. */
	pool: string[];
	/** What each row's twelve buttons cast. */
	rows: Record<string, (string | null)[]>;
	/** Each bar's twelve keys, shared by bar 1's stances. */
	keys: Record<Bar, string[]>;
};

/** The game's own default: bar 1 on the number row, the rest unbound. */
export function emptyKeybinds(): Keybinds {
	const blank = () => Array<string>(SLOTS).fill('');
	return {
		pool: [],
		rows: {},
		keys: {
			1: ['1', '2', '3', '4', '5', '6', '7', '8', '9', '0', '-', '='],
			2: blank(),
			3: blank(),
			4: blank(),
			5: blank()
		}
	};
}

const storageKey = (characterId: number) => `bloodlegion:keybinds:${characterId}`;

/** A character's plan, kept in this browser; a missing or unreadable one is the default. */
export function loadKeybinds(characterId: number): Keybinds {
	const fresh = emptyKeybinds();
	try {
		const saved = JSON.parse(localStorage.getItem(storageKey(characterId)) ?? 'null');
		if (!saved) return fresh;
		return {
			pool: Array.isArray(saved.pool) ? saved.pool : [],
			rows: saved.rows ?? {},
			keys: { ...fresh.keys, ...saved.keys }
		};
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

/** A key press as the game labels it on a button: `s-Q`, `c-1`, `a-F`. */
export function keyLabel(event: KeyboardEvent): string | null {
	const ignored = ['Shift', 'Control', 'Alt', 'Meta', 'Tab', 'CapsLock'];
	if (ignored.includes(event.key)) return null;
	const names: Record<string, string> = { ' ': 'Spc', Escape: 'Esc', Backquote: '`' };
	let key = names[event.key] ?? event.key;
	// With Shift held, `event.key` is the shifted character; the physical key reads better.
	if (event.code.startsWith('Digit')) key = event.code.slice(5);
	else if (event.code.startsWith('Key')) key = event.code.slice(3);
	else if (event.code.startsWith('Numpad')) key = 'N' + event.code.slice(6);
	if (key.length === 1) key = key.toUpperCase();
	return modifiers(event) + key;
}

/** A mouse button past the first two, as the game labels them: M3 is the wheel click. */
export function mouseLabel(event: MouseEvent): string | null {
	// 0 and 2 are the left and right buttons; 1 is the wheel, 3 and 4 the side buttons.
	if (event.button === 0 || event.button === 2) return null;
	return modifiers(event) + 'M' + (event.button === 1 ? 3 : event.button + 1);
}

export function wheelLabel(event: WheelEvent): string {
	return modifiers(event) + (event.deltaY < 0 ? 'MwU' : 'MwD');
}

const modifiers = (event: KeyboardEvent | MouseEvent) =>
	(event.altKey ? 'a-' : '') + (event.ctrlKey ? 'c-' : '') + (event.shiftKey ? 's-' : '');
