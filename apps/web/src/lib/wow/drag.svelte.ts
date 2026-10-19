import type { CharacterSpec } from '$lib/types';

/** Who is being dragged, enough to show them wherever they would land. */
export type Dragged = {
	characterId: number;
	/** The raid they are dragged out of; null from the roster or a loot line. */
	raidId: number | null;
	first_name: string;
	last_name: string;
	class: string;
	specs: CharacterSpec[];
};

/** Where a drag would land: a raid's slot, a place in a loot line, or back on the roster. */
export type DropTarget =
	| { kind: 'slot'; raidId: number; group: number; slot: number }
	| { kind: 'line'; id: number; index: number }
	| { kind: 'roster' };

/**
 * The drag under way, shared so every place a character can land previews the move while the
 * pointer is over it. `over` is set only through `hover` and settles once per dragover.
 */
export const drag = $state<{ current: Dragged | null; over: DropTarget | null }>({
	current: null,
	over: null
});

// Each dragover clears the target on its way down, lets the innermost drop zone name itself, and
// commits on its way back up, so leaving every zone clears it and nothing flickers in between.
let pending: DropTarget | null = null;
// The drag that started last, until it ends.
let active: Dragged | null = null;
const same = (a: DropTarget | null, b: DropTarget | null) =>
	JSON.stringify(a) === JSON.stringify(b);

/** Names the drop zone under the pointer; call from its dragover. The innermost zone wins. */
export function hover(target: DropTarget) {
	if (pending === null) pending = target;
}

function clearPending() {
	pending = null;
}

let lastOver = 0;

function commit() {
	lastOver = performance.now();
	if (!same(pending, drag.over)) drag.over = pending;
}

// Off the window no dragover follows to clear the target, and browsers disagree on telling a
// leave of the window from a leave of an element; a dragover fires every 50ms or so while the
// pointer is over the page, so none for a while means it has gone.
function leave() {
	setTimeout(() => {
		if (performance.now() - lastOver > 150) drag.over = pending = null;
	}, 200);
}

function end() {
	active = null;
	drag.current = null;
	drag.over = pending = null;
	document.removeEventListener('dragover', clearPending, true);
	document.removeEventListener('dragover', commit);
	document.removeEventListener('dragleave', leave);
	document.removeEventListener('dragend', end, true);
}

/**
 * Starts dragging one character: any text selection on the page is cleared first (a browser
 * drags the selection along, which shows as several characters on the move), and the drag image
 * is the element under the pointer alone. The page previews the move as it goes; the dragged
 * element must stay in the page until the drag ends (moved is fine, removed is not).
 */
export function dragCharacter(
	event: DragEvent,
	person: Omit<Dragged, 'raidId' | 'specs'> & { specs?: CharacterSpec[] },
	raidId: number | null = null
) {
	window.getSelection()?.removeAllRanges();
	const transfer = event.dataTransfer;
	if (!transfer) return;
	transfer.clearData();
	transfer.effectAllowed = 'move';
	transfer.setData('text/plain', String(person.characterId));
	if (raidId !== null) transfer.setData('application/x-raid', String(raidId));
	const element = event.currentTarget as HTMLElement;
	const box = element.getBoundingClientRect();
	transfer.setDragImage(element, event.clientX - box.left, event.clientY - box.top);

	end();
	document.addEventListener('dragover', clearPending, true);
	document.addEventListener('dragover', commit);
	document.addEventListener('dragleave', leave);
	document.addEventListener('dragend', end, true);
	// Also on the element itself, in case it has left the document by the end.
	element.addEventListener('dragend', end, { once: true });
	const dragged: Dragged = {
		characterId: person.characterId,
		raidId,
		first_name: person.first_name,
		last_name: person.last_name,
		class: person.class,
		specs: person.specs ?? []
	};
	active = dragged;
	// After the browser takes the drag image, so it shows the element as it was, not lifted.
	setTimeout(() => {
		if (active === dragged) drag.current = dragged;
	});
}
