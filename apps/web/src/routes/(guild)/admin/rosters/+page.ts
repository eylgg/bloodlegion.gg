import { error } from '@sveltejs/kit';
import type { PageLoad } from './$types';
import type { RosterUpload } from '$lib/types';
import { api, statusFrom } from '$lib/api';

/** The latest rosters the Blood Legion client sent, with what each changed. */
export const load: PageLoad = async ({ parent, fetch }) => {
	const { user } = await parent();
	if (!user.is_superuser) error(403, 'Only admins see the roster uploads.');
	try {
		return { uploads: await api.get<RosterUpload[]>('/api/admin/rosters', { fetch }) };
	} catch (err) {
		error(statusFrom(err, 503), 'The roster uploads are unavailable right now.');
	}
};
