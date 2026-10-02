import { error } from '@sveltejs/kit';
import type { PageLoad } from './$types';
import type { Item } from '$lib/types';
import { api, statusFrom } from '$lib/api';

export const load: PageLoad = async ({ fetch }) => {
	try {
		return { items: await api.get<Item[]>('/api/items', { fetch }) };
	} catch (err) {
		error(statusFrom(err, 503), 'The items are unavailable right now.');
	}
};
