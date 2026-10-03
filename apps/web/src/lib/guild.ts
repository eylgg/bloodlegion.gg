import type { Quality, Rank, User, Zone } from '$lib/types';

/** Highest first, as the backend orders them. */
export const RANKS: Rank[] = [
	'leader',
	'officer',
	'raider',
	'trial',
	'member',
	'friend',
	'retired'
];

export const RANK_LABEL: Record<Rank, string> = {
	leader: 'Leader',
	officer: 'Officer',
	raider: 'Raider',
	trial: 'Trial',
	member: 'Member',
	friend: 'Friend',
	retired: 'Retired'
};

/** The raiding roster: leaders through trials. */
export const RAIDING_RANKS: Rank[] = ['leader', 'officer', 'raider', 'trial'];

/** Mirrors `User::is_officer` on the backend: who runs raids and records loot. */
export function isOfficer(user: User): boolean {
	return user.is_superuser || user.guild_rank === 'leader' || user.guild_rank === 'officer';
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
