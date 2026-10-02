import { error } from '@sveltejs/kit';
import type { PageLoad } from './$types';
import type { GameItem, ItemDetail } from '$lib/types';
import { api, statusFrom } from '$lib/api';

export const load: PageLoad = async ({ params, fetch }) => {
	let detail: ItemDetail;
	try {
		detail = await api.get<ItemDetail>(`/api/items/${params.id}`, { fetch });
	} catch (err) {
		error(statusFrom(err, 503), 'That item could not be loaded.');
	}
	// The tooltip, when the item mirror has the item; the page works without it.
	const id = detail.item.game_item_id;
	const gameItem =
		id === null
			? null
			: await api.get<GameItem>(`/api/game-items/${id}`, { fetch }).catch(() => null);
	return { detail, gameItem };
};
