import { error } from '@sveltejs/kit';
import type { LayoutLoad } from './$types';
import type { Calendar, GuildSettings, Race, WowClass, Zone } from '$lib/types';
import { api, statusFrom } from '$lib/api';
import { requireUser } from '$lib/auth';
import { isOfficer } from '$lib/guild';

/**
 * The guild's pages are for members: a visitor is sent to sign in first. The class, race, and zone
 * catalogs, the raid calendar, and the guild settings are loaded once here for every page (and the components) under it.
 */
export const load: LayoutLoad = async ({ parent, url, fetch }) => {
	const { user } = await parent();
	requireUser(user, url);
	try {
		const [classes, races, zones, calendar, settings] = await Promise.all([
			api.get<WowClass[]>('/api/wow/classes', { fetch }),
			api.get<Race[]>('/api/wow/races', { fetch }),
			api.get<Zone[]>('/api/raids/zones', { fetch }),
			api.get<Calendar>('/api/raids/calendar', { fetch }),
			api.get<GuildSettings>('/api/guild', { fetch })
		]);
		return { user, classes, races, zones, calendar, settings, officer: isOfficer(user) };
	} catch (err) {
		error(statusFrom(err, 503), 'The guild pages are unavailable right now.');
	}
};
