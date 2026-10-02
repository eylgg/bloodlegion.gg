import { error } from '@sveltejs/kit';
import type { PageLoad } from './$types';
import type { Member } from '$lib/types';
import { api, statusFrom } from '$lib/api';

export const load: PageLoad = async ({ fetch }) => {
	try {
		return { members: await api.get<Member[]>('/api/guild/members', { fetch }) };
	} catch (err) {
		error(statusFrom(err, 503), 'The roster is unavailable right now.');
	}
};
