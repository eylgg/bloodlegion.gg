import { error } from '@sveltejs/kit';
import type { PageLoad } from './$types';
import type { ItemDetail } from '$lib/types';
import { api, statusFrom } from '$lib/api';

export const load: PageLoad = async ({ params, fetch }) => {
	try {
		return { detail: await api.get<ItemDetail>(`/api/items/${params.id}`, { fetch }) };
	} catch (err) {
		error(statusFrom(err, 503), 'That item could not be loaded.');
	}
};
