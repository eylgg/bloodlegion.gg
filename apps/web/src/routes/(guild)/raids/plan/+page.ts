import { error } from '@sveltejs/kit';
import type { PageLoad } from './$types';
import type { Effect, GuildCharacter, Raid, RaidDetail, WeekPlan } from '$lib/types';
import { api, statusFrom } from '$lib/api';
import { todayAt } from '$lib/guild';

/**
 * The raid planner: one raid week's raids side by side, with the roster to fill them from.
 * `?week=N` picks the week (`before` for raids before the release); the default is the current
 * week, or week 1 before the release.
 */
export const load: PageLoad = async ({ parent, url, fetch }) => {
	const { calendar, settings } = await parent();
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
			// The order raids are numbered in (see `raidNames`): by start, then as scheduled.
			.sort((a, b) => a.starts_at.localeCompare(b.starts_at) || a.id - b.id);
		const [details, plan] = await Promise.all([
			Promise.all(shown.map((r) => api.get<RaidDetail>(`/api/raids/${r.id}`, { fetch }))),
			week === null ? null : api.get<WeekPlan>(`/api/raids/weeks/${week}`, { fetch })
		]);
		// When a raid added this week starts unless said otherwise: the week's first evening at the
		// default raid time (before the release, today's).
		const defaultStart =
			plan?.default_start_local ?? todayAt(settings.default_raid_time, settings.time_zone);
		// The weeks to offer: through the current one, the latest with a raid, and the next.
		const latest = Math.max(
			1,
			calendar.current_week?.number ?? 1,
			...raids.map((r) => r.week?.number ?? 0)
		);
		return {
			week,
			span: plan?.week ?? null,
			defaultStart,
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
