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
