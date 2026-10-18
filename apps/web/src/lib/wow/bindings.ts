/**
 * The game's key bindings, as its Key Bindings panel lists them: each command by its name in
 * `Bindings.xml` (what the addon passes to `SetBinding`) with two keys, the Classic defaults
 * first. Keys are spelled as the game spells them: `ALT-CTRL-SHIFT-` before the key, mouse
 * buttons as `BUTTON3`, the wheel as `MOUSEWHEELUP`.
 */

export type Command = { command: string; label: string; keys: [string, string] };
export type Category = { name: string; commands: Command[] };

const cmd = (command: string, label: string, key1 = '', key2 = ''): Command => ({
	command,
	label,
	keys: [key1, key2]
});

const ACTION_KEYS = ['1', '2', '3', '4', '5', '6', '7', '8', '9', '0', '-', '='];

/** The binding command for an action bar's button; bar 1 is the main bar, 2–5 the extra ones. */
export function buttonCommand(bar: number, slot: number) {
	const button = slot + 1;
	return bar === 1 ? `ACTIONBUTTON${button}` : `MULTIACTIONBAR${bar - 1}BUTTON${button}`;
}

const range = (n: number) => Array.from({ length: n }, (_, i) => i);

export const CATEGORIES: Category[] = [
	{
		name: 'Movement Keys',
		commands: [
			cmd('MOVEFORWARD', 'Move Forward', 'W', 'UP'),
			cmd('MOVEBACKWARD', 'Move Backward', 'S', 'DOWN'),
			cmd('TURNLEFT', 'Turn Left', 'A', 'LEFT'),
			cmd('TURNRIGHT', 'Turn Right', 'D', 'RIGHT'),
			cmd('STRAFELEFT', 'Strafe Left', 'Q'),
			cmd('STRAFERIGHT', 'Strafe Right', 'E'),
			cmd('JUMP', 'Jump', 'SPACE'),
			cmd('SITORSTAND', 'Sit/Move Down', 'X'),
			cmd('TOGGLESHEATH', 'Sheath/Unsheath Weapon', 'Z'),
			cmd('TOGGLEAUTORUN', 'Toggle Autorun', 'NUMLOCK', 'BUTTON4'),
			cmd('TOGGLERUN', 'Toggle Run/Walk', 'NUMPADDIVIDE'),
			cmd('FOLLOWTARGET', 'Follow Target')
		]
	},
	{
		name: 'Chat',
		commands: [
			cmd('OPENCHAT', 'Open Chat', 'ENTER'),
			cmd('OPENCHATSLASH', 'Open Chat Slash', '/'),
			cmd('REPLY', 'Chat Reply', 'R'),
			cmd('REPLY2', 'Re-Whisper', 'SHIFT-R')
		]
	},
	{
		name: 'Action Bar',
		commands: [
			...range(12).map((i) => cmd(buttonCommand(1, i), `Action Button ${i + 1}`, ACTION_KEYS[i])),
			...range(10).map((i) =>
				cmd(`SHAPESHIFTBUTTON${i + 1}`, `Special Action Button ${i + 1}`, `CTRL-F${i + 1}`)
			),
			...range(10).map((i) =>
				cmd(`BONUSACTIONBUTTON${i + 1}`, `Pet Action Button ${i + 1}`, `CTRL-${(i + 1) % 10}`)
			),
			cmd('ACTIONPAGE1', 'Action Page 1', 'SHIFT-1'),
			cmd('ACTIONPAGE2', 'Action Page 2', 'SHIFT-2'),
			cmd('ACTIONPAGE3', 'Action Page 3', 'SHIFT-3'),
			cmd('ACTIONPAGE4', 'Action Page 4', 'SHIFT-4'),
			cmd('ACTIONPAGE5', 'Action Page 5', 'SHIFT-5'),
			cmd('ACTIONPAGE6', 'Action Page 6', 'SHIFT-6'),
			cmd('PREVIOUSACTIONPAGE', 'Previous Action Bar', 'SHIFT-UP', 'SHIFT-MOUSEWHEELUP'),
			cmd('NEXTACTIONPAGE', 'Next Action Bar', 'SHIFT-DOWN', 'SHIFT-MOUSEWHEELDOWN')
		]
	},
	...(
		[
			[2, 'Bottom Left'],
			[3, 'Bottom Right'],
			[4, 'Right'],
			[5, 'Right 2']
		] as const
	).map(([bar, name]) => ({
		name: `MultiActionBar: ${name}`,
		commands: range(12).map((i) => cmd(buttonCommand(bar, i), `${name} Action Button ${i + 1}`))
	})),
	{
		name: 'Targeting Functions',
		commands: [
			cmd('TARGETNEARESTENEMY', 'Target Nearest Enemy', 'TAB'),
			cmd('TARGETPREVIOUSENEMY', 'Target Previous Enemy', 'SHIFT-TAB'),
			cmd('TARGETNEARESTFRIEND', 'Target Nearest Friend', 'CTRL-TAB'),
			cmd('TARGETPREVIOUSFRIEND', 'Target Previous Friend', 'CTRL-SHIFT-TAB'),
			cmd('TARGETSELF', 'Target Self', 'F1'),
			cmd('TARGETPARTYMEMBER1', 'Target Party Member 1', 'F2'),
			cmd('TARGETPARTYMEMBER2', 'Target Party Member 2', 'F3'),
			cmd('TARGETPARTYMEMBER3', 'Target Party Member 3', 'F4'),
			cmd('TARGETPARTYMEMBER4', 'Target Party Member 4', 'F5'),
			cmd('TARGETPET', 'Target Pet'),
			cmd('ASSISTTARGET', 'Assist Target', 'F'),
			cmd('ATTACKTARGET', 'Attack Target', 'T'),
			cmd('PETATTACK', 'Pet Attack'),
			cmd('INTERACTTARGET', 'Interact With Target')
		]
	},
	{
		name: 'Interface Panel Functions',
		commands: [
			cmd('TOGGLEGAMEMENU', 'Toggle Game Menu', 'ESCAPE'),
			cmd('OPENALLBAGS', 'Open All Bags', 'B'),
			cmd('TOGGLEBACKPACK', 'Toggle Backpack', 'F12'),
			cmd('TOGGLECHARACTER0', 'Toggle Character Pane', 'C'),
			cmd('TOGGLESPELLBOOK', 'Toggle Spellbook', 'P'),
			cmd('TOGGLEPETBOOK', 'Toggle Pet Book', 'SHIFT-P'),
			cmd('TOGGLETALENTS', 'Toggle Talent Pane', 'N'),
			cmd('TOGGLEQUESTLOG', 'Toggle Quest Log', 'L'),
			cmd('TOGGLEWORLDMAP', 'Toggle World Map', 'M'),
			cmd('TOGGLESOCIAL', 'Toggle Social Pane', 'O'),
			cmd('TOGGLEMINIMAP', 'Toggle Minimap')
		]
	},
	{
		name: 'Camera Functions',
		commands: [
			cmd('CAMERAZOOMIN', 'Zoom In', 'MOUSEWHEELUP', 'HOME'),
			cmd('CAMERAZOOMOUT', 'Zoom Out', 'MOUSEWHEELDOWN', 'END'),
			cmd('NEXTVIEW', 'Next View', 'PAGEDOWN'),
			cmd('PREVVIEW', 'Previous View', 'PAGEUP')
		]
	},
	{
		name: 'Miscellaneous',
		commands: [
			cmd('TOGGLEUI', 'Toggle User Interface', 'ALT-Z'),
			cmd('SCREENSHOT', 'Screen Shot', 'PRINTSCREEN'),
			cmd('TOGGLESOUND', 'Toggle Sound', 'CTRL-S'),
			cmd('TOGGLEMUSIC', 'Toggle Music', 'CTRL-M')
		]
	}
];

const COMMANDS = new Map(CATEGORIES.flatMap((c) => c.commands).map((c) => [c.command, c]));

export const commandLabel = (command: string) => COMMANDS.get(command)?.label ?? command;

/**
 * Every command's default keys. Only the action buttons and what a player changed are exported,
 * so a default here that differs from the game's never rebinds anything.
 */
export function defaultBindings(): Record<string, [string, string]> {
	return Object.fromEntries([...COMMANDS.values()].map((c) => [c.command, [...c.keys]]));
}

/* --- keys --- */

const CODES: Record<string, string> = {
	Minus: '-',
	Equal: '=',
	BracketLeft: '[',
	BracketRight: ']',
	Backslash: '\\',
	Semicolon: ';',
	Quote: "'",
	Comma: ',',
	Period: '.',
	Slash: '/',
	Backquote: '`',
	Space: 'SPACE',
	Tab: 'TAB',
	Enter: 'ENTER',
	NumpadEnter: 'ENTER',
	Backspace: 'BACKSPACE',
	Insert: 'INSERT',
	Delete: 'DELETE',
	Home: 'HOME',
	End: 'END',
	PageUp: 'PAGEUP',
	PageDown: 'PAGEDOWN',
	ArrowUp: 'UP',
	ArrowDown: 'DOWN',
	ArrowLeft: 'LEFT',
	ArrowRight: 'RIGHT',
	NumLock: 'NUMLOCK',
	PrintScreen: 'PRINTSCREEN',
	NumpadAdd: 'NUMPADPLUS',
	NumpadSubtract: 'NUMPADMINUS',
	NumpadMultiply: 'NUMPADMULTIPLY',
	NumpadDivide: 'NUMPADDIVIDE',
	NumpadDecimal: 'NUMPADDECIMAL'
};

const modifiers = (event: KeyboardEvent | MouseEvent) =>
	(event.altKey ? 'ALT-' : '') + (event.ctrlKey ? 'CTRL-' : '') + (event.shiftKey ? 'SHIFT-' : '');

/** A key press as the game names it, from the physical key (so Shift-1 is `SHIFT-1`, not `!`). */
export function keyFromEvent(event: KeyboardEvent): string | null {
	const { code } = event;
	let key: string | undefined;
	if (/^Key[A-Z]$/.test(code)) key = code.slice(3);
	else if (/^Digit\d$/.test(code)) key = code.slice(5);
	else if (/^Numpad\d$/.test(code)) key = 'NUMPAD' + code.slice(6);
	else if (/^F\d{1,2}$/.test(code)) key = code;
	else key = CODES[code];
	return key ? modifiers(event) + key : null;
}

/** A mouse button past the first two: the wheel click is BUTTON3, the side buttons 4 and 5. */
export function keyFromMouse(event: MouseEvent): string | null {
	if (event.button === 0 || event.button === 2) return null;
	return modifiers(event) + 'BUTTON' + (event.button === 1 ? 3 : event.button + 1);
}

export const keyFromWheel = (event: WheelEvent) =>
	modifiers(event) + (event.deltaY < 0 ? 'MOUSEWHEELUP' : 'MOUSEWHEELDOWN');

const SHORT: [RegExp, string][] = [
	[/ALT-/g, 'a-'],
	[/CTRL-/g, 'c-'],
	[/SHIFT-/g, 's-'],
	[/MOUSEWHEELUP/, 'MwU'],
	[/MOUSEWHEELDOWN/, 'MwD'],
	[/BUTTON(\d)/, 'M$1'],
	[/NUMPADPLUS/, 'N+'],
	[/NUMPADMINUS/, 'N-'],
	[/NUMPADMULTIPLY/, 'N*'],
	[/NUMPADDIVIDE/, 'N/'],
	[/NUMPADDECIMAL/, 'N.'],
	[/NUMPAD/, 'N'],
	[/SPACE/, 'Spc'],
	[/BACKSPACE/, 'BkSp'],
	[/ESCAPE/, 'Esc'],
	[/PAGEUP/, 'PU'],
	[/PAGEDOWN/, 'PD'],
	[/INSERT/, 'Ins'],
	[/DELETE/, 'Del'],
	[/NUMLOCK/, 'NL'],
	[/ENTER/, 'Ent'],
	[/TAB/, 'Tab'],
	[/HOME/, 'Hm'],
	[/END/, 'End'],
	[/PRINTSCREEN/, 'PrtSc']
];

/** A key as an action button shows it: `SHIFT-BUTTON4` is `s-M4`. */
export const shortKey = (key: string) => SHORT.reduce((k, [from, to]) => k.replace(from, to), key);
