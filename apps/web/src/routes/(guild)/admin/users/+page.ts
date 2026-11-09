import { error } from '@sveltejs/kit';
import type { PageLoad } from './$types';
import type { AdminAccount } from '$lib/types';
import { api, statusFrom } from '$lib/api';

/** Every account, with the Battle.net and Discord accounts linked to it. */
export const load: PageLoad = async ({ parent, fetch }) => {
	const { user } = await parent();
	if (!user.is_superuser) error(403, 'Only admins see the users.');
	try {
		return { accounts: await api.get<AdminAccount[]>('/api/admin/users', { fetch }) };
	} catch (err) {
		error(statusFrom(err, 503), 'The users are unavailable right now.');
	}
};
