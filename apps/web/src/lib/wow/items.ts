import { api } from '$lib/api';
import type { GameItem } from '$lib/types';

/** An icon from the item mirror, served by the backend. */
export const itemIcon = (name: string) => `/api/game-items/icons/${name}.jpg`;

// Tooltips are fetched on first hover and kept for the page's life: the mirror changes daily at
// most. A failed fetch is forgotten, so the next hover tries again.
const cache = new Map<number, Promise<GameItem | null>>();

/** A mirrored item with its tooltip, or null when the mirror lacks it. */
export function gameItem(id: number): Promise<GameItem | null> {
	let found = cache.get(id);
	if (!found) {
		found = api.get<GameItem>(`/api/game-items/${id}`).catch(() => {
			cache.delete(id);
			return null;
		});
		cache.set(id, found);
	}
	return found;
}
