/**
 * Starts dragging one character: any text selection on the page is cleared first (a browser
 * drags the selection along, which shows as several characters on the move), and the drag image
 * is the element under the pointer alone.
 */
export function dragCharacter(event: DragEvent, characterId: number, raidId: number | null = null) {
	window.getSelection()?.removeAllRanges();
	const transfer = event.dataTransfer;
	if (!transfer) return;
	transfer.clearData();
	transfer.effectAllowed = 'move';
	transfer.setData('text/plain', String(characterId));
	if (raidId !== null) transfer.setData('application/x-raid', String(raidId));
	const element = event.currentTarget as HTMLElement;
	const box = element.getBoundingClientRect();
	transfer.setDragImage(element, event.clientX - box.left, event.clientY - box.top);
}
