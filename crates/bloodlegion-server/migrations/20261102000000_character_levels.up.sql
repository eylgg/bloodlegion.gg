/*
# Character levels

Each character's level, 1 to 60, as an officer last recorded it (Forever has no armory to read it
from), and when it reached 60, which settles who got there first: NULL below 60. The characters
already here start at 1.
*/

ALTER TABLE characters
    ADD COLUMN level smallint NOT NULL DEFAULT 1
        CONSTRAINT characters_level_check CHECK (level BETWEEN 1 AND 60),
    ADD COLUMN reached_60_at timestamptz,
    ADD CONSTRAINT characters_reached_60_at_check
        CHECK ((level = 60) = (reached_60_at IS NOT NULL));

DROP TRIGGER characters_updated_at_trig ON characters;
CREATE TRIGGER characters_updated_at_trig
    BEFORE UPDATE ON characters
    FOR EACH ROW
    WHEN (
        (OLD.user_id, OLD.first_name, OLD.last_name, OLD.class, OLD.race, OLD.sex, OLD.level,
            OLD.reached_60_at, OLD.is_main)
        IS DISTINCT FROM
        (NEW.user_id, NEW.first_name, NEW.last_name, NEW.class, NEW.race, NEW.sex, NEW.level,
            NEW.reached_60_at, NEW.is_main)
    )
    EXECUTE FUNCTION tf_set_updated_at();
