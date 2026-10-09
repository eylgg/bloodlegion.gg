import { error } from '@sveltejs/kit';
import type { PageLoad } from './$types';
import type { Boss, GuildCharacter, LootPriority, Raid, RaidDetail } from '$lib/types';
import { api, statusFrom } from '$lib/api';

/**
 * The loot plan, officers only: for each item a zone drops, who is in line for it, beside who is
 * in which of that zone's raids in a week. `?zone=` picks the zone (the first one raided that
 * week by default) and `?week=N` the week (the current one by default).
 */
export const load: PageLoad = async ({ parent, url, fetch }) => {
	const { calendar, zones, officer } = await parent();
	if (!officer) error(403, 'The loot plan is for officers.');
	try {
		const [priorities, bosses, characters, raids] = await Promise.all([
			api.get<LootPriority[]>('/api/loot-priorities', { fetch }),
			api.get<Boss[]>('/api/bosses', { fetch }),
			api.get<GuildCharacter[]>('/api/characters', { fetch }),
			api.get<Raid[]>('/api/raids', { fetch })
		]);
		const week = Number(url.searchParams.get('week')) || (calendar.current_week?.number ?? 1);
		const weekRaids = raids
			.filter((r) => r.week?.number === week)
			// The order raids are numbered in (see `raidNames`): by start, then as scheduled.
			.sort((a, b) => a.starts_at.localeCompare(b.starts_at) || a.id - b.id);
		const param = url.searchParams.get('zone');
		const zone = zones.some((z) => z.slug === param)
			? (param as string)
			: (weekRaids[0]?.zone ?? zones[0]?.slug ?? '');
		const details = await Promise.all(
			weekRaids
				.filter((r) => r.zone === zone)
				.map((r) => api.get<RaidDetail>(`/api/raids/${r.id}`, { fetch }))
		);
		const latest = Math.max(
			1,
			calendar.current_week?.number ?? 1,
			...raids.map((r) => r.week?.number ?? 0)
		);
		return {
			zone,
			week,
			weeks: Array.from({ length: latest + 1 }, (_, i) => i + 1),
			// Named among the whole week's raids, as the planner names them.
			weekRaids,
			raids: details,
			priorities,
			bosses: bosses.filter((b) => b.zone === zone),
			characters
		};
	} catch (err) {
		error(statusFrom(err, 503), 'The loot plan is unavailable right now.');
	}
};
