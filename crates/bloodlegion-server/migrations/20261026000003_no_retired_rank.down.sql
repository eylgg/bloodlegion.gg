-- Who was retired is not kept: they stay members.
ALTER TABLE users DROP CONSTRAINT users_guild_rank_check;
ALTER TABLE users
    ADD CONSTRAINT users_guild_rank_check
    CHECK (guild_rank IN ('leader', 'officer', 'raider', 'trial', 'member', 'friend', 'retired'));

CREATE FUNCTION tf_notify_discord_roles()
    RETURNS trigger
    LANGUAGE plpgsql
AS $$
BEGIN
    PERFORM pg_notify('discord_roles', '');
    RETURN NULL;
END;
$$;

CREATE TRIGGER users_discord_roles_trig
    AFTER UPDATE OF guild_rank, disabled_at ON users
    FOR EACH ROW
    WHEN (
        (OLD.guild_rank, OLD.disabled_at) IS DISTINCT FROM (NEW.guild_rank, NEW.disabled_at)
    )
    EXECUTE FUNCTION tf_notify_discord_roles();

CREATE TRIGGER auth_oauth2_provider_credentials_discord_roles_trig
    AFTER INSERT OR DELETE OR UPDATE OF disconnected_at ON auth_oauth2_provider_credentials
    FOR EACH STATEMENT
    EXECUTE FUNCTION tf_notify_discord_roles();
