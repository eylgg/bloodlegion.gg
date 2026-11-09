import { error } from '@sveltejs/kit';
import type { PageLoad } from './$types';
import type { DiscordSettings, RosterStanding } from '$lib/types';
import { api } from '$lib/api';

export const load: PageLoad = async ({ parent, fetch }) => {
	const { user } = await parent();
	if (!user.is_superuser) error(403, 'Only admins change the guild settings.');
	const [discord, standing] = await Promise.all([
		api.get<DiscordSettings>('/api/admin/discord', { fetch }),
		api.get<RosterStanding>('/api/admin/rosters/standing', { fetch })
	]);
	return { discord, standing };
};
