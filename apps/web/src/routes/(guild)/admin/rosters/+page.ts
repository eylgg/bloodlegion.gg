import { error } from '@sveltejs/kit';
import type { PageLoad } from './$types';
import type { GameRank, RosterStanding, RosterUpload } from '$lib/types';
import { api, statusFrom } from '$lib/api';

/** The latest rosters the Blood Legion client sent, with what each changed. */
export const load: PageLoad = async ({ parent, fetch }) => {
	const { user } = await parent();
	if (!user.is_superuser) error(403, 'Only admins see the roster uploads.');
	try {
		const [uploads, standing, ranks] = await Promise.all([
			api.get<RosterUpload[]>('/api/admin/rosters', { fetch }),
			api.get<RosterStanding>('/api/admin/rosters/standing', { fetch }),
			api.get<GameRank[]>('/api/admin/rosters/ranks', { fetch })
		]);
		return { uploads, standing, ranks };
	} catch (err) {
		error(statusFrom(err, 503), 'The roster uploads are unavailable right now.');
	}
};
