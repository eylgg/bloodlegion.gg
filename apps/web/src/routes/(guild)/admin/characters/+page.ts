import { error } from '@sveltejs/kit';
import type { PageLoad } from './$types';
import type { AdminCharacter } from '$lib/types';
import { api, statusFrom } from '$lib/api';

/** Every character on the site, guilded or not. */
export const load: PageLoad = async ({ parent, fetch }) => {
	const { user } = await parent();
	if (!user.is_superuser) error(403, 'Only admins see every character.');
	try {
		return { characters: await api.get<AdminCharacter[]>('/api/admin/characters', { fetch }) };
	} catch (err) {
		error(statusFrom(err, 503), 'The characters are unavailable right now.');
	}
};
