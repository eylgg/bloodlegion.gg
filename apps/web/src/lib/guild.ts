import { resolve } from '$app/paths';
import type { Faction, Quality, Rank, Ruleset, User, Zone } from '$lib/types';

/**
 * A character's page: `/roster/garr-bonecrush`, its name lowercase. Names are unique and letters
 * only, so the hyphen is never part of one.
 */
export const characterSlug = (firstName: string, lastName: string) =>
	`${firstName}-${lastName}`.toLowerCase();

export const characterPath = (firstName: string, lastName: string) =>
	resolve('/(guild)/roster/[name]', { name: characterSlug(firstName, lastName) });

/** A player's page: `/@eyl`, the username lowercase, as it is unique. */
/** The character's armory page at Blizzard: its name lowercase, like its page here. */
export const armoryUrl = (firstName: string, lastName: string) =>
	`https://worldofwarcraft.blizzard.com/en-us/forever/us/armory/character/${encodeURIComponent(
		characterSlug(firstName, lastName)
	)}`;

export const playerPath = (username: string) =>
	resolve('/(guild)/@[username]', { username: username.toLowerCase() });

/** Highest first, as the backend orders them. */
export const RULESETS: { ruleset: Ruleset; label: string }[] = [
	{ ruleset: 'normal', label: 'Normal' },
	{ ruleset: 'pvp', label: 'PvP' },
	{ ruleset: 'roleplay', label: 'Roleplay' },
	{ ruleset: 'roleplay-pvp', label: 'RP-PvP' },
	{ ruleset: 'hardcore', label: 'Hardcore' }
];

export const FACTION_LABEL: Record<Faction, string> = { alliance: 'Alliance', horde: 'Horde' };

export const rulesetLabel = (ruleset: Ruleset) =>
	RULESETS.find((r) => r.ruleset === ruleset)?.label ?? ruleset;

export const RANKS: Rank[] = ['leader', 'officer', 'raider', 'trial', 'member', 'public'];

export const RANK_LABEL: Record<Rank, string> = {
	leader: 'Leader',
	officer: 'Officer',
	raider: 'Raider',
	trial: 'Trial',
	member: 'Member',
	public: 'Public'
};

/** A rank's heading over its members, as the roster groups them. */
export const RANK_GROUP: Record<Rank, string> = {
	leader: 'Leaders',
	officer: 'Officers',
	raider: 'Raiders',
	trial: 'Trials',
	member: 'Members',
	public: 'Public'
};

/** The raiding roster: leaders through trials. */
export const RAIDING_RANKS: Rank[] = ['leader', 'officer', 'raider', 'trial'];

/**
 * Mirrors `User::is_guild_member` on the backend: who sees the guild's pages. Anyone can sign in,
 * so a new account is public until an officer gives it a rank.
 */
export function isGuildMember(user: User): boolean {
	return user.is_superuser || user.guild_rank !== 'public';
}

/** Mirrors `User::is_officer` on the backend: who runs raids and records loot. */
export function isOfficer(user: User): boolean {
	return user.is_superuser || user.guild_rank === 'leader' || user.guild_rank === 'officer';
}

/**
 * Mirrors `may_set_rank` on the backend, for whether `user` gets a rank picker for an account at
 * `rank`: officers set ranks, only the leader makes or unmakes officers, and only a superuser makes
 * or unmakes a leader or sets their own.
 */
export function canSetRank(user: User, id: number, rank: Rank): boolean {
	if (user.is_superuser) return true;
	if (!isOfficer(user) || id === user.id || rank === 'leader') return false;
	return user.guild_rank === 'leader' || rank !== 'officer';
}

/** The ranks `user` may give, by the same rules. */
export function rankChoices(user: User): Rank[] {
	if (user.is_superuser) return RANKS;
	return RANKS.filter((r) => r !== 'leader' && (user.guild_rank === 'leader' || r !== 'officer'));
}

/** Highest first, as the backend accepts them. */
export const QUALITIES: Quality[] = ['legendary', 'epic', 'rare', 'uncommon', 'common', 'poor'];

export const QUALITY_LABEL: Record<Quality, string> = {
	poor: 'Poor',
	common: 'Common',
	uncommon: 'Uncommon',
	rare: 'Rare',
	epic: 'Epic',
	legendary: 'Legendary'
};

/** The game's own item quality colors. */
export const QUALITY_COLOR: Record<Quality, string> = {
	poor: '#9d9d9d',
	common: '#ffffff',
	uncommon: '#1eff00',
	rare: '#0070dd',
	epic: '#a335ee',
	legendary: '#ff8000'
};

export function zoneName(zones: Zone[], slug: string): string {
	return zones.find((z) => z.slug === slug)?.name ?? slug;
}

export function fullName(character: { first_name: string | null; last_name: string | null }) {
	return [character.first_name, character.last_name].filter(Boolean).join(' ');
}

const dateFormat = new Intl.DateTimeFormat(undefined, {
	weekday: 'short',
	month: 'short',
	day: 'numeric',
	year: 'numeric'
});
const dateTimeFormat = new Intl.DateTimeFormat(undefined, {
	weekday: 'short',
	month: 'short',
	day: 'numeric',
	year: 'numeric',
	hour: 'numeric',
	minute: '2-digit'
});

/** A day, in the reader's own locale and zone. */
export const formatDate = (iso: string) => dateFormat.format(new Date(iso));
export const formatDateTime = (iso: string) => dateTimeFormat.format(new Date(iso));

/**
 * Today in `timeZone`, at `clock` (`20:00`), as a `datetime-local` value: the default start for a
 * new raid, on the guild's clock rather than the reader's.
 */
export function todayAt(clock: string, timeZone: string): string {
	const day = new Intl.DateTimeFormat('en-CA', {
		timeZone,
		year: 'numeric',
		month: '2-digit',
		day: '2-digit'
	}).format(new Date());
	return `${day}T${clock}`;
}

/** A zone's city, for prose: `America/New_York` is `New York`. */
export const zoneCity = (timeZone: string) =>
	timeZone.split('/').pop()?.replace(/_/g, ' ') ?? timeZone;

/** A day as a clock in `timeZone` reads it. */
export function formatDateInZone(iso: string, timeZone: string): string {
	return new Intl.DateTimeFormat(undefined, {
		weekday: 'short',
		month: 'short',
		day: 'numeric',
		year: 'numeric',
		timeZone
	}).format(new Date(iso));
}

/** An instant as a clock in `timeZone` reads it, with the zone's abbreviation (`6:00 PM EST`). */
export function formatInZone(iso: string, timeZone: string): string {
	return new Intl.DateTimeFormat(undefined, {
		weekday: 'short',
		month: 'short',
		day: 'numeric',
		hour: 'numeric',
		minute: '2-digit',
		timeZone,
		timeZoneName: 'short'
	}).format(new Date(iso));
}

const spanFormat = new Intl.DateTimeFormat(undefined, { month: 'short', day: 'numeric' });

/** A raiding week's days, `Dec 15 – Dec 22`, in the reader's zone. */
export function formatWeekSpan(week: { starts_at: string; ends_at: string }): string {
	return spanFormat.formatRange(new Date(week.starts_at), new Date(week.ends_at));
}

/**
 * Raids that start within this many hours of each other overlap, so a player is in one of them.
 * Mirrors the backend's `raids::RAID_LENGTH`, which enforces it.
 */
export const RAID_LENGTH_HOURS = 3;

const weekdayFormats = {
	long: new Intl.DateTimeFormat(undefined, { weekday: 'long', timeZone: 'UTC' }),
	short: new Intl.DateTimeFormat(undefined, { weekday: 'short', timeZone: 'UTC' })
};
const clockFormat = new Intl.DateTimeFormat(undefined, {
	hour: 'numeric',
	minute: '2-digit',
	timeZone: 'UTC'
});

/**
 * Each raid's name: its zone and the day it starts on the guild's clock ("Onyxia's Lair
 * Tuesday"), with the time when the zone has another raid that day in the same raid week
 * ("Molten Core Tuesday 8:00 PM"), and numbered in order of scheduling when that is the same too
 * (two raids should never start at once, but should they). The planner, short of room, says the day short ("Tue").
 */
export function raidNames(
	zones: Zone[],
	raids: {
		id: number;
		zone: string;
		starts_local: string;
		starts_at: string;
		week: { number: number } | null;
	}[],
	weekday: 'long' | 'short' = 'long'
): Map<number, string> {
	const names = new Map<number, string>();
	const week = (r: (typeof raids)[number]) => `${r.zone}:${r.week?.number ?? 'before'}`;
	// `starts_local` is the guild's wall clock (`2026-12-09T20:00`): read as UTC, it formats as is.
	const local = (r: (typeof raids)[number]) => new Date(`${r.starts_local.slice(0, 16)}Z`);
	const day = (r: (typeof raids)[number]) => `${week(r)}:${r.starts_local.slice(0, 10)}`;
	const time = (r: (typeof raids)[number]) => `${week(r)}:${r.starts_local.slice(0, 16)}`;
	const sorted = [...raids].sort((a, b) => a.starts_at.localeCompare(b.starts_at) || a.id - b.id);
	for (const raid of sorted) {
		let name = `${zoneName(zones, raid.zone)} ${weekdayFormats[weekday].format(local(raid))}`;
		if (sorted.filter((r) => day(r) === day(raid)).length > 1) {
			name += ` ${clockFormat.format(local(raid))}`;
			const same = sorted.filter((r) => time(r) === time(raid));
			if (same.length > 1) name += ` ${same.indexOf(raid) + 1}`;
		}
		names.set(raid.id, name);
	}
	return names;
}
