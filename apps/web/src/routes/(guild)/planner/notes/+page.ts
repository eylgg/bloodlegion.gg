import { error } from '@sveltejs/kit';
import type { PageLoad } from './$types';
import type { NoteListing } from '$lib/types';
import { api, statusFrom } from '$lib/api';

/** The roster's notes, under the planner (whose layout lets only officers in). */
export const load: PageLoad = async ({ fetch }) => {
	try {
		return { notes: await api.get<NoteListing[]>('/api/characters/notes', { fetch }) };
	} catch (err) {
		error(statusFrom(err, 503), 'The notes are unavailable right now.');
	}
};
