import { error } from '@sveltejs/kit';
import type { PageLoad } from './$types';
import type { Boss } from '$lib/types';
import { api, statusFrom } from '$lib/api';

export const load: PageLoad = async ({ fetch }) => {
	try {
		return { bosses: await api.get<Boss[]>('/api/bosses', { fetch }) };
	} catch (err) {
		error(statusFrom(err, 503), 'The bosses are unavailable right now.');
	}
};
