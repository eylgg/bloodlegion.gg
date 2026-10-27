import { error } from '@sveltejs/kit';
import type { PageLoad } from './$types';
import type { DiscordOverview } from '$lib/types';
import { api, errorMessage } from '$lib/api';

/** How the bot and the server stand, read from Discord now. */
export const load: PageLoad = async ({ parent, fetch }) => {
	const { user } = await parent();
	if (!user.is_superuser) error(403, 'Only admins see the Discord check.');
	try {
		return {
			overview: await api.get<DiscordOverview>('/api/admin/discord/check', { fetch }),
			failure: null
		};
	} catch (err) {
		return { overview: null, failure: errorMessage(err, 'The check failed. Please try again.') };
	}
};
