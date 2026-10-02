import { error } from '@sveltejs/kit';
import type { PageLoad } from './$types';
import type { NoteListing } from '$lib/types';
import { api, statusFrom } from '$lib/api';

export const load: PageLoad = async ({ parent, fetch }) => {
	const { officer } = await parent();
	if (!officer) error(403, 'Only officers read the roster’s notes.');
	try {
		return { notes: await api.get<NoteListing[]>('/api/characters/notes', { fetch }) };
	} catch (err) {
		error(statusFrom(err, 503), 'The notes are unavailable right now.');
	}
};
