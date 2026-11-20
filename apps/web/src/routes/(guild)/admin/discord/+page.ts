import { error } from '@sveltejs/kit';
import type { PageLoad } from './$types';
import type { AdminAccount, DiscordOverview } from '$lib/types';
import { api, errorMessage } from '$lib/api';

/** How the bot and the server stand, read from Discord now, and the site's accounts. */
export const load: PageLoad = async ({ parent, fetch }) => {
	const { user } = await parent();
	if (!user.is_superuser) error(403, 'Only admins see the Discord check.');
	// The accounts a Discord account can be linked to by hand; without them the page still shows.
	const accounts = api.get<AdminAccount[]>('/api/admin/users', { fetch }).catch(() => []);
	try {
		return {
			overview: await api.get<DiscordOverview>('/api/admin/discord/check', { fetch }),
			accounts: await accounts,
			failure: null
		};
	} catch (err) {
		return {
			overview: null,
			accounts: await accounts,
			failure: errorMessage(err, 'The check failed. Please try again.')
		};
	}
};
