/*
# Raid times in local time

A raid's start is kept as the guild's clock reads it (8 PM) in a named time zone, rather than as an
instant: a raid night is a local event, and a time zone change on the guild's side should not move
raids already scheduled. The instant is derived (`starts_at`, generated), so ordering, the weekly
lockouts, and the loot filters keep working on it.

The guild's settings say which zone new raids are in and when they usually start.
*/

-- One row, keyed on a boolean sentinel (see the initial migration's conventions).
CREATE TABLE guild_settings (
    singleton boolean
        PRIMARY KEY
        DEFAULT true
        CONSTRAINT guild_settings_singleton_check
        CHECK (singleton),
    -- An IANA zone name the database knows (`America/New_York`); the app checks it against
    -- pg_timezone_names, which a CHECK cannot query.
    time_zone text
        NOT NULL
        DEFAULT 'America/New_York'
        CONSTRAINT guild_settings_time_zone_check
        CHECK (char_length(time_zone) > 0 AND char_length(time_zone) <= 64),
    -- When a raid starts unless the officer scheduling it says otherwise, in `time_zone`.
    default_raid_time time without time zone NOT NULL DEFAULT '20:00',
    updated_at timestamp with time zone NOT NULL DEFAULT now()
);

INSERT INTO guild_settings DEFAULT VALUES;

CREATE TRIGGER guild_settings_prevent_deletion_trig
    BEFORE DELETE ON guild_settings
    FOR EACH ROW
    EXECUTE FUNCTION tf_prevent_deletion();

CREATE TRIGGER guild_settings_updated_at_trig
    BEFORE UPDATE ON guild_settings
    FOR EACH ROW
    WHEN (OLD.* IS DISTINCT FROM NEW.*)
    EXECUTE FUNCTION tf_set_updated_at();

-- Existing raids were entered as instants; read them on New York's clock, the guild's zone.
ALTER TABLE raids
    ADD COLUMN starts_local timestamp without time zone,
    ADD COLUMN time_zone text
        CONSTRAINT raids_time_zone_check
        CHECK (char_length(time_zone) > 0 AND char_length(time_zone) <= 64);

UPDATE raids
SET starts_local = starts_at AT TIME ZONE 'America/New_York',
    time_zone = 'America/New_York';

ALTER TABLE raids
    ALTER COLUMN starts_local SET NOT NULL,
    ALTER COLUMN time_zone SET NOT NULL;

-- A BEFORE trigger's WHEN cannot reference generated columns, so the updated_at trigger names the
-- stored ones (as users_updated_at_trig does).
DROP TRIGGER raids_updated_at_trig ON raids;
DROP INDEX raids_starts_at_idx;
ALTER TABLE raids DROP COLUMN starts_at;

-- STORED is load-bearing: PG18 defaults generated columns to VIRTUAL, which cannot be indexed.
-- timezone(text, timestamp) is immutable, as a generated column needs.
ALTER TABLE raids
    ADD COLUMN starts_at timestamp with time zone
        NOT NULL
        GENERATED ALWAYS AS (starts_local AT TIME ZONE time_zone) STORED;

CREATE INDEX raids_starts_at_idx
    ON raids (starts_at);

CREATE TRIGGER raids_updated_at_trig
    BEFORE UPDATE ON raids
    FOR EACH ROW
    WHEN (
        (OLD.zone, OLD.title, OLD.starts_local, OLD.time_zone)
        IS DISTINCT FROM
        (NEW.zone, NEW.title, NEW.starts_local, NEW.time_zone)
    )
    EXECUTE FUNCTION tf_set_updated_at();
