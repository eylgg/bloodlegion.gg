import { api } from '$lib/api';
import { RAID_LENGTH_HOURS } from '$lib/guild';
import type { Attendee, GuildCharacter, RaidDetail } from '$lib/types';

export type Target = { group: number; slot: number };

/** The first free place in `group`, else anywhere in the raid; null when it is full. */
export function freeSlot(raid: RaidDetail, size: number, group: number): Target | null {
	const taken = (g: number, s: number) =>
		raid.attendees.some((a) => a.group_number === g && a.slot === s);
	const groups = Array.from({ length: Math.floor(size / 5) }, (_, i) => i + 1);
	for (const g of [group, ...groups.filter((g) => g !== group)]) {
		for (let s = 1; s <= 5; s++) if (!taken(g, s)) return { group: g, slot: s };
	}
	return null;
}

const overlaps = (a: string, b: string) =>
	Math.abs(Date.parse(a) - Date.parse(b)) < RAID_LENGTH_HOURS * 3600 * 1000;

/**
 * Where putting `character` on `raid` clashes with the other raids here: their player is on one
 * at the same time (whichever character), or the character is saved to the zone that week.
 * Mirrors the backend's rules, which refuse either.
 */
export function clashes(raids: RaidDetail[], raid: RaidDetail, character: GuildCharacter) {
	const samePlayer = (a: Attendee) =>
		a.character_id === character.id ||
		(character.user_id !== null && a.user_id === character.user_id);
	return raids.flatMap((other) => {
		if (other.raid.id === raid.raid.id) return [];
		const atOnce = overlaps(other.raid.starts_at, raid.raid.starts_at);
		const saved =
			other.raid.zone === raid.raid.zone &&
			raid.raid.week !== null &&
			other.raid.week?.number === raid.raid.week.number;
		return other.attendees
			.filter((a) => (atOnce && samePlayer(a)) || (saved && a.character_id === character.id))
			.map((attendee) => ({ other, attendee, atOnce }));
	});
}

const withAttendees = (raids: RaidDetail[], raidId: number, attendees: Attendee[]) =>
	raids.map((r) =>
		r.raid.id === raidId
			? { ...r, raid: { ...r.raid, attendee_count: attendees.length }, attendees }
			: r
	);

/** Takes a character off a raid; the raids as they are after. */
export async function takeOff(raids: RaidDetail[], raidId: number, characterId: number) {
	await api.del(`/api/raids/${raidId}/attendees/${characterId}`);
	const raid = raids.find((r) => r.raid.id === raidId);
	return withAttendees(
		raids,
		raidId,
		(raid?.attendees ?? []).filter((a) => a.character_id !== characterId)
	);
}

/**
 * Puts `character` on `raid` at `target` (or the nearest free place, when it is taken), moving
 * them out of any raid here they clash with once `confirm` agrees. The raids as they are after,
 * or null when it was called off. Throws the API's error.
 */
export async function putOn(
	raids: RaidDetail[],
	raid: RaidDetail,
	character: GuildCharacter,
	target: Target,
	size: number,
	confirm: (message: string) => boolean,
	nameOf: (raidId: number) => string
): Promise<RaidDetail[] | null> {
	const taken = raid.attendees.some(
		(a) => a.group_number === target.group && a.slot === target.slot
	);
	const place = taken ? freeSlot(raid, size, target.group) : target;
	if (!place) throw new Error(`${nameOf(raid.raid.id)} is full.`);
	const found = clashes(raids, raid, character);
	if (found.length > 0) {
		const fullName = (c: { first_name: string; last_name: string }) =>
			`${c.first_name} ${c.last_name}`;
		const player = character.username ?? fullName(character);
		const reasons = found.map(({ other, attendee, atOnce }) =>
			atOnce
				? `${player} is in ${nameOf(other.raid.id)} at the same time (on ${fullName(attendee)}).`
				: `${fullName(attendee)} is already saved to ${nameOf(other.raid.id)} this week.`
		);
		if (!confirm(`${reasons.join(' ')} Move them here?`)) return null;
	}
	let next = raids;
	for (const { other, attendee } of found) {
		next = await takeOff(next, other.raid.id, attendee.character_id);
	}
	const attendees = await api.put<Attendee[]>(
		`/api/raids/${raid.raid.id}/attendees/${character.id}`,
		{ group_number: place.group, slot: place.slot, spec: null }
	);
	return withAttendees(next, raid.raid.id, attendees);
}
