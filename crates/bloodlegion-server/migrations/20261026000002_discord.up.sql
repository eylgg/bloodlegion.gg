/*
# Discord

The guild's Discord server, run by the bot (the `discord` module). The bot keeps the ids of what it
made, so renaming a role or category by hand neither duplicates it nor loses track of it, and it
mirrors the public #announcements channel for the site. A member's rank, being disabled, or their
linked Discord account changing tells the bot (`discord_roles`) to bring their roles in line.
*/

-- How the bot reaches the server, which a superuser sets on the site: the server's id, and the bot's
-- token, encrypted with ENCRYPTION_KEY like the providers' client secrets. Without both there is no
-- bot; changing either restarts it (`discord_settings`).
CREATE TABLE discord_settings (
    singleton boolean
        PRIMARY KEY
        DEFAULT true
        CONSTRAINT discord_settings_singleton_check
        CHECK (singleton),
    guild_id bigint
        CONSTRAINT discord_settings_guild_id_check
        CHECK (guild_id > 0),
    bot_token_ciphertext text
        CONSTRAINT discord_settings_bot_token_ciphertext_check
        CHECK (char_length(bot_token_ciphertext) > 0),
    updated_at timestamp with time zone NOT NULL DEFAULT now()
);

INSERT INTO discord_settings DEFAULT VALUES;

CREATE TRIGGER discord_settings_prevent_deletion_trig
    BEFORE DELETE ON discord_settings
    FOR EACH ROW
    EXECUTE FUNCTION tf_prevent_deletion();

CREATE TRIGGER discord_settings_updated_at_trig
    BEFORE UPDATE ON discord_settings
    FOR EACH ROW
    WHEN (OLD.* IS DISTINCT FROM NEW.*)
    EXECUTE FUNCTION tf_set_updated_at();

CREATE FUNCTION tf_notify_discord_settings()
    RETURNS trigger
    LANGUAGE plpgsql
AS $$
BEGIN
    PERFORM pg_notify('discord_settings', '');
    RETURN NULL;
END;
$$;

CREATE TRIGGER discord_settings_notify_trig
    AFTER UPDATE ON discord_settings
    FOR EACH ROW
    WHEN (
        (OLD.guild_id, OLD.bot_token_ciphertext)
        IS DISTINCT FROM (NEW.guild_id, NEW.bot_token_ciphertext)
    )
    EXECUTE FUNCTION tf_notify_discord_settings();

-- What the bot made in the server, by its own key (`role:officer`, `category:public`, ...).
CREATE TABLE discord_objects (
    key text
        PRIMARY KEY
        CONSTRAINT discord_objects_key_check
        CHECK (char_length(key) BETWEEN 1 AND 64),
    discord_id bigint NOT NULL
);

-- The posts in #announcements, for the site. `content` is the Discord Markdown as written, rendered
-- to HTML when read; `mentions` names the users, roles, and channels it mentions by id, as they
-- were named when it was posted or last edited (`{"users": {"<id>": "<name>"}, ...}`).
CREATE TABLE discord_announcements (
    message_id bigint PRIMARY KEY,
    author text NOT NULL,
    content text NOT NULL,
    mentions jsonb
        NOT NULL
        DEFAULT '{}'
        CONSTRAINT discord_announcements_mentions_check
        CHECK (mentions IS JSON OBJECT),
    posted_at timestamp with time zone NOT NULL,
    edited_at timestamp with time zone
);

CREATE INDEX discord_announcements_posted_at_idx ON discord_announcements (posted_at DESC);

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
