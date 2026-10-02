import { error } from '@sveltejs/kit';
import type { LayoutLoad } from './$types';
import type { Calendar, GuildSettings, WowClass, Zone } from '$lib/types';
import { api, statusFrom } from '$lib/api';
import { requireUser } from '$lib/auth';
import { isOfficer } from '$lib/guild';

/**
 * The guild's pages are for members: a visitor is sent to sign in first. The class and zone
 * catalogs, the raid calendar, and the guild settings are loaded once here for every page (and the components) under it.
 */
export const load: LayoutLoad = async ({ parent, url, fetch }) => {
	const { user } = await parent();
	requireUser(user, url);
	try {
		const [classes, zones, calendar, settings] = await Promise.all([
			api.get<WowClass[]>('/api/launch/classes', { fetch }),
			api.get<Zone[]>('/api/raids/zones', { fetch }),
			api.get<Calendar>('/api/raids/calendar', { fetch }),
			api.get<GuildSettings>('/api/guild/settings', { fetch })
		]);
		return { user, classes, zones, calendar, settings, officer: isOfficer(user) };
	} catch (err) {
		error(statusFrom(err, 503), 'The guild pages are unavailable right now.');
	}
};
