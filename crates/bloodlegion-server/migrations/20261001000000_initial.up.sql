/*
# Blood Legion

This is the initial migration, derived from CeresForge's with SAML and its integrations removed. Conventions:

- One clause per line
- Lines wrap at 100 columns

- Upper bounds use `<= N`
- Non-empty text uses `char_length(x) > 0` as its lower bound, and nullable text wraps as `x IS NULL
  OR (char_length(x) > 0 AND char_length(x) <= N)`
- A slug is `char_length >= 3 AND <= 64 AND ~ '^[a-z][a-z0-9-]*$'`
- A raw 32-byte value in base64url-unpadded form is `char_length = 43 AND ~ '^[A-Za-z0-9_-]+$'`
  (token/code/secret hashes, state, nonce, code_verifier, code_challenge, kid), except password_hash
  is an Argon2id PHC string ($argon2id$v=19$m=,t=,p=$salt$hash) with a `> 0 AND <= 256` bound that
  caps the encoded salt/hash sizes
- Secrets at rest take the `_ciphertext` suffix and a `> 0 AND <= 1024` bound (8192 for key
  material)
- jsonb columns carry `CHECK (x IS JSON OBJECT)` or `CHECK (x IS JSON ARRAY)`

- The default primary key is `id bigint GENERATED ALWAYS AS IDENTITY`, except a 1:1 extension table
  keys on its parent FK, a natural-key table (client_id, *_hash, kid, composite throttle key) uses
  that key, and a singleton settings table keys on a boolean sentinel
- All foreign keys are `ON DELETE CASCADE`
- Name CHECKS: <table>_<col>_check for single-column checks, and <table>_<purpose>_check otherwise
  or where the column form would exceed 63 bytes (canvas_settings_singleton_check,
  ..._method_s256_check)
- btree is the default, so omit `USING btree`
- Name UNIQUE constraints `<table>_<col(s)>_key`, columns in definition order
- All indexes use the `_idx` suffix; `_key` is the only name that marks a UNIQUE constraint

- Timestamps are always `timestamp with time zone`, taking `_at` for events and `_until` for future
  bounds
- A `BEFORE UPDATE WHEN (OLD.* IS DISTINCT FROM NEW.*) EXECUTE tf_set_updated_at()` trigger
  maintains updated_at, and a table that omits it is insert-then-consume, append-only, or a
  wholly-rewritten cache

- A trigger function starts with `tf_`; a trigger ends with `_trig` and starts with the table it
  fires ON
- Triggers and their functions share a <purpose> slot (the same one as `<table>_<purpose>_check`);
  so a trigger is `<table>_<purpose>_trig` and its function `tf_<table>_<purpose>`
- <purpose> is a verb phrase for the invariant
- Shared generic functions (tf_set_updated_at, tf_prevent_deletion, tf_set_once_*) omit the
  <table> segment, since many triggers reuse them
- ensure_<child> / prevent_<child>_orphan are deferred constraint-trigger pairs enforcing "at least
  one child"; prevent_<child>_orphan takes FOR UPDATE on the parent to serialize concurrent removers
- <discriminator>_consistent / <discriminator>_required / <discriminator>_immutable are immediate
  triggers gating on a parent column
- <col>_set_once guards that an audit column (completion/consumption/revocation/deactivation
  timestamps, and the code's created_family_id) never changes once set: not cleared, not rewritten,
  so the audit fact stays honest
*/

/*
## Shared functions
*/

CREATE FUNCTION tf_set_updated_at()
    RETURNS trigger
    LANGUAGE plpgsql
AS $$
BEGIN
    NEW.updated_at := now();
    RETURN NEW;
END;
$$;

-- Refuses to delete rows from a singleton table.
CREATE FUNCTION tf_prevent_deletion()
    RETURNS trigger
    LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'rows in % cannot be deleted', TG_TABLE_NAME;
END;
$$;

-- The set-once guards below protect audit columns: once set, the value can never change (not
-- cleared, not rewritten). The four timestamps here plus the authorization code's created_family_id
-- (its trigger lives at that table). Each trigger fires only when a caller alters an already-set
-- value (the WHEN clause filters the violation), so the body only raises.
CREATE FUNCTION tf_set_once_completed_at()
    RETURNS trigger
    LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION '% completed_at is set once; cannot be altered once set', TG_TABLE_NAME
        USING ERRCODE = 'check_violation';
END;
$$;

CREATE FUNCTION tf_set_once_consumed_at()
    RETURNS trigger
    LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION '% consumed_at is set once; cannot be altered once set', TG_TABLE_NAME
        USING ERRCODE = 'check_violation';
END;
$$;

CREATE FUNCTION tf_set_once_revoked_at()
    RETURNS trigger
    LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION '% revoked_at is set once; cannot be altered once set', TG_TABLE_NAME
        USING ERRCODE = 'check_violation';
END;
$$;

CREATE FUNCTION tf_set_once_deactivated_at()
    RETURNS trigger
    LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION '% deactivated_at is set once; cannot be altered once set', TG_TABLE_NAME
        USING ERRCODE = 'check_violation';
END;
$$;

CREATE FUNCTION tf_set_once_created_family_id()
    RETURNS trigger
    LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION '% created_family_id is set once; cannot be altered once set', TG_TABLE_NAME
        USING ERRCODE = 'check_violation';
END;
$$;

/*
## Users and emails
*/

CREATE TABLE users (
    id bigint
        PRIMARY KEY
        GENERATED ALWAYS AS IDENTITY,
    -- The name exactly as the person chose it, capitals included, for display. Sign-in, lookups,
    -- and uniqueness use username_normalized.
    username text
        NOT NULL
        CONSTRAINT users_username_check
        CHECK (
            char_length(username) >= 3
            AND char_length(username) <= 32
            AND username ~ '^[A-Za-z][A-Za-z0-9]*$'
        ),
    -- Lowercase form, generated from username; carries uniqueness and powers case-insensitive
    -- lookups via plain equality (the same split user_emails makes).
    username_normalized text
        NOT NULL
        -- STORED is load-bearing: PG18 defaults generated columns to VIRTUAL, which can't be
        -- indexed; the uniqueness/lookup constraint needs it persisted.
        GENERATED ALWAYS AS (lower(username)) STORED,
    is_superuser boolean NOT NULL DEFAULT false,
    first_name text
        CONSTRAINT users_first_name_check
        CHECK (
            first_name IS NULL
            OR (char_length(first_name) > 0 AND char_length(first_name) <= 64)
        ),
    last_name text
        CONSTRAINT users_last_name_check
        CHECK (
            last_name IS NULL
            OR (char_length(last_name) > 0 AND char_length(last_name) <= 64)
        ),
    disabled_at timestamp with time zone,
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    updated_at timestamp with time zone NOT NULL DEFAULT now(),

    CONSTRAINT users_username_normalized_key UNIQUE (username_normalized)
);

-- Gated on the explicit non-generated columns (a BEFORE trigger's WHEN cannot reference NEW
-- generated columns, and NEW.* would include the generated username_normalized).
CREATE TRIGGER users_updated_at_trig
    BEFORE UPDATE ON users
    FOR EACH ROW
    WHEN (
        (OLD.username, OLD.is_superuser, OLD.first_name, OLD.last_name, OLD.disabled_at)
        IS DISTINCT FROM
        (NEW.username, NEW.is_superuser, NEW.first_name, NEW.last_name, NEW.disabled_at)
    )
    EXECUTE FUNCTION tf_set_updated_at();

CREATE TABLE user_emails (
    id bigint
        PRIMARY KEY
        GENERATED ALWAYS AS IDENTITY,
    user_id bigint
        NOT NULL
        REFERENCES users(id) ON DELETE CASCADE,
    -- The address exactly as the user typed it (for display). Printable-ASCII, no space; see the
    -- COMMENT on user_emails_email_check below for why.
    email text
        NOT NULL
        CONSTRAINT user_emails_email_check
        CHECK (
            char_length(email) >= 3
            AND char_length(email) <= 256
            AND email ~ '^[\x21-\x7E]+$'
        ),
    -- Normalized (lowercased) form, generated from email; carries uniqueness and powers
    -- case-insensitive auth lookups via plain equality.
    email_normalized text
        NOT NULL
        -- STORED is load-bearing: PG18 defaults generated columns to VIRTUAL, which can't be
        -- indexed; the uniqueness/lookup indexes need it persisted.
        GENERATED ALWAYS AS (lower(email)) STORED,
    verified_at timestamp with time zone,
    is_primary boolean NOT NULL DEFAULT false,
    -- Asserted by an identity provider (an OIDC login recorded it), so a future email-management
    -- UI must refuse to remove it while federated (the user can still demote it from primary). The
    -- application owns both transitions: disconnecting an identity source legitimately flips it
    -- back to false, so no trigger guards it.
    is_federated boolean NOT NULL DEFAULT false,
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    updated_at timestamp with time zone NOT NULL DEFAULT now(),

    -- A user may not list the same address twice (case-insensitively), verified or not; the
    -- (user_id) prefix covers FK lookups and the cascade, so Postgres needs no separate index.
    CONSTRAINT user_emails_user_id_email_normalized_key
        UNIQUE (user_id, email_normalized)
);

COMMENT ON CONSTRAINT user_emails_email_check ON user_emails IS
    'Printable-ASCII, no space, so lower(email) -> email_normalized folds collation-independently '
    '(no ICU casefolding skew on an identity key). Rejects RFC 6531 (EAI) Unicode addresses by '
    'design.';

COMMENT ON COLUMN user_emails.is_primary IS
    'An account may have no email at all (a Battle.net login supplies none), and a primary email '
    'need not be verified: signup sets is_primary before verification completes. '
    'Paths that act on the address (password reset, notifications) must additionally check '
    'verified_at IS NOT NULL; app code enforces that obligation, not the schema.';

-- Uniqueness applies only to verified emails (an unverified address must not squat the namespace),
-- on the normalized form so case-variants collide.
CREATE UNIQUE INDEX user_emails_email_normalized_idx
    ON user_emails (email_normalized)
    WHERE verified_at IS NOT NULL;

-- At most one primary email per user.
CREATE UNIQUE INDEX user_emails_one_primary_per_user_idx
    ON user_emails (user_id)
    WHERE is_primary;

COMMENT ON INDEX user_emails_one_primary_per_user_idx IS
    'At most one primary email per user. Partial unique indexes are non-deferrable, so swap '
    'primaries in two statements: demote the old (is_primary = false) before promoting the new.';

-- Gated on the explicit non-generated columns (a BEFORE trigger's WHEN cannot reference NEW
-- generated columns, and NEW.* would include the generated email_normalized).
CREATE TRIGGER user_emails_updated_at_trig
    BEFORE UPDATE ON user_emails
    FOR EACH ROW
    WHEN (
        (OLD.user_id, OLD.email, OLD.verified_at, OLD.is_primary, OLD.is_federated)
        IS DISTINCT FROM
        (NEW.user_id, NEW.email, NEW.verified_at, NEW.is_primary, NEW.is_federated)
    )
    EXECUTE FUNCTION tf_set_updated_at();

-- ISOLATION NOTE: prevent_orphan guards are sound only under READ COMMITTED (Postgres default). The
-- FOR UPDATE on the parent serializes concurrent parent-deletes; NOT EXISTS then re-counts siblings
-- on a fresh snapshot that sees all committed sibling deletes from other sessions. Under REPEATABLE
-- READ the snapshot freezes at transaction start, so the re-count misses a sibling delete after
-- that point; two concurrent last-child removers can both commit, leaving zero children. Do not use
-- REPEATABLE READ on transactions that delete children here. To close this at any isolation level,
-- also UPDATE the parent row (e.g. SET updated_at = now()) in the same transaction; that creates a
-- write-write conflict and triggers first-updater-wins at RR and SERIALIZABLE.

-- OWNERSHIP NOTE: every guard here (immutability, set-once, orphan, consistency triggers) is
-- advisory against the table owner, who can ALTER TABLE ... DISABLE TRIGGER or bypass with a
-- superuser session. These invariants therefore hold only while the application connects as a
-- non-owner role with plain DML privileges; a migration/DDL role is a separate connection.

-- RETENTION NOTE: the completed *_requests / authorization-code / credential rows double as a
-- PII-bearing audit ledger with no built-in expiry once completed. ON DELETE CASCADE from users
-- makes account deletion clean, but retention for *non-deleted* users is entirely the app's job
-- (the cleaner sweeps expired/pending rows via the expires_at indexes; it does not sweep completed
-- rows).

/*
## Sessions and local credentials
*/

CREATE TABLE auth_sessions (
    id bigint
        PRIMARY KEY
        GENERATED ALWAYS AS IDENTITY,
    -- Stores only the SHA-256 hash of the session token, never the raw token.
    token_hash text
        NOT NULL
        UNIQUE
        CONSTRAINT auth_sessions_token_hash_check
        CHECK (
            char_length(token_hash) = 43
            AND token_hash ~ '^[A-Za-z0-9_-]+$'
        ),
    user_id bigint
        NOT NULL
        REFERENCES users(id) ON DELETE CASCADE,
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    expires_at timestamp with time zone
        NOT NULL
        DEFAULT (now() + interval '90 days'),
    last_used_at timestamp with time zone NOT NULL DEFAULT now(),
    ip_address inet NOT NULL,
    user_agent text
        CONSTRAINT auth_sessions_user_agent_check
        CHECK (
            user_agent IS NULL
            OR (char_length(user_agent) > 0 AND char_length(user_agent) <= 1024)
        ),
    revoked_at timestamp with time zone
);

CREATE INDEX auth_sessions_user_id_idx
    ON auth_sessions (user_id);

CREATE INDEX auth_sessions_expires_at_idx
    ON auth_sessions (expires_at);

-- token_hash, user_id, and created_at are write-once: session rotation always issues a new row.
CREATE FUNCTION tf_auth_sessions_identity_immutable()
    RETURNS trigger
    LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION
        'auth_sessions: token_hash, user_id, and created_at are immutable'
        USING ERRCODE = 'check_violation';
END;
$$;

CREATE TRIGGER auth_sessions_identity_immutable_trig
    BEFORE UPDATE ON auth_sessions
    FOR EACH ROW
    WHEN (
        OLD.token_hash IS DISTINCT FROM NEW.token_hash
        OR OLD.user_id IS DISTINCT FROM NEW.user_id
        OR OLD.created_at IS DISTINCT FROM NEW.created_at
    )
    EXECUTE FUNCTION tf_auth_sessions_identity_immutable();

CREATE TRIGGER auth_sessions_revoked_at_set_once_trig
    BEFORE UPDATE ON auth_sessions
    FOR EACH ROW
    WHEN (OLD.revoked_at IS NOT NULL AND OLD.revoked_at IS DISTINCT FROM NEW.revoked_at)
    EXECUTE FUNCTION tf_set_once_revoked_at();

CREATE TABLE auth_local_credentials (
    user_id bigint
        PRIMARY KEY
        REFERENCES users(id) ON DELETE CASCADE,
    password_hash text
        NOT NULL
        CONSTRAINT auth_local_credentials_password_hash_check
        CHECK (
            char_length(password_hash) > 0
            AND char_length(password_hash) <= 256
        ),
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    updated_at timestamp with time zone NOT NULL DEFAULT now()
);

CREATE TRIGGER auth_local_credentials_updated_at_trig
    BEFORE UPDATE ON auth_local_credentials
    FOR EACH ROW
    WHEN (OLD.* IS DISTINCT FROM NEW.*)
    EXECUTE FUNCTION tf_set_updated_at();

-- Brute-force throttle. Every counter anchors to a source IP. `username` is NULL for the IP-wide
-- bucket (credential stuffing across accounts from one source) and set for a per-(username, ip)
-- bucket (targeted guessing of one account). There is deliberately NO IP-independent per-username
-- bucket: a global username lock would let any unauthenticated party lock a victim out of local
-- login indefinitely (an availability attack), so a per-account counter also keys on the source IP.
CREATE TABLE auth_local_login_throttles (
    ip_address inet NOT NULL,
    -- NULL = the IP-wide bucket; otherwise the normalized (lowercase) username this counter
    -- tracks. OBLIGATION: the app only keys a per-username bucket after the value parses as a
    -- `Username` and lowercases it (see auth::local::login), so case rotation ("Jon" vs "jon")
    -- can't mint fresh buckets;
    -- an unparseable username records against the IP-wide bucket only. Not a CHECK here because the
    -- app is the single validator (the users.username newtype), matching that boundary.
    username text
        CONSTRAINT auth_local_login_throttles_username_check
        CHECK (
            username IS NULL
            OR (char_length(username) > 0 AND char_length(username) <= 256)
        ),
    failed_count integer
        NOT NULL
        DEFAULT 0
        CONSTRAINT auth_local_login_throttles_failed_count_check
        CHECK (failed_count >= 0),
    first_failed_at timestamp with time zone NOT NULL DEFAULT now(),
    last_failed_at timestamp with time zone NOT NULL DEFAULT now(),
    locked_until timestamp with time zone,

    -- The natural key. NULLS NOT DISTINCT (Postgres 15+) treats the IP-wide (username IS NULL) row
    -- as a single value per IP, so it is one upsertable row rather than unbounded duplicates, and
    -- `ON CONFLICT (ip_address, username)` works for both bucket shapes.
    CONSTRAINT auth_local_login_throttles_key
        UNIQUE NULLS NOT DISTINCT (ip_address, username)
);

CREATE INDEX auth_local_login_throttles_last_failed_at_idx
    ON auth_local_login_throttles (last_failed_at);

/*
## Identity providers (OAuth2 as RP)
*/

-- Shared envelope for every identity provider: the built-in local-login row and each OAuth2/OIDC
-- provider. slug and name are globally unique.
CREATE TABLE auth_providers (
    id bigint
        PRIMARY KEY
        GENERATED ALWAYS AS IDENTITY,
    kind text
        NOT NULL
        CONSTRAINT auth_providers_kind_check
        CHECK (kind IN ('oauth2', 'local')),
    slug text
        NOT NULL
        UNIQUE
        CONSTRAINT auth_providers_slug_check
        CHECK (
            char_length(slug) >= 3
            AND char_length(slug) <= 64
            AND slug ~ '^[a-z][a-z0-9-]*$'
            -- The built-in local-login provider is the sole kind='local' row and owns the slug
            -- `local`: the equivalence locks the two together, so no SAML/OAuth2 provider can claim
            -- that slug and nothing can rename the local row to anything else.
            AND (kind = 'local') = (slug = 'local')
        ),
    name text
        NOT NULL
        UNIQUE
        CONSTRAINT auth_providers_name_check
        CHECK (char_length(name) >= 3 AND char_length(name) <= 64),
    is_registration_allowed boolean NOT NULL DEFAULT false,
    is_auto_connection_allowed boolean NOT NULL DEFAULT false,
    is_email_verified boolean NOT NULL DEFAULT false,
    is_disconnection_allowed boolean NOT NULL DEFAULT false,
    -- When true, a first login from this provider connects to an unclaimed account (no linked
    -- login identity) by matching the asserted username to the account username instead of by
    -- verified email. It trusts the asserted username outright, so enable it only for a provider
    -- whose usernames come from the same authority that provisioned the accounts.
    is_unclaimed_username_connection_allowed boolean NOT NULL DEFAULT false,
    disabled_at timestamp with time zone,
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    updated_at timestamp with time zone NOT NULL DEFAULT now(),

    -- Referenceable composite target for subtype FKs: a body can only attach to a kind-matching
    -- envelope (id alone is unique, but a composite FK needs a unique constraint on exactly these
    -- two columns).
    CONSTRAINT auth_providers_id_kind_key UNIQUE (id, kind)
);

CREATE TRIGGER auth_providers_updated_at_trig
    BEFORE UPDATE ON auth_providers
    FOR EACH ROW
    WHEN (OLD.* IS DISTINCT FROM NEW.*)
    EXECUTE FUNCTION tf_set_updated_at();

-- kind identifies the subtype permanently; morphing a kind would silently invalidate all
-- subtype-specific invariants.
CREATE FUNCTION tf_auth_providers_kind_immutable()
    RETURNS trigger
    LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION
        'auth_providers.kind is immutable (was %, attempted %)', OLD.kind, NEW.kind
        USING ERRCODE = 'check_violation';
END;
$$;

CREATE TRIGGER auth_providers_kind_immutable_trig
    BEFORE UPDATE ON auth_providers
    FOR EACH ROW
    WHEN (OLD.kind IS DISTINCT FROM NEW.kind)
    EXECUTE FUNCTION tf_auth_providers_kind_immutable();

-- Every OAuth2 envelope must have exactly one body in auth_oauth2_providers. Deferred so INSERT
-- INTO auth_providers and INSERT INTO auth_oauth2_providers can happen in the same transaction.
CREATE FUNCTION tf_auth_providers_ensure_oauth2_body()
    RETURNS trigger
    LANGUAGE plpgsql
AS $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM auth_oauth2_providers WHERE provider_id = NEW.id) THEN
        RAISE EXCEPTION
            'auth_providers row % (kind=oauth2) has no auth_oauth2_providers body', NEW.id
            USING ERRCODE = 'foreign_key_violation';
    END IF;
    RETURN NULL;
END;
$$;

CREATE CONSTRAINT TRIGGER auth_providers_ensure_oauth2_body_trig
    AFTER INSERT ON auth_providers
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW
    WHEN (NEW.kind = 'oauth2')
    EXECUTE FUNCTION tf_auth_providers_ensure_oauth2_body();

-- The built-in local-login provider. Unlike OAuth2 it has no subtype body (the ensure_oauth2_body
-- trigger above fires only for its own kind), and its per-user credentials live in
-- auth_local_credentials keyed on the user, not here. This row carries only the shared envelope:
-- disabled_at toggles local login on/off, and the four is_* flags gate its (future) self-service
-- registration and disconnection. It starts enabled with every flag off.
INSERT INTO auth_providers (kind, slug, name) VALUES ('local', 'local', 'Local');

-- The local provider is a permanent singleton: toggle it via disabled_at, never delete it. Removing
-- it would strand local login (discovery and the login handler read this row). OAuth2 rows
-- stay freely deletable; the WHEN pins the guard to the local row only.
CREATE FUNCTION tf_auth_providers_prevent_local_delete()
    RETURNS trigger
    LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'the built-in local provider cannot be deleted';
END;
$$;

CREATE TRIGGER auth_providers_prevent_local_delete_trig
    BEFORE DELETE ON auth_providers
    FOR EACH ROW
    WHEN (OLD.kind = 'local')
    EXECUTE FUNCTION tf_auth_providers_prevent_local_delete();

/*
## OAuth2 (Blood Legion as IdP)
*/

CREATE TABLE auth_oauth2_clients (
    id text
        PRIMARY KEY
        CONSTRAINT auth_oauth2_clients_id_check
        CHECK (char_length(id) = 32 AND id ~ '^[A-Za-z0-9_-]+$'),
    -- NULL for a public client (PKCE only); a 43-char SHA-256 hash otherwise.
    secret_hash text
        CONSTRAINT auth_oauth2_clients_secret_hash_check
        CHECK (
            secret_hash IS NULL
            OR (
                char_length(secret_hash) = 43
                AND secret_hash ~ '^[A-Za-z0-9_-]+$'
            )
        ),
    name text
        NOT NULL
        CONSTRAINT auth_oauth2_clients_name_check
        CHECK (char_length(name) >= 3 AND char_length(name) <= 64),
    client_type text
        NOT NULL
        DEFAULT 'confidential'
        CONSTRAINT auth_oauth2_clients_client_type_check
        CHECK (client_type IN ('confidential', 'public')),
    is_pkce_required boolean NOT NULL DEFAULT true,
    -- When true, OIDC userinfo presents `<username>@<origin host>` as the email for this client,
    -- not the user's account email. This suits clients that require an in-domain address, for
    -- example Tailscale. The value is a synthetic stand-in, so email_verified is false, except when
    -- the user's verified primary email equals it (then true). The username keeps each user
    -- distinct, so the masking does not collapse identities.
    is_email_masked boolean NOT NULL DEFAULT false,
    -- Scopes pre-authorized for this client: a request whose scopes all fall within this set skips
    -- the consent screen (a first-party client the operator controls). Empty (the default) always
    -- prompts. Same `scope` grammar and 512-char bound as the consent/grant columns.
    auto_consent_scope text
        NOT NULL
        DEFAULT ''
        CONSTRAINT auth_oauth2_clients_auto_consent_scope_check
        CHECK (
            auto_consent_scope ~ '^([\x21\x23-\x5B\x5D-\x7E]+( [\x21\x23-\x5B\x5D-\x7E]+)*)?$'
            AND char_length(auto_consent_scope) <= 512
        ),
    -- Federates this client to a single external provider (by its immutable auth_providers.id).
    -- NULL (the default) means unrestricted: any signed-in user, identity from their CeresForge
    -- account. When set, `authorize` admits only users with a live credential at that provider and
    -- presents that provider's profile identity (username/email) to the client. FK with ON DELETE
    -- RESTRICT so deleting a provider still backing a client fails (the dependency surfaces as an
    -- error rather than silently opening the gate); the id is immutable, so no rename can break the
    -- link. The admin API works in slugs and resolves to this id on write. No index on this FK:
    -- provider deletes are rare and the clients table is admin-scale, so the RESTRICT seq scan is
    -- negligible.
    identity_provider_id bigint
        REFERENCES auth_providers(id) ON DELETE RESTRICT,
    disabled_at timestamp with time zone,
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    updated_at timestamp with time zone NOT NULL DEFAULT now(),

    -- public <-> no secret; confidential <-> has a secret.
    CONSTRAINT auth_oauth2_clients_secret_shape_check
    CHECK (
        (client_type = 'public' AND secret_hash IS NULL)
        OR (client_type = 'confidential' AND secret_hash IS NOT NULL)
    ),
    -- a public client always requires PKCE.
    CONSTRAINT auth_oauth2_clients_public_is_pkce_required_check
    CHECK (client_type <> 'public' OR is_pkce_required)
);

CREATE TRIGGER auth_oauth2_clients_updated_at_trig
    BEFORE UPDATE ON auth_oauth2_clients
    FOR EACH ROW
    WHEN (OLD.* IS DISTINCT FROM NEW.*)
    EXECUTE FUNCTION tf_set_updated_at();

CREATE TABLE auth_oauth2_client_redirect_uris (
    id bigint
        PRIMARY KEY
        GENERATED ALWAYS AS IDENTITY,
    client_id text
        NOT NULL
        REFERENCES auth_oauth2_clients(id) ON DELETE CASCADE,
    uri text
        NOT NULL
        CONSTRAINT auth_oauth2_client_redirect_uris_uri_check
        CHECK (char_length(uri) > 0 AND char_length(uri) <= 512),
    created_at timestamp with time zone NOT NULL DEFAULT now(),

    CONSTRAINT auth_oauth2_client_redirect_uris_client_id_uri_key
        UNIQUE (client_id, uri)
);

CREATE TABLE auth_oauth2_client_authorization_codes (
    -- Stores only the SHA-256 hash; the raw code travels in the redirect to the client.
    code_hash text
        PRIMARY KEY
        CONSTRAINT auth_oauth2_client_authorization_codes_code_hash_check
        CHECK (char_length(code_hash) = 43 AND code_hash ~ '^[A-Za-z0-9_-]+$'),
    client_id text
        NOT NULL
        REFERENCES auth_oauth2_clients(id) ON DELETE CASCADE,
    user_id bigint
        NOT NULL
        REFERENCES users(id) ON DELETE CASCADE,
    redirect_uri text
        NOT NULL
        CONSTRAINT auth_oauth2_client_authorization_codes_redirect_uri_check
        CHECK (
            char_length(redirect_uri) > 0
            AND char_length(redirect_uri) <= 512
        ),
    scope text
        NOT NULL
        CONSTRAINT auth_oauth2_client_authorization_codes_scope_check
        CHECK (
            scope ~ '^([\x21\x23-\x5B\x5D-\x7E]+( [\x21\x23-\x5B\x5D-\x7E]+)*)?$'
            AND char_length(scope) <= 512
        ),
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    expires_at timestamp with time zone
        NOT NULL
        DEFAULT (now() + interval '10 minutes'),
    completed_at timestamp with time zone,
    -- The refresh-token family this code creates when a client redeems it (NULL until then). A
    -- replay of an already-consumed code can then revoke the tokens it minted (RFC 9700 §4.1.2).
    -- The identity-immutable trigger below does not guard this column, so the completing UPDATE may
    -- set it.
    created_family_id text
        CONSTRAINT auth_oauth2_client_authorization_codes_created_family_id_check
        CHECK (
            created_family_id IS NULL
            OR (char_length(created_family_id) > 0 AND char_length(created_family_id) <= 64)
        ),
    nonce text
        CONSTRAINT auth_oauth2_client_authorization_codes_nonce_check
        CHECK (
            nonce IS NULL
            OR (char_length(nonce) >= 3 AND char_length(nonce) <= 256)
        ),
    -- S256 challenge: exactly 43 chars (base64url SHA-256, unpadded).
    code_challenge text
        CONSTRAINT auth_oauth2_client_authorization_codes_code_challenge_check
        CHECK (
            code_challenge IS NULL
            OR (
                char_length(code_challenge) = 43
                AND code_challenge ~ '^[A-Za-z0-9_-]+$'
            )
        ),
    -- Shortened: ..._code_challenge_method_check reaches 66 bytes and silently truncates at 63.
    code_challenge_method text
        CONSTRAINT auth_oauth2_client_authorization_codes_method_s256_check
        CHECK (
            code_challenge_method IS NULL
            OR code_challenge_method = 'S256'
        ),

    -- challenge and method are present (or absent) together.
    CONSTRAINT auth_oauth2_client_authorization_codes_pkce_check
    CHECK ((code_challenge IS NULL) = (code_challenge_method IS NULL))
);

CREATE INDEX auth_oauth2_client_authorization_codes_client_id_idx
    ON auth_oauth2_client_authorization_codes (client_id);

CREATE INDEX auth_oauth2_client_authorization_codes_user_id_idx
    ON auth_oauth2_client_authorization_codes (user_id);

CREATE INDEX auth_oauth2_client_authorization_codes_expires_at_idx
    ON auth_oauth2_client_authorization_codes (expires_at);

-- Shortened name: ..._completed_at_set_once_trig reaches 66 bytes and silently truncates at 63.
CREATE TRIGGER auth_oauth2_client_authorization_codes_completed_at_trig
    BEFORE UPDATE ON auth_oauth2_client_authorization_codes
    FOR EACH ROW
    WHEN (OLD.completed_at IS NOT NULL AND OLD.completed_at IS DISTINCT FROM NEW.completed_at)
    EXECUTE FUNCTION tf_set_once_completed_at();

-- The completing UPDATE writes created_family_id (alongside completed_at), never again: rewriting
-- it would sever the replayed-code -> family-revocation linkage (RFC 9700). Set-once, so the
-- trigger allows NULL -> value (completion) but not value -> anything.
CREATE TRIGGER auth_oauth2_client_authorization_codes_created_family_id_trig
    BEFORE UPDATE ON auth_oauth2_client_authorization_codes
    FOR EACH ROW
    WHEN (
        OLD.created_family_id IS NOT NULL
        AND OLD.created_family_id IS DISTINCT FROM NEW.created_family_id
    )
    EXECUTE FUNCTION tf_set_once_created_family_id();

-- Grant fields are write-once: client_id, user_id, redirect_uri, scope, PKCE, nonce, and created_at
-- must not change between issue and redemption.
CREATE FUNCTION tf_auth_oauth2_client_authorization_codes_identity_immutable()
    RETURNS trigger
    LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION
        'auth_oauth2_client_authorization_codes: grant fields are immutable'
        USING ERRCODE = 'check_violation';
END;
$$;

CREATE TRIGGER auth_oauth2_client_authorization_codes_identity_immutable_trig
    BEFORE UPDATE ON auth_oauth2_client_authorization_codes
    FOR EACH ROW
    WHEN (
        OLD.client_id IS DISTINCT FROM NEW.client_id
        OR OLD.user_id IS DISTINCT FROM NEW.user_id
        OR OLD.redirect_uri IS DISTINCT FROM NEW.redirect_uri
        OR OLD.scope IS DISTINCT FROM NEW.scope
        OR OLD.code_challenge IS DISTINCT FROM NEW.code_challenge
        OR OLD.code_challenge_method IS DISTINCT FROM NEW.code_challenge_method
        OR OLD.nonce IS DISTINCT FROM NEW.nonce
        OR OLD.created_at IS DISTINCT FROM NEW.created_at
    )
    EXECUTE FUNCTION tf_auth_oauth2_client_authorization_codes_identity_immutable();

-- is_pkce_required lives on the client row, so the pkce_check above can't enforce it cross-table;
-- this trigger is the backstop. A code for an is_pkce_required client must carry a challenge.
-- INSERT-only: codes are insert-then-consume and the FK guarantees the client exists at insert
-- time, so nothing to defer.
CREATE FUNCTION tf_auth_oauth2_client_authorization_codes_pkce_required()
    RETURNS trigger
    LANGUAGE plpgsql
AS $$
BEGIN
    IF NEW.code_challenge IS NULL AND EXISTS (
        SELECT 1 FROM auth_oauth2_clients
        WHERE id = NEW.client_id AND is_pkce_required
    ) THEN
        RAISE EXCEPTION
            'oauth2 client "%" requires PKCE; code needs a code_challenge',
            NEW.client_id
            USING ERRCODE = 'check_violation';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER auth_oauth2_client_authorization_codes_pkce_required_trig
    BEFORE INSERT ON auth_oauth2_client_authorization_codes
    FOR EACH ROW
    EXECUTE FUNCTION tf_auth_oauth2_client_authorization_codes_pkce_required();

CREATE TABLE auth_oauth2_client_refresh_tokens (
    id bigint
        PRIMARY KEY
        GENERATED ALWAYS AS IDENTITY,
    -- Stores only the SHA-256 hash of the refresh token, never the raw token.
    token_hash text
        NOT NULL
        UNIQUE
        CONSTRAINT auth_oauth2_client_refresh_tokens_token_hash_check
        CHECK (
            char_length(token_hash) = 43
            AND token_hash ~ '^[A-Za-z0-9_-]+$'
        ),
    user_id bigint
        NOT NULL
        REFERENCES users(id) ON DELETE CASCADE,
    client_id text
        NOT NULL
        REFERENCES auth_oauth2_clients(id) ON DELETE CASCADE,
    scope text
        NOT NULL
        CONSTRAINT auth_oauth2_client_refresh_tokens_scope_check
        CHECK (
            scope ~ '^([\x21\x23-\x5B\x5D-\x7E]+( [\x21\x23-\x5B\x5D-\x7E]+)*)?$'
            AND char_length(scope) <= 512
        ),
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    expires_at timestamp with time zone
        NOT NULL
        DEFAULT (now() + interval '90 days'),
    family_id text
        NOT NULL
        CONSTRAINT auth_oauth2_client_refresh_tokens_family_id_check
        CHECK (char_length(family_id) > 0 AND char_length(family_id) <= 64),
    consumed_at timestamp with time zone,
    revoked_at timestamp with time zone
);

CREATE INDEX auth_oauth2_client_refresh_tokens_client_id_idx
    ON auth_oauth2_client_refresh_tokens (client_id);

CREATE INDEX auth_oauth2_client_refresh_tokens_user_id_idx
    ON auth_oauth2_client_refresh_tokens (user_id);

CREATE INDEX auth_oauth2_client_refresh_tokens_family_id_idx
    ON auth_oauth2_client_refresh_tokens (family_id);

CREATE INDEX auth_oauth2_client_refresh_tokens_expires_at_idx
    ON auth_oauth2_client_refresh_tokens (expires_at);

CREATE TRIGGER auth_oauth2_client_refresh_tokens_consumed_at_set_once_trig
    BEFORE UPDATE ON auth_oauth2_client_refresh_tokens
    FOR EACH ROW
    WHEN (OLD.consumed_at IS NOT NULL AND OLD.consumed_at IS DISTINCT FROM NEW.consumed_at)
    EXECUTE FUNCTION tf_set_once_consumed_at();

CREATE TRIGGER auth_oauth2_client_refresh_tokens_revoked_at_set_once_trig
    BEFORE UPDATE ON auth_oauth2_client_refresh_tokens
    FOR EACH ROW
    WHEN (OLD.revoked_at IS NOT NULL AND OLD.revoked_at IS DISTINCT FROM NEW.revoked_at)
    EXECUTE FUNCTION tf_set_once_revoked_at();

-- token_hash, family_id, user_id, client_id, scope, and created_at are write-once: token rotation
-- always issues a new row; UPDATE only touches consumed_at, revoked_at, and expires_at.
CREATE FUNCTION tf_auth_oauth2_client_refresh_tokens_identity_immutable()
    RETURNS trigger
    LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION
        'auth_oauth2_client_refresh_tokens: token_hash, family_id, user_id, client_id, scope, '
        'and created_at are immutable'
        USING ERRCODE = 'check_violation';
END;
$$;

CREATE TRIGGER auth_oauth2_client_refresh_tokens_identity_immutable_trig
    BEFORE UPDATE ON auth_oauth2_client_refresh_tokens
    FOR EACH ROW
    WHEN (
        OLD.token_hash IS DISTINCT FROM NEW.token_hash
        OR OLD.family_id IS DISTINCT FROM NEW.family_id
        OR OLD.user_id IS DISTINCT FROM NEW.user_id
        OR OLD.client_id IS DISTINCT FROM NEW.client_id
        OR OLD.scope IS DISTINCT FROM NEW.scope
        OR OLD.created_at IS DISTINCT FROM NEW.created_at
    )
    EXECUTE FUNCTION tf_auth_oauth2_client_refresh_tokens_identity_immutable();

CREATE TABLE auth_oauth2_client_consents (
    id bigint
        PRIMARY KEY
        GENERATED ALWAYS AS IDENTITY,
    user_id bigint
        NOT NULL
        REFERENCES users(id) ON DELETE CASCADE,
    client_id text
        NOT NULL
        REFERENCES auth_oauth2_clients(id) ON DELETE CASCADE,
    scope text
        NOT NULL
        CONSTRAINT auth_oauth2_client_consents_scope_check
        CHECK (
            scope ~ '^([\x21\x23-\x5B\x5D-\x7E]+( [\x21\x23-\x5B\x5D-\x7E]+)*)?$'
            AND char_length(scope) <= 512
        ),
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    updated_at timestamp with time zone NOT NULL DEFAULT now(),

    CONSTRAINT auth_oauth2_client_consents_user_id_client_id_key
        UNIQUE (user_id, client_id)
);

CREATE INDEX auth_oauth2_client_consents_client_id_idx
    ON auth_oauth2_client_consents (client_id);

CREATE TRIGGER auth_oauth2_client_consents_updated_at_trig
    BEFORE UPDATE ON auth_oauth2_client_consents
    FOR EACH ROW
    WHEN (OLD.* IS DISTINCT FROM NEW.*)
    EXECUTE FUNCTION tf_set_updated_at();

-- Enforce "every OAuth2 client has at least one redirect uri" via deferred constraint triggers (so
-- a client and its first uri insert in one txn).
CREATE FUNCTION tf_auth_oauth2_clients_ensure_redirect_uri()
    RETURNS trigger
    LANGUAGE plpgsql
AS $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM auth_oauth2_client_redirect_uris WHERE client_id = NEW.id
    ) THEN
        RAISE EXCEPTION 'oauth2 client "%" must have at least one redirect uri', NEW.id
            USING ERRCODE = 'check_violation';
    END IF;
    RETURN NULL;
END;
$$;

CREATE FUNCTION tf_auth_oauth2_client_redirect_uris_prevent_orphan()
    RETURNS trigger
    LANGUAGE plpgsql
AS $$
BEGIN
    PERFORM 1 FROM auth_oauth2_clients WHERE id = OLD.client_id FOR UPDATE;
    IF EXISTS (SELECT 1 FROM auth_oauth2_clients WHERE id = OLD.client_id)
    AND NOT EXISTS (
        SELECT 1 FROM auth_oauth2_client_redirect_uris WHERE client_id = OLD.client_id
    ) THEN
        RAISE EXCEPTION 'oauth2 client "%" must have at least one redirect uri', OLD.client_id
            USING ERRCODE = 'check_violation';
    END IF;
    RETURN NULL;
END;
$$;

CREATE CONSTRAINT TRIGGER auth_oauth2_clients_ensure_redirect_uri_trig
    AFTER INSERT ON auth_oauth2_clients
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW
    EXECUTE FUNCTION tf_auth_oauth2_clients_ensure_redirect_uri();

COMMENT ON TRIGGER auth_oauth2_clients_ensure_redirect_uri_trig ON auth_oauth2_clients IS
    'Deferred: every client must have at least one redirect uri by commit (client and its first '
    'uri insert in one transaction).';

CREATE CONSTRAINT TRIGGER auth_oauth2_client_redirect_uris_prevent_orphan_trig
    AFTER DELETE OR UPDATE ON auth_oauth2_client_redirect_uris
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW
    EXECUTE FUNCTION tf_auth_oauth2_client_redirect_uris_prevent_orphan();

COMMENT ON TRIGGER auth_oauth2_client_redirect_uris_prevent_orphan_trig
    ON auth_oauth2_client_redirect_uris IS
    'Deferred: the guard rejects removing a client''s last redirect URI, unless the client itself '
    'is gone, so a cascade delete is exempt.';

/*
## OAuth2/OIDC (CeresForge as client/RP)
*/

-- Subtype body for OAuth2/OIDC providers. Shared fields live in auth_providers. The composite FK
-- pins the discriminator so this row can only attach to a kind='oauth2' envelope.
CREATE TABLE auth_oauth2_providers (
    provider_id bigint
        PRIMARY KEY,
    -- Pinned discriminator: generated so the composite FK always matches the envelope.
    kind text
        NOT NULL
        GENERATED ALWAYS AS ('oauth2') STORED,
    -- OIDC-only: the discovery anchor and the id_token issuer for validation. Present iff is_oidc
    -- (the issuer_oidc check). UNIQUE still holds: a plain OAuth2 provider has no issuer (NULL),
    -- and Postgres allows many NULLs; non-OIDC credentials use (provider_id, subject) instead of
    -- (issuer, subject).
    issuer text
        UNIQUE
        CONSTRAINT auth_oauth2_providers_issuer_check
        CHECK (char_length(issuer) > 0 AND char_length(issuer) <= 512),
    client_id text
        NOT NULL
        CONSTRAINT auth_oauth2_providers_client_id_check
        CHECK (char_length(client_id) > 0 AND char_length(client_id) <= 256),
    client_secret_ciphertext text
        NOT NULL
        CONSTRAINT auth_oauth2_providers_client_secret_ciphertext_check
        CHECK (
            char_length(client_secret_ciphertext) > 0
            AND char_length(client_secret_ciphertext) <= 1024
        ),
    -- Sub-discriminator: OIDC providers issue an id_token (nonce-bound, JWKS-verified); plain
    -- OAuth2 providers (e.g. Discord) use access token + userinfo only. Controls nonce/jwks
    -- requirements via *_oidc_consistent_trig, and requires openid in scopes (oidc_openid check).
    -- is_oidc stays here: an OAuth2-only sub-discriminator, not a provider-level one.
    is_oidc boolean NOT NULL DEFAULT true,
    -- Mapping only: [{"claim": <id_token claim>, "target": <user field>}], e.g. [{"claim":
    -- "given_name", "target": "first_name"}]. Targets are the shared user fields plus the
    -- OAuth2-only `subject` and `is_email_verified`; every target is single-valued. Unlike
    -- auth_saml_providers.attributes, this list does not specify which scopes to request; `scope`
    -- controls what the provider returns.
    claims jsonb
        NOT NULL
        CONSTRAINT auth_oauth2_providers_claims_check
        CHECK (claims IS JSON ARRAY),
    -- Space-delimited scope set this provider requests at the authorize endpoint (singular `scope`
    -- like the rest of the consent/grant side, per the OAuth2 `scope` parameter). Per-provider, not
    -- hardcoded: upstreams differ (groups, custom scopes). At least one scope must exist (no outer
    -- `?`); an OIDC provider must include openid (oidc_openid check), a plain OAuth2 one need not.
    scope text
        NOT NULL
        CONSTRAINT auth_oauth2_providers_scope_check
        CHECK (
            scope ~ '^[\x21\x23-\x5B\x5D-\x7E]+( [\x21\x23-\x5B\x5D-\x7E]+)*$'
            AND char_length(scope) <= 512
        ),

    FOREIGN KEY (provider_id, kind)
        REFERENCES auth_providers (id, kind) ON DELETE CASCADE,

    -- An OIDC provider must include openid, which yields an id_token. Plain OAuth2 need not.
    CONSTRAINT auth_oauth2_providers_oidc_openid_check
    CHECK (NOT is_oidc OR scope ~ '(^|\s)openid(\s|$)'),

    -- issuer is present iff is_oidc: it drives discovery and id_token validation.
    CONSTRAINT auth_oauth2_providers_issuer_oidc_check
    CHECK ((issuer IS NOT NULL) = is_oidc)
);

-- Prevent deleting an oauth2 body while its envelope still exists. Deferred so a DELETE FROM
-- auth_providers (which cascades here) completes atomically.
CREATE FUNCTION tf_auth_oauth2_providers_prevent_orphan()
    RETURNS trigger
    LANGUAGE plpgsql
AS $$
BEGIN
    PERFORM 1 FROM auth_providers WHERE id = OLD.provider_id FOR UPDATE;
    IF EXISTS (SELECT 1 FROM auth_providers WHERE id = OLD.provider_id) THEN
        RAISE EXCEPTION
            'deleting auth_oauth2_providers % would orphan its auth_providers envelope',
            OLD.provider_id
            USING ERRCODE = 'foreign_key_violation';
    END IF;
    RETURN NULL;
END;
$$;

CREATE CONSTRAINT TRIGGER auth_oauth2_providers_prevent_orphan_trig
    AFTER DELETE ON auth_oauth2_providers
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW
    EXECUTE FUNCTION tf_auth_oauth2_providers_prevent_orphan();

-- is_oidc is identity-defining: it gates cross-table invariants (jwks in metadata, nonce in
-- requests) that no single-table CHECK or per-row trigger re-validates on a provider UPDATE. Make
-- it immutable; recreate the provider to switch OIDC-ness.
CREATE FUNCTION tf_auth_oauth2_providers_is_oidc_immutable()
    RETURNS trigger
    LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION
        'oauth2 provider "%" is_oidc is immutable; recreate the provider to switch',
        OLD.provider_id
        USING ERRCODE = 'check_violation';
END;
$$;

CREATE TRIGGER auth_oauth2_providers_is_oidc_immutable_trig
    BEFORE UPDATE ON auth_oauth2_providers
    FOR EACH ROW
    WHEN (OLD.is_oidc IS DISTINCT FROM NEW.is_oidc)
    EXECUTE FUNCTION tf_auth_oauth2_providers_is_oidc_immutable();

CREATE TABLE auth_oauth2_provider_requests (
    id bigint
        PRIMARY KEY
        GENERATED ALWAYS AS IDENTITY,
    state text
        NOT NULL
        UNIQUE
        CONSTRAINT auth_oauth2_provider_requests_state_check
        CHECK (char_length(state) = 43 AND state ~ '^[A-Za-z0-9_-]+$'),
    provider_id bigint
        NOT NULL
        REFERENCES auth_oauth2_providers(provider_id) ON DELETE CASCADE,
    -- user_id NULL marks a registration (login creates a new user); a non-NULL value marks a
    -- connection to that existing user. The insert fixes this and correlation_immutable blocks any
    -- later change, so the request never resolves a user afterward; the completed identity lives in
    -- auth_oauth2_provider_credentials, with subject as its key.
    user_id bigint REFERENCES users(id) ON DELETE CASCADE,
    -- Nullable: the nonce binds an id_token, which only OIDC providers issue. Required iff the
    -- provider is_oidc, via the oidc_consistent trigger below.
    nonce text
        CONSTRAINT auth_oauth2_provider_requests_nonce_check
        CHECK (nonce IS NULL OR (char_length(nonce) = 43 AND nonce ~ '^[A-Za-z0-9_-]+$')),
    code_verifier text
        NOT NULL
        CONSTRAINT auth_oauth2_provider_requests_code_verifier_check
        CHECK (
            char_length(code_verifier) = 43
            AND code_verifier ~ '^[A-Za-z0-9_-]+$'
        ),
    next text
        NOT NULL
        CONSTRAINT auth_oauth2_provider_requests_next_check
        CHECK (char_length(next) > 0 AND char_length(next) <= 512),
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    expires_at timestamp with time zone
        NOT NULL
        DEFAULT (now() + interval '10 minutes'),
    completed_at timestamp with time zone,
    -- On completion, the audit ledger records the resolved subject (the `sub`) and the raw id_token
    -- payload.
    subject text
        CONSTRAINT auth_oauth2_provider_requests_subject_check
        CHECK (
            subject IS NULL
            OR (char_length(subject) > 0 AND char_length(subject) <= 256)
        ),
    raw_userinfo jsonb
        CONSTRAINT auth_oauth2_provider_requests_raw_userinfo_check
        CHECK (raw_userinfo IS JSON OBJECT),

    CONSTRAINT auth_oauth2_provider_requests_completion_check
    CHECK (
        (
            completed_at IS NULL
            AND subject IS NULL
            AND raw_userinfo IS NULL
        )
        OR (
            completed_at IS NOT NULL
            AND subject IS NOT NULL
            AND raw_userinfo IS NOT NULL
        )
    )
);

COMMENT ON CONSTRAINT auth_oauth2_provider_requests_completion_check
    ON auth_oauth2_provider_requests IS
    'A request is either pending (completed_at, subject, raw_userinfo all NULL) or completed (all '
    'set). complete_request flips completed_at from NULL once, as the single-use gate (run after '
    'the token exchange succeeds).';

CREATE INDEX auth_oauth2_provider_requests_provider_id_idx
    ON auth_oauth2_provider_requests (provider_id);

CREATE INDEX auth_oauth2_provider_requests_user_id_idx
    ON auth_oauth2_provider_requests (user_id);

CREATE INDEX auth_oauth2_provider_requests_expires_at_idx
    ON auth_oauth2_provider_requests (expires_at);

CREATE TRIGGER auth_oauth2_provider_requests_completed_at_set_once_trig
    BEFORE UPDATE ON auth_oauth2_provider_requests
    FOR EACH ROW
    WHEN (OLD.completed_at IS NOT NULL AND OLD.completed_at IS DISTINCT FROM NEW.completed_at)
    EXECUTE FUNCTION tf_set_once_completed_at();

-- subject and raw_userinfo are write-once once completed: the audit record must never change.
CREATE FUNCTION tf_auth_oauth2_provider_requests_payload_immutable()
    RETURNS trigger
    LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION
        'auth_oauth2_provider_requests: subject and raw_userinfo are immutable once set'
        USING ERRCODE = 'check_violation';
END;
$$;

CREATE TRIGGER auth_oauth2_provider_requests_payload_immutable_trig
    BEFORE UPDATE ON auth_oauth2_provider_requests
    FOR EACH ROW
    WHEN (
        OLD.completed_at IS NOT NULL
        AND (
            OLD.subject IS DISTINCT FROM NEW.subject
            OR OLD.raw_userinfo IS DISTINCT FROM NEW.raw_userinfo
        )
    )
    EXECUTE FUNCTION tf_auth_oauth2_provider_requests_payload_immutable();

-- Correlation and routing fields are write-once: set at insert, consumed at callback.
CREATE FUNCTION tf_auth_oauth2_provider_requests_correlation_immutable()
    RETURNS trigger
    LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION
        'auth_oauth2_provider_requests: state, provider_id, user_id, nonce, code_verifier, '
        'next, created_at, and expires_at are immutable'
        USING ERRCODE = 'check_violation';
END;
$$;

CREATE TRIGGER auth_oauth2_provider_requests_correlation_immutable_trig
    BEFORE UPDATE ON auth_oauth2_provider_requests
    FOR EACH ROW
    WHEN (
        OLD.state IS DISTINCT FROM NEW.state
        OR OLD.provider_id IS DISTINCT FROM NEW.provider_id
        OR OLD.user_id IS DISTINCT FROM NEW.user_id
        OR OLD.nonce IS DISTINCT FROM NEW.nonce
        OR OLD.code_verifier IS DISTINCT FROM NEW.code_verifier
        OR OLD.next IS DISTINCT FROM NEW.next
        OR OLD.created_at IS DISTINCT FROM NEW.created_at
        OR OLD.expires_at IS DISTINCT FROM NEW.expires_at
    )
    EXECUTE FUNCTION tf_auth_oauth2_provider_requests_correlation_immutable();

-- The nonce is OIDC-only (it binds an id_token). Enforce the is_oidc invariant for requests: OIDC
-- providers require a nonce; non-OIDC providers forbid one. is_oidc is immutable (see
-- auth_oauth2_providers_is_oidc_immutable_trig). Spans two tables, so it can't be a CHECK.
CREATE FUNCTION tf_auth_oauth2_provider_requests_oidc_consistent()
    RETURNS trigger
    LANGUAGE plpgsql
AS $$
DECLARE
    oidc boolean;
BEGIN
    SELECT is_oidc INTO oidc
    FROM auth_oauth2_providers
    WHERE provider_id = NEW.provider_id;
    IF oidc AND NEW.nonce IS NULL THEN
        RAISE EXCEPTION
            'oauth2 provider "%" is OIDC; request needs a nonce', NEW.provider_id
            USING ERRCODE = 'check_violation';
    ELSIF NOT oidc AND NEW.nonce IS NOT NULL THEN
        RAISE EXCEPTION
            'oauth2 provider "%" is not OIDC; request must not carry a nonce', NEW.provider_id
            USING ERRCODE = 'check_violation';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER auth_oauth2_provider_requests_oidc_consistent_trig
    BEFORE INSERT ON auth_oauth2_provider_requests
    FOR EACH ROW
    EXECUTE FUNCTION tf_auth_oauth2_provider_requests_oidc_consistent();

CREATE TABLE auth_oauth2_provider_credentials (
    id bigint
        PRIMARY KEY
        GENERATED ALWAYS AS IDENTITY,
    provider_id bigint
        NOT NULL
        REFERENCES auth_oauth2_providers(provider_id) ON DELETE CASCADE,
    subject text
        NOT NULL
        CONSTRAINT auth_oauth2_provider_credentials_subject_check
        CHECK (char_length(subject) > 0 AND char_length(subject) <= 256),
    user_id bigint
        NOT NULL
        REFERENCES users(id) ON DELETE CASCADE,
    -- raw_userinfo: the untouched verified id_token payload; profile: parsed identity fields
    -- translated from it via the provider's claim mapping, where each single-valued target holds
    -- one scalar.
    raw_userinfo jsonb
        NOT NULL
        CONSTRAINT auth_oauth2_provider_credentials_raw_userinfo_check
        CHECK (raw_userinfo IS JSON OBJECT),
    profile jsonb
        NOT NULL
        CONSTRAINT auth_oauth2_provider_credentials_profile_check
        CHECK (profile IS JSON OBJECT),
    -- NULL while connected; set when the user severs the link. The subject constraint stays full
    -- (not partial): no other CeresForge account can claim a disconnected subject; only the
    -- original user can reconnect it.
    --
    -- SAFETY ASSUMPTION: this tombstone is only safe while subject values are non-reassignable. For
    -- is_oidc = true the OIDC spec guarantees sub is never reassigned within an issuer, so the full
    -- unique is exactly right. For is_oidc = false (e.g. Discord snowflake) the guarantee is
    -- provider-specific and assumed stable; if a non-OIDC provider could recycle subjects, a
    -- disconnected tombstone would permanently lock out the new legitimate owner with no in-band
    -- recovery. Provider configuration MUST source subjects from a provider-stable, non-recyclable
    -- field to keep this guarantee valid.
    disconnected_at timestamp with time zone,
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    updated_at timestamp with time zone NOT NULL DEFAULT now(),

    CONSTRAINT auth_oauth2_provider_credentials_provider_id_subject_key
        UNIQUE (provider_id, subject)
);

CREATE INDEX auth_oauth2_provider_credentials_user_id_idx
    ON auth_oauth2_provider_credentials (user_id);

-- At most one active (non-disconnected) connection per user per provider. Replaces the full UNIQUE
-- (provider_id, user_id) constraint so a user can disconnect and reconnect a different subject.
-- Exactly one row exists per external identity (subject unique is full); reconnect is UPDATE ...
-- SET disconnected_at = NULL on that row, not a new insert. Profile and raw_userinfo go stale after
-- reconnect until the app refreshes them; not enforced here.
CREATE UNIQUE INDEX auth_oauth2_provider_credentials_active_user_idx
    ON auth_oauth2_provider_credentials (provider_id, user_id)
    WHERE disconnected_at IS NULL;

COMMENT ON INDEX auth_oauth2_provider_credentials_active_user_idx IS
    'At most one active (non-disconnected) credential per (provider, user). Partial unique '
    'indexes are non-deferrable, so reconnecting identity A while identity B is still active '
    'for the same (provider, user) requires two statements: disconnect B first '
    '(SET disconnected_at = now()), then reconnect A (SET disconnected_at = NULL).';

CREATE TRIGGER auth_oauth2_provider_credentials_updated_at_trig
    BEFORE UPDATE ON auth_oauth2_provider_credentials
    FOR EACH ROW
    WHEN (OLD.* IS DISTINCT FROM NEW.*)
    EXECUTE FUNCTION tf_set_updated_at();

-- Exact-equality profile lookups (profile ->> 'key' = $1).
CREATE INDEX auth_oauth2_provider_credentials_profile_email_idx
    ON auth_oauth2_provider_credentials ((profile ->> 'email'));
CREATE INDEX auth_oauth2_provider_credentials_profile_username_idx
    ON auth_oauth2_provider_credentials ((profile ->> 'username'));

-- Resolved endpoints, 1:1 with the provider: fetched from discovery for an OIDC provider,
-- admin-configured for a plain OAuth2 one (rationale in COMMENT ON).
CREATE TABLE auth_oauth2_provider_metadata (
    provider_id bigint
        PRIMARY KEY
        REFERENCES auth_oauth2_providers(provider_id) ON DELETE CASCADE,
    authorization_endpoint text
        NOT NULL
        CONSTRAINT auth_oauth2_provider_metadata_authorization_endpoint_check
        CHECK (
            char_length(authorization_endpoint) > 0
            AND char_length(authorization_endpoint) <= 512
        ),
    token_endpoint text
        NOT NULL
        CONSTRAINT auth_oauth2_provider_metadata_token_endpoint_check
        CHECK (
            char_length(token_endpoint) > 0
            AND char_length(token_endpoint) <= 512
        ),
    -- Required for a non-OIDC provider (it's the only way to read the profile), optional for OIDC
    -- (the id_token carries the claims). The oidc_consistent trigger below enforces this.
    userinfo_endpoint text
        CONSTRAINT auth_oauth2_provider_metadata_userinfo_endpoint_check
        CHECK (
            userinfo_endpoint IS NULL
            OR (
                char_length(userinfo_endpoint) > 0
                AND char_length(userinfo_endpoint) <= 512
            )
        ),
    -- jwks_uri/jwks verify id_token signatures and exist iff is_oidc; oidc_consistent enforces it.
    jwks_uri text
        CONSTRAINT auth_oauth2_provider_metadata_jwks_uri_check
        CHECK (jwks_uri IS NULL OR (char_length(jwks_uri) > 0 AND char_length(jwks_uri) <= 512)),
    jwks jsonb
        CONSTRAINT auth_oauth2_provider_metadata_jwks_check
        CHECK (jwks IS JSON OBJECT),
    fetched_at timestamp with time zone NOT NULL DEFAULT now(),
    expires_at timestamp with time zone
);

COMMENT ON TABLE auth_oauth2_provider_metadata IS
    'Resolved endpoints (+ JWKS for OIDC), 1:1 with the provider. For OIDC this is the fetched '
    'discovery document; for plain OAuth2 an admin configures it. No updated_at trigger: each '
    'refresh rewrites the row entirely, so fetched_at is the update time (distinct from the '
    'provider row''s admin-edit updated_at).';

-- Enforce the is_oidc invariant for metadata: OIDC providers require jwks_uri/jwks (they verify
-- id_token signatures); non-OIDC providers forbid them and require userinfo_endpoint instead (the
-- only way to read the profile without an id_token). auth_oauth2_providers_is_oidc_immutable_trig
-- guarantees is_oidc is immutable. INSERT OR UPDATE because each refresh upserts the metadata row.
CREATE FUNCTION tf_auth_oauth2_provider_metadata_oidc_consistent()
    RETURNS trigger
    LANGUAGE plpgsql
AS $$
DECLARE
    oidc boolean;
BEGIN
    SELECT is_oidc INTO oidc
    FROM auth_oauth2_providers
    WHERE provider_id = NEW.provider_id;
    IF oidc THEN
        IF NEW.jwks_uri IS NULL OR NEW.jwks IS NULL THEN
            RAISE EXCEPTION
                'oauth2 provider "%" is OIDC; metadata needs jwks_uri and jwks', NEW.provider_id
                USING ERRCODE = 'check_violation';
        END IF;
    ELSE
        IF NEW.jwks_uri IS NOT NULL OR NEW.jwks IS NOT NULL THEN
            RAISE EXCEPTION
                'oauth2 provider "%" is not OIDC; metadata must not carry jwks', NEW.provider_id
                USING ERRCODE = 'check_violation';
        END IF;
        IF NEW.userinfo_endpoint IS NULL THEN
            RAISE EXCEPTION
                'oauth2 provider "%" is not OIDC; metadata needs a userinfo_endpoint',
                NEW.provider_id
                USING ERRCODE = 'check_violation';
        END IF;
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER auth_oauth2_provider_metadata_oidc_consistent_trig
    BEFORE INSERT OR UPDATE ON auth_oauth2_provider_metadata
    FOR EACH ROW
    EXECUTE FUNCTION tf_auth_oauth2_provider_metadata_oidc_consistent();

/*
## Signing keys (JWKS rotation)
*/

CREATE TABLE auth_jwk_public_keys (
    id bigint
        PRIMARY KEY
        GENERATED ALWAYS AS IDENTITY,
    kid text
        NOT NULL
        UNIQUE
        CONSTRAINT auth_jwk_public_keys_kid_check
        CHECK (char_length(kid) = 43 AND kid ~ '^[A-Za-z0-9_-]+$'),
    -- base64url RSA modulus; <= 2048 comfortably covers an RSA-8192 modulus, keeping the file's
    -- "upper bound every text column" convention.
    n text
        NOT NULL
        CONSTRAINT auth_jwk_public_keys_n_check
        CHECK (char_length(n) > 0 AND char_length(n) <= 2048 AND n ~ '^[A-Za-z0-9_-]+$'),
    e text
        NOT NULL
        CONSTRAINT auth_jwk_public_keys_e_check
        CHECK (char_length(e) > 0 AND char_length(e) <= 2048 AND e ~ '^[A-Za-z0-9_-]+$'),
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    expires_at timestamp with time zone,
    revoked_at timestamp with time zone
);

CREATE TRIGGER auth_jwk_public_keys_revoked_at_set_once_trig
    BEFORE UPDATE ON auth_jwk_public_keys
    FOR EACH ROW
    WHEN (OLD.revoked_at IS NOT NULL AND OLD.revoked_at IS DISTINCT FROM NEW.revoked_at)
    EXECUTE FUNCTION tf_set_once_revoked_at();

-- kid, n, e, and created_at are write-once: the public material RPs trust to verify tokens must
-- never change under an existing kid.
CREATE FUNCTION tf_auth_jwk_public_keys_identity_immutable()
    RETURNS trigger
    LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION
        'auth_jwk_public_keys: kid, n, e, and created_at are immutable'
        USING ERRCODE = 'check_violation';
END;
$$;

CREATE TRIGGER auth_jwk_public_keys_identity_immutable_trig
    BEFORE UPDATE ON auth_jwk_public_keys
    FOR EACH ROW
    WHEN (
        OLD.kid IS DISTINCT FROM NEW.kid
        OR OLD.n IS DISTINCT FROM NEW.n
        OR OLD.e IS DISTINCT FROM NEW.e
        OR OLD.created_at IS DISTINCT FROM NEW.created_at
    )
    EXECUTE FUNCTION tf_auth_jwk_public_keys_identity_immutable();

CREATE TABLE auth_jwk_private_keys (
    kid text
        PRIMARY KEY
        REFERENCES auth_jwk_public_keys(kid) ON DELETE CASCADE,
    private_key_ciphertext text
        NOT NULL
        CONSTRAINT auth_jwk_private_keys_private_key_ciphertext_check
        CHECK (
            char_length(private_key_ciphertext) > 0
            AND char_length(private_key_ciphertext) <= 8192
        ),
    -- NULL while active; set on deactivation. Set once: see deactivated_at_set_once_trig.
    deactivated_at timestamp with time zone,
    created_at timestamp with time zone NOT NULL DEFAULT now()
);

-- Idiomatic, version-independent: index a constant over the active set.
CREATE UNIQUE INDEX auth_jwk_private_keys_one_active_idx
    ON auth_jwk_private_keys ((true))
    WHERE deactivated_at IS NULL;

-- kid, private_key_ciphertext, and created_at are write-once: the key material and its identity are
-- fixed at issue and must never change.
CREATE FUNCTION tf_auth_jwk_private_keys_identity_immutable()
    RETURNS trigger
    LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION
        'auth_jwk_private_keys: kid, private_key_ciphertext, and created_at are immutable'
        USING ERRCODE = 'check_violation';
END;
$$;

CREATE TRIGGER auth_jwk_private_keys_identity_immutable_trig
    BEFORE UPDATE ON auth_jwk_private_keys
    FOR EACH ROW
    WHEN (
        OLD.kid IS DISTINCT FROM NEW.kid
        OR OLD.private_key_ciphertext IS DISTINCT FROM NEW.private_key_ciphertext
        OR OLD.created_at IS DISTINCT FROM NEW.created_at
    )
    EXECUTE FUNCTION tf_auth_jwk_private_keys_identity_immutable();

CREATE TRIGGER auth_jwk_private_keys_deactivated_at_set_once_trig
    BEFORE UPDATE ON auth_jwk_private_keys
    FOR EACH ROW
    WHEN (OLD.deactivated_at IS NOT NULL AND OLD.deactivated_at IS DISTINCT FROM NEW.deactivated_at)
    EXECUTE FUNCTION tf_set_once_deactivated_at();

