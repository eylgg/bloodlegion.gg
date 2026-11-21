/*
# Placeholder ranks for Discord accounts

A superuser can say what rank someone in the Discord server has before they have linked their
Discord account to a site account (or have a site account at all): a placeholder, kept by the
Discord user's id. The bot's role checks and fixes count it as that person's rank, so they keep (or
are given) its role.

When the Discord account is linked to a site account, by its member or by a superuser, the account
takes the placeholder's rank if it is higher than its own (a new account is public, so it always
is; linking never lowers a rank), and the placeholder goes: the account's rank is what counts from
then on.

No placeholder is for the public (that is having none), nor for the leader or an officer, whose
roles run the server: they need an account.
*/

CREATE TABLE discord_placeholders (
    -- The Discord user's id, as a linked account's subject has it.
    discord_user_id text
        PRIMARY KEY
        CONSTRAINT discord_placeholders_discord_user_id_check
        CHECK (discord_user_id ~ '^[1-9][0-9]{0,19}$'),
    rank text
        NOT NULL
        CONSTRAINT discord_placeholders_rank_check
        CHECK (rank IN ('raider', 'trial', 'member', 'friend')),
    -- How Discord named them when it was set, for a list that names them even after they leave.
    discord_name text
        NOT NULL,
    created_by bigint
        REFERENCES users(id) ON DELETE SET NULL,
    created_at timestamptz
        NOT NULL
        DEFAULT now()
);

CREATE FUNCTION tf_apply_discord_placeholder() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
    ranks constant text[] :=
        ARRAY['leader', 'officer', 'raider', 'trial', 'member', 'friend', 'public'];
    placeholder text;
BEGIN
    IF NEW.disconnected_at IS NOT NULL OR NOT EXISTS (
        SELECT 1 FROM auth_providers WHERE id = NEW.provider_id AND slug = 'discord'
    ) THEN
        RETURN NEW;
    END IF;
    DELETE FROM discord_placeholders
    WHERE discord_user_id = NEW.subject
    RETURNING rank INTO placeholder;
    IF placeholder IS NOT NULL THEN
        UPDATE users
        SET guild_rank = placeholder
        WHERE id = NEW.user_id
          AND array_position(ranks, placeholder) < array_position(ranks, guild_rank);
    END IF;
    RETURN NEW;
END
$$;

CREATE TRIGGER auth_oauth2_provider_credentials_discord_placeholder_trig
    AFTER INSERT OR UPDATE OF disconnected_at, user_id ON auth_oauth2_provider_credentials
    FOR EACH ROW
    EXECUTE FUNCTION tf_apply_discord_placeholder();
