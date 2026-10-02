import { error } from '@sveltejs/kit';
import type { PageLoad } from './$types';
import type { GameItemPage, Item } from '$lib/types';
import { api, statusFrom } from '$lib/api';

/** Items per page: the browser never loads the whole database at once. */
export const _PAGE_SIZE = 50;

/** The item database, one page at a time; the URL holds the filters and the page. */
export const load: PageLoad = async ({ url, fetch }) => {
	const page = Math.max(1, Number(url.searchParams.get('page')) || 1);
	const query = new URLSearchParams({
		limit: String(_PAGE_SIZE),
		offset: String((page - 1) * _PAGE_SIZE)
	});
	for (const key of ['q', 'quality', 'slot']) {
		const value = url.searchParams.get(key);
		if (value) query.set(key, value);
	}
	try {
		const [results, slots, items] = await Promise.all([
			api.get<GameItemPage>(`/api/game-items?${query}`, { fetch }),
			api.get<string[]>('/api/game-items/slots', { fetch }),
			api.get<Item[]>('/api/items', { fetch })
		]);
		// The guild's items the database does not know (new in Forever), listed on their own.
		return { results, slots, page, unlisted: items.filter((i) => i.game_item_id === null) };
	} catch (err) {
		error(statusFrom(err, 503), 'The items are unavailable right now.');
	}
};
