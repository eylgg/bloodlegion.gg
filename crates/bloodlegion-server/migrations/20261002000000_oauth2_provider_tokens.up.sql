-- The tokens an upstream provider issued at the latest sign-in, so the server can call the
-- provider's APIs on the person's behalf afterwards (Battle.net's WoW profile API). Keyed by the
-- upstream identity (provider, subject) rather than the credential, because a first sign-in that
-- still needs a username has no credential yet; the credential joins on the same pair. One row
-- per identity, rewritten on every sign-in. Tokens are secrets, so they are stored encrypted.
CREATE TABLE auth_oauth2_provider_tokens (
    provider_id bigint
        NOT NULL
        REFERENCES auth_oauth2_providers(provider_id) ON DELETE CASCADE,
    subject text
        NOT NULL
        CONSTRAINT auth_oauth2_provider_tokens_subject_check
        CHECK (char_length(subject) > 0 AND char_length(subject) <= 256),
    access_token_ciphertext text
        NOT NULL
        CONSTRAINT auth_oauth2_provider_tokens_access_token_ciphertext_check
        CHECK (
            char_length(access_token_ciphertext) > 0
            AND char_length(access_token_ciphertext) <= 8192
        ),
    -- NULL when the provider issued none (Battle.net's authorization-code flow, as far as known).
    refresh_token_ciphertext text
        CONSTRAINT auth_oauth2_provider_tokens_refresh_token_ciphertext_check
        CHECK (
            refresh_token_ciphertext IS NULL
            OR (
                char_length(refresh_token_ciphertext) > 0
                AND char_length(refresh_token_ciphertext) <= 8192
            )
        ),
    -- NULL when the provider did not say how long the access token lives.
    expires_at timestamp with time zone,
    -- The scopes granted with this token, as the token response reported them (or as requested).
    scope text
        NOT NULL
        CONSTRAINT auth_oauth2_provider_tokens_scope_check
        CHECK (char_length(scope) <= 512),
    updated_at timestamp with time zone NOT NULL DEFAULT now(),

    PRIMARY KEY (provider_id, subject)
);
