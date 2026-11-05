import { error } from '@sveltejs/kit';
import type { PageLoad } from './$types';
import type { BattlenetCharacter } from '$lib/types';
import { api, statusFrom } from '$lib/api';

/** The Forever characters on members' Battle.net accounts, as their latest sign-ins read them. */
export const load: PageLoad = async ({ parent, fetch }) => {
	const { user } = await parent();
	if (!user.is_superuser) error(403, 'Only admins see the Battle.net characters.');
	try {
		return {
			characters: await api.get<BattlenetCharacter[]>('/api/admin/battlenet-characters', { fetch })
		};
	} catch (err) {
		error(statusFrom(err, 503), 'The Battle.net characters are unavailable right now.');
	}
};
