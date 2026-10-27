/*
# Discord bot status

What the bot last said about itself, for the role check page: who it signed in as, since when it
has run, when it was last seen (the instance running it writes every few seconds), when it last
checked its roles and categories, and the last error that stopped it.
*/

CREATE TABLE discord_status (
    singleton boolean
        PRIMARY KEY
        DEFAULT true
        CONSTRAINT discord_status_singleton_check
        CHECK (singleton),
    bot_user text,
    running_since timestamp with time zone,
    seen_at timestamp with time zone,
    checked_at timestamp with time zone,
    last_error text,
    last_error_at timestamp with time zone
);

INSERT INTO discord_status DEFAULT VALUES;

CREATE TRIGGER discord_status_prevent_deletion_trig
    BEFORE DELETE ON discord_status
    FOR EACH ROW
    EXECUTE FUNCTION tf_prevent_deletion();
