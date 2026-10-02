-- A member may link several accounts at one provider: most people with more than one Battle.net
-- account keep characters on each. The partial index allowed one active credential per (provider,
-- user); without it, (provider, subject) stays unique, so an upstream identity still belongs to at
-- most one account. Unlinking deletes the credential rather than tombstoning it, which frees the
-- identity to be linked again (to this account or another one).
DROP INDEX auth_oauth2_provider_credentials_active_user_idx;
