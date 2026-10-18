import { describe, expect, it } from 'vitest';
import { keyFromEvent, shortKey } from './bindings';
import { emptyKeybinds, exportString, stancesOf } from './keybinds';

const decode = (exported: string) => {
	const bytes = Uint8Array.from(atob(exported.replace(/^BLK1:/, '')), (c) => c.charCodeAt(0));
	return new TextDecoder().decode(bytes).split('\n');
};

describe('exportString', () => {
	it('places each stance on its own bonus bar and the extra bars on theirs', () => {
		const plan = emptyKeybinds();
		plan.rows['1:battle'] = ['Heroic Strike', null, 'Charge'];
		plan.rows['1:berserker'] = ['Whirlwind'];
		plan.rows['2'] = [null, 'My Macro'];
		const lines = decode(exportString(plan, 'warrior', 'Grom Hellscream', stancesOf('warrior')));

		expect(lines.slice(0, 2)).toEqual(['C\twarrior', 'N\tGrom Hellscream']);
		expect(lines).toContain('R\t73\t84');
		expect(lines).toContain('S\t73\t78\tHeroic Strike');
		expect(lines).toContain('S\t75\t100\tCharge');
		expect(lines).toContain('R\t97\t108');
		expect(lines).toContain('S\t97\t1680\tWhirlwind');
		expect(lines).toContain('R\t61\t72');
		expect(lines).toContain('M\t62\tMy Macro');
	});

	it('sends the action buttons always, and other bindings only when changed', () => {
		const plan = emptyKeybinds();
		plan.bindings.MULTIACTIONBAR1BUTTON1 = ['SHIFT-1', ''];
		plan.bindings.STRAFELEFT = ['', ''];
		const lines = decode(exportString(plan, 'mage', 'Jaina Proudmoore', stancesOf('mage')));

		expect(lines).toContain('K\tACTIONBUTTON1\t1\t');
		expect(lines).toContain('K\tMULTIACTIONBAR1BUTTON1\tSHIFT-1\t');
		expect(lines).toContain('K\tSTRAFELEFT\t\t');
		expect(lines.some((l) => l.startsWith('K\tMOVEFORWARD'))).toBe(false);
	});
});

describe('keys', () => {
	it('names keys as the game does, from the physical key', () => {
		const press = (code: string, mods: Partial<KeyboardEvent> = {}) =>
			keyFromEvent({
				code,
				altKey: false,
				ctrlKey: false,
				shiftKey: false,
				...mods
			} as KeyboardEvent);
		expect(press('Digit1', { shiftKey: true })).toBe('SHIFT-1');
		expect(press('KeyQ', { ctrlKey: true, altKey: true })).toBe('ALT-CTRL-Q');
		expect(press('Numpad4')).toBe('NUMPAD4');
		expect(press('Backquote')).toBe('`');
		expect(press('ShiftLeft', { shiftKey: true })).toBe(null);
	});

	it('shortens them as action buttons show them', () => {
		expect(shortKey('SHIFT-BUTTON4')).toBe('s-M4');
		expect(shortKey('CTRL-MOUSEWHEELUP')).toBe('c-MwU');
		expect(shortKey('NUMPAD4')).toBe('N4');
	});
});
