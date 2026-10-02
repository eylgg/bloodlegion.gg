import { error } from '@sveltejs/kit';
import type { PageLoad } from './$types';
import type { Raid } from '$lib/types';
import { api, statusFrom } from '$lib/api';

export const load: PageLoad = async ({ fetch }) => {
	try {
		return { raids: await api.get<Raid[]>('/api/raids', { fetch }) };
	} catch (err) {
		error(statusFrom(err, 503), 'The raids are unavailable right now.');
	}
};
