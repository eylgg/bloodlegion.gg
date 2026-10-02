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

/** An instant as a `datetime-local` input wants it: the reader's local time, to the minute. */
export function toLocalInput(iso: string): string {
	const date = new Date(iso);
	const pad = (n: number) => String(n).padStart(2, '0');
	return (
		`${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}` +
		`T${pad(date.getHours())}:${pad(date.getMinutes())}`
	);
}

/** A `datetime-local` value (the reader's local time) as the instant the API takes. */
export const fromLocalInput = (value: string) => new Date(value).toISOString();
