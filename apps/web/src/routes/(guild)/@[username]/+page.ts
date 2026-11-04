import { error, redirect } from '@sveltejs/kit';
import type { PageLoad } from './$types';
import type { Member } from '$lib/types';
import { api, statusFrom } from '$lib/api';
import { playerPath } from '$lib/guild';

/**
 * A player's page: their rank and their characters, for the guild's members. The lowercase
 * username is the page's own (`/@eyl`): any other capitalization redirects there for good.
 */
export const load: PageLoad = async ({ params, fetch }) => {
	const name = params.username.toLowerCase();
	if (params.username !== name) redirect(308, playerPath(name));
	let members: Member[];
	try {
		members = await api.get<Member[]>('/api/guild/members', { fetch });
	} catch (err) {
		error(statusFrom(err, 503), 'This player is unavailable right now.');
	}
	// Usernames are unique whatever their capitalization, as the backend keeps them.
	const player = members.find((m) => m.username.toLowerCase() === name);
	if (!player) error(404, 'No such player.');
	return { player };
};
