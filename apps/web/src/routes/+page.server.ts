import { error } from '@sveltejs/kit';
import type { PageServerLoad } from './$types';
import type {
	Announcement,
	Calendar,
	LoginOptions,
	LoginProvider,
	LootEntry,
	Raid,
	WowClass,
	Zone
} from '$lib/types';
import { api, statusFrom } from '$lib/api';

/** How many of the latest items won a member's front page shows. */
const RECENT_LOOT = 8;

/**
 * The front page is two pages. Signed out: the ways to sign in, for its buttons, and the latest
 * Discord announcements; an unreachable backend means none of either, and the page falls back to
 * a plain link to the sign-in page, which then says what is wrong. Signed in: the member's own
 * front page (`home`), inside the guild's pages: the raids (for the next few), the latest loot, and
 * the announcements, with the zone and class catalogs and the calendar the guild's pages load.
 */
export const load: PageServerLoad = async ({ fetch, parent }) => {
	const { user } = await parent();
	const announcements = api
		.get<Announcement[]>('/api/announcements', { fetch })
		.catch((): Announcement[] => []);
	if (!user) {
		const providers = await api
			.get<LoginOptions>('/api/auth/providers', { fetch })
			.then((options) => options.providers)
			.catch((): LoginProvider[] => []);
		return {
			providers,
			announcements: await announcements,
			zones: [] as Zone[],
			classes: [] as WowClass[],
			home: null
		};
	}
	try {
		const [zones, classes, calendar, raids, loot] = await Promise.all([
			api.get<Zone[]>('/api/raids/zones', { fetch }),
			api.get<WowClass[]>('/api/wow/classes', { fetch }),
			api.get<Calendar>('/api/raids/calendar', { fetch }),
			api.get<Raid[]>('/api/raids', { fetch }),
			api.get<LootEntry[]>(`/api/loot?limit=${RECENT_LOOT}`, { fetch })
		]);
		return {
			providers: [] as LoginProvider[],
			announcements: await announcements,
			// Top level, where the loot table looks for them as on the guild's pages.
			zones,
			classes,
			home: { user, calendar, raids, loot }
		};
	} catch (err) {
		error(statusFrom(err, 503), 'The guild pages are unavailable right now.');
	}
};
