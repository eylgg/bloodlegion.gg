import { error, redirect } from '@sveltejs/kit';
import type { PageLoad } from './$types';
import type { GameItem } from '$lib/types';
import { api, statusFrom } from '$lib/api';

/**
 * An item from the game's database. Once the guild has its own item for it (it has been won), its
 * page is that item's, with who won it; until then, this shows the tooltip alone.
 */
export const load: PageLoad = async ({ params, fetch }) => {
	let item: GameItem;
	try {
		item = await api.get<GameItem>(`/api/game-items/${params.id}`, { fetch });
	} catch (err) {
		error(statusFrom(err, 503), 'That item is not in the database.');
	}
	if (item.guild_item_id !== null) redirect(307, `/items/${item.guild_item_id}`);
	return { item };
};
