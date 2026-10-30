import { error, redirect } from '@sveltejs/kit';
import type { LayoutLoad } from './$types';
import type { Calendar, GuildSettings, Race, WowClass, Zone } from '$lib/types';
import { api, statusFrom } from '$lib/api';
import { requireUser } from '$lib/auth';
import { isGuildMember, isOfficer } from '$lib/guild';

/**
 * The guild's pages are for members: a visitor is sent to sign in first, and the public (a new
 * account an officer has not given a rank yet) only have their profile, to link Discord. The class, race, and zone
 * catalogs, the raid calendar, and the guild settings are loaded once here for every page (and the components) under it.
 */
export const load: LayoutLoad = async ({ parent, url, route, fetch }) => {
	const { user } = await parent();
	requireUser(user, url);
	const member = isGuildMember(user);
	if (!member && route.id !== '/(guild)/profile') redirect(303, '/');
	try {
		const [classes, races, zones, calendar, settings] = await Promise.all([
			api.get<WowClass[]>('/api/wow/classes', { fetch }),
			api.get<Race[]>('/api/wow/races', { fetch }),
			api.get<Zone[]>('/api/raids/zones', { fetch }),
			api.get<Calendar>('/api/raids/calendar', { fetch }),
			// Members only: the public's profile does not use it.
			member ? api.get<GuildSettings>('/api/guild', { fetch }) : (null as unknown as GuildSettings)
		]);
		return { user, classes, races, zones, calendar, settings, member, officer: isOfficer(user) };
	} catch (err) {
		error(statusFrom(err, 503), 'The guild pages are unavailable right now.');
	}
};
