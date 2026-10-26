import { error } from '@sveltejs/kit';
import type { PageLoad } from './$types';
import type { DiscordSettings } from '$lib/types';
import { api } from '$lib/api';

export const load: PageLoad = async ({ parent, fetch }) => {
	const { user } = await parent();
	if (!user.is_superuser) error(403, 'Only admins change the guild settings.');
	return { discord: await api.get<DiscordSettings>('/api/discord/settings', { fetch }) };
};
