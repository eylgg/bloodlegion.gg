import { error } from '@sveltejs/kit';
import type { PageLoad } from './$types';
import type { Character, RosterEntry, WowClass } from '$lib/types';
import { api, statusFrom } from '$lib/api';
import { requireUser } from '$lib/auth';

export const load: PageLoad = async ({ parent, url, fetch }) => {
	const { user } = await parent();
	requireUser(user, url);
	try {
		const [classes, characters, roster] = await Promise.all([
			api.get<WowClass[]>('/api/launch/classes', { fetch }),
			api.get<Character[]>('/api/launch/characters', { fetch }),
			api.get<RosterEntry[]>('/api/launch/roster', { fetch })
		]);
		return { user, classes, characters, roster };
	} catch (err) {
		error(statusFrom(err, 503), 'Launch sign-ups are unavailable right now.');
	}
};
