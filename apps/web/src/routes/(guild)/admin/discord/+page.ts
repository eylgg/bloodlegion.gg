import { error } from '@sveltejs/kit';
import type { PageLoad } from './$types';
import type { DiscordCheck } from '$lib/types';
import { api, errorMessage } from '$lib/api';

/** The role check, read from Discord now. A failure (no bot yet, Discord down) is shown on the page. */
export const load: PageLoad = async ({ parent, fetch }) => {
	const { user } = await parent();
	if (!user.is_superuser) error(403, 'Only admins see the Discord role check.');
	try {
		return {
			check: await api.get<DiscordCheck>('/api/admin/discord/check', { fetch }),
			failure: null
		};
	} catch (err) {
		return { check: null, failure: errorMessage(err, 'The role check failed. Please try again.') };
	}
};
