export type User = {
	id: number;
	username: string;
	is_superuser: boolean;
	disabled: boolean;
	/** The primary email, or null: a Battle.net account has none. */
	email: string | null;
	is_email_verified: boolean;
	is_email_federated: boolean;
	first_name: string | null;
	last_name: string | null;
	guild_rank: Rank;
	created_at: string;
	updated_at: string;
};

export type LoginProvider = { kind: 'local' } | { kind: 'oauth2'; slug: string; name: string };

export type LoginOptions = {
	providers: LoginProvider[];
};

export type ProviderConnection = {
	kind: 'oauth2';
	slug: string;
	name: string;
	connected: boolean;
};

export type AuthMethods = {
	local_enabled: boolean;
	password_set: boolean;
	providers: ProviderConnection[];
};

export type SessionInfo = {
	id: number;
	created_at: string;
	last_used_at: string;
	ip_address: string;
	user_agent: string | null;
	is_current: boolean;
};

/** A first sign-in through a provider that asserted no usable username: the person picks one. */
export type Registration = {
	provider_slug: string;
	provider_name: string;
	/** How the provider identifies the person (e.g. a BattleTag), for the page to say who it is. */
	identity: string | null;
	/** A username to prefill, derived from `identity`, or null when nothing usable was asserted. */
	suggestion: string | null;
};

/** A spec's place in a raid: the in-game tank and healer, with damage split by range. */
export type Role = 'tank' | 'healer' | 'melee' | 'ranged';

export type Spec = { slug: string; name: string; roles: Role[] };

/** A WoW: Forever class, from `GET /api/launch/classes`; `color` is the official class color. */
export type WowClass = { slug: string; name: string; color: string; specs: Spec[] };

/** A character reserved for launch: a sign-up, not a real character. */
export type Character = {
	id: number;
	class: string;
	specs: string[];
	is_main: boolean;
	created_at: string;
	updated_at: string;
};

export type CharacterInput = {
	class: string;
	specs: string[];
	is_main: boolean;
};

/** One member's sign-up, as the guild-wide list shows it. */
export type RosterEntry = {
	username: string;
	class: string;
	specs: string[];
	is_main: boolean;
};

/* --- the guild --- */

/** A guild rank, highest first (see `RANKS`). Leaders and officers run the guild's records. */
export type Rank = 'leader' | 'officer' | 'raider' | 'trial' | 'member' | 'friend' | 'retired';

/** One account at a provider linked to the signed-in member, from `GET /api/auth/linked-accounts`. */
export type LinkedAccount = {
	id: number;
	provider_slug: string;
	provider_name: string;
	/** How the provider names the account (a BattleTag), when it said. */
	identity: string | null;
	can_unlink: boolean;
	linked_at: string;
};

/** A WoW: Forever character: a first and a last name, no realm. */
export type GuildCharacter = {
	id: number;
	/** The member who plays it; null for an unclaimed character (a pug). */
	user_id: number | null;
	username: string | null;
	first_name: string;
	last_name: string;
	class: string;
	is_main: boolean;
	created_at: string;
	updated_at: string;
};

export type GuildCharacterInput = {
	first_name: string;
	last_name: string;
	class: string;
	is_main: boolean;
	/** Officers only: omit for yourself, a member's id, or null for an unclaimed character. */
	user_id?: number | null;
};

export type Member = {
	id: number;
	username: string;
	guild_rank: Rank;
	characters: GuildCharacter[];
};

export type Note = { character_id: number; body: string; updated_at: string };

export type NoteListing = {
	character_id: number;
	first_name: string;
	last_name: string;
	class: string;
	username: string;
	body: string;
	updated_at: string;
};

export type CharacterDetail = {
	character: GuildCharacter;
	raids_attended: number;
	loot: LootEntry[];
	note: Note | null;
	can_edit: boolean;
	can_write_note: boolean;
};

/* --- raids and loot --- */

export type Zone = { slug: string; name: string; size: number };

export type Boss = { id: number; zone: string; name: string };

export type Quality = 'poor' | 'common' | 'uncommon' | 'rare' | 'epic' | 'legendary';

export type Item = {
	id: number;
	name: string;
	quality: Quality;
	game_item_id: number | null;
	/** How many times it has been won. */
	drops: number;
};

export type Raid = {
	id: number;
	zone: string;
	title: string | null;
	starts_at: string;
	attendee_count: number;
	loot_count: number;
};

export type Attendee = {
	character_id: number;
	user_id: number | null;
	username: string | null;
	first_name: string;
	last_name: string;
	class: string;
	is_main: boolean;
};

/** One item won. No boss for trash; no character when nobody took it (disenchanted, banked). */
export type LootEntry = {
	id: number;
	raid_id: number;
	zone: string;
	raid_title: string | null;
	raid_starts_at: string;
	boss_id: number | null;
	boss_name: string | null;
	item_id: number;
	item_name: string;
	item_quality: Quality;
	game_item_id: number | null;
	character_id: number | null;
	first_name: string | null;
	last_name: string | null;
	class: string | null;
};

export type RaidDetail = { raid: Raid; attendees: Attendee[]; loot: LootEntry[] };

export type BossDetail = {
	boss: Boss;
	kills: number;
	drops: {
		item_id: number;
		item_name: string;
		item_quality: Quality;
		game_item_id: number | null;
		count: number;
	}[];
	loot: LootEntry[];
};

export type ItemDetail = { item: Item; loot: LootEntry[] };

export type Question = {
	id: number;
	title: string;
	body: string;
	yes: number;
	no: number;
	/** The signed-in member's own answer. */
	answer: boolean | null;
	created_at: string;
};

export type QuestionDetail = {
	question: Question;
	/** Who said what; officers only. */
	answers: { user_id: number; username: string; choice: boolean; updated_at: string }[] | null;
};
