import { error } from '@sveltejs/kit';
import type { PageLoad } from './$types';
import type { Effect, GuildCharacter, Raid, RaidDetail } from '$lib/types';
import { api, statusFrom } from '$lib/api';

/**
 * The raid planner: one raid week's raids side by side, with the roster to fill them from.
 * `?week=N` picks the week (`before` for raids before the release); the default is the current
 * week, or week 1 before the release.
 */
export const load: PageLoad = async ({ parent, url, fetch }) => {
	const { calendar } = await parent();
	try {
		const [raids, characters, effects] = await Promise.all([
			api.get<Raid[]>('/api/raids', { fetch }),
			api.get<GuildCharacter[]>('/api/characters', { fetch }),
			api.get<Effect[]>('/api/raids/effects', { fetch })
		]);
		const param = url.searchParams.get('week');
		const week: number | null =
			param === 'before' ? null : Number(param) || (calendar.current_week?.number ?? 1);
		const shown = raids
			.filter((r) => (week === null ? r.week === null : r.week?.number === week))
			.sort((a, b) => a.starts_at.localeCompare(b.starts_at));
		const details = await Promise.all(
			shown.map((r) => api.get<RaidDetail>(`/api/raids/${r.id}`, { fetch }))
		);
		// The weeks to offer: through the current one, the latest with a raid, and the next.
		const latest = Math.max(
			1,
			calendar.current_week?.number ?? 1,
			...raids.map((r) => r.week?.number ?? 0)
		);
		return {
			week,
			weeks: Array.from({ length: latest + 1 }, (_, i) => i + 1),
			hasBefore: raids.some((r) => r.week === null),
			raids: details,
			characters,
			effects
		};
	} catch (err) {
		error(statusFrom(err, 503), 'The raid planner is unavailable right now.');
	}
};
