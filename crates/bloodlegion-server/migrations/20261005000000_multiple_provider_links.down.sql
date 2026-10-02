-- Fails while any account has two active credentials at one provider; unlink the extras first.
CREATE UNIQUE INDEX auth_oauth2_provider_credentials_active_user_idx
    ON auth_oauth2_provider_credentials (provider_id, user_id)
    WHERE disconnected_at IS NULL;

COMMENT ON INDEX auth_oauth2_provider_credentials_active_user_idx IS
    'At most one active (non-disconnected) credential per (provider, user). Partial unique '
    'indexes are non-deferrable, so reconnecting identity A while identity B is still active '
    'for the same (provider, user) requires two statements: disconnect B first '
    '(SET disconnected_at = now()), then reconnect A (SET disconnected_at = NULL).';
