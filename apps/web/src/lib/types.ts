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
