/*
# The raid builder

Characters have two specs (Forever has dual spec), each with the notable talents it takes (the
ones that change what the character brings a raid: Blessing of Kings, Shadow Weaving). A raid's
attendees are placed in its groups of five, each playing one of their two specs that night.

Specs and talents are slugs from the app's class catalog (launch::catalog), which checks them.
*/

ALTER TABLE characters
    ADD COLUMN primary_spec text
        CONSTRAINT characters_primary_spec_check
        CHECK (primary_spec IS NULL OR (char_length(primary_spec) > 0 AND char_length(primary_spec) <= 32)),
    ADD COLUMN primary_talents text[]
        NOT NULL
        DEFAULT '{}'
        CONSTRAINT characters_primary_talents_check
        CHECK (cardinality(primary_talents) <= 32),
    ADD COLUMN secondary_spec text
        CONSTRAINT characters_secondary_spec_check
        CHECK (secondary_spec IS NULL OR (char_length(secondary_spec) > 0 AND char_length(secondary_spec) <= 32)),
    ADD COLUMN secondary_talents text[]
        NOT NULL
        DEFAULT '{}'
        CONSTRAINT characters_secondary_talents_check
        CHECK (cardinality(secondary_talents) <= 32);

-- A character's talents belong to a spec it has.
ALTER TABLE characters
    ADD CONSTRAINT characters_talents_need_spec_check
    CHECK (
        (primary_spec IS NOT NULL OR cardinality(primary_talents) = 0)
        AND (secondary_spec IS NOT NULL OR cardinality(secondary_talents) = 0)
    );

-- Start from what members said at launch sign-up: the specs they hoped to play in that class.
UPDATE characters c
SET primary_spec = l.specs[1],
    secondary_spec = l.specs[2]
FROM launch_characters l
WHERE l.user_id = c.user_id
    AND l.class = c.class;

DROP TRIGGER characters_updated_at_trig ON characters;
CREATE TRIGGER characters_updated_at_trig
    BEFORE UPDATE ON characters
    FOR EACH ROW
    WHEN (
        (OLD.user_id, OLD.first_name, OLD.last_name, OLD.class, OLD.is_main, OLD.primary_spec,
         OLD.primary_talents, OLD.secondary_spec, OLD.secondary_talents)
        IS DISTINCT FROM
        (NEW.user_id, NEW.first_name, NEW.last_name, NEW.class, NEW.is_main, NEW.primary_spec,
         NEW.primary_talents, NEW.secondary_spec, NEW.secondary_talents)
    )
    EXECUTE FUNCTION tf_set_updated_at();

-- Where each attendee stands: a group (1 to the zone's size over five) and a slot in it (1 to 5),
-- or neither, on the bench. Which of their specs they play that night.
ALTER TABLE raid_attendees
    ADD COLUMN group_number smallint,
    ADD COLUMN slot smallint
        CONSTRAINT raid_attendees_slot_check
        CHECK (slot IS NULL OR (slot >= 1 AND slot <= 5)),
    ADD COLUMN uses_secondary boolean NOT NULL DEFAULT false,
    ADD CONSTRAINT raid_attendees_placed_check
        CHECK ((group_number IS NULL) = (slot IS NULL)),
    ADD CONSTRAINT raid_attendees_raid_id_group_number_slot_key
        UNIQUE (raid_id, group_number, slot);

-- A group the raid's zone has.
CREATE FUNCTION tf_raid_attendees_group_in_zone()
    RETURNS trigger
    LANGUAGE plpgsql
AS $$
BEGIN
    IF NEW.group_number IS NOT NULL
        AND (
            NEW.group_number < 1
            OR NEW.group_number > (SELECT raid_zone_size(zone) / 5 FROM raids WHERE id = NEW.raid_id)
        )
    THEN
        RAISE EXCEPTION 'raid % has no group %', NEW.raid_id, NEW.group_number
            USING ERRCODE = 'check_violation',
                  CONSTRAINT = 'raid_attendees_group_in_zone_check';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER raid_attendees_group_in_zone_trig
    BEFORE INSERT OR UPDATE OF group_number ON raid_attendees
    FOR EACH ROW
    EXECUTE FUNCTION tf_raid_attendees_group_in_zone();

-- Moving a raid to a smaller zone benches whoever stood in a group it no longer has.
CREATE FUNCTION tf_raids_bench_lost_groups()
    RETURNS trigger
    LANGUAGE plpgsql
AS $$
BEGIN
    UPDATE raid_attendees
    SET group_number = NULL, slot = NULL
    WHERE raid_id = NEW.id
        AND group_number > raid_zone_size(NEW.zone) / 5;
    RETURN NULL;
END;
$$;

CREATE TRIGGER raids_bench_lost_groups_trig
    AFTER UPDATE OF zone ON raids
    FOR EACH ROW
    WHEN (OLD.zone IS DISTINCT FROM NEW.zone)
    EXECUTE FUNCTION tf_raids_bench_lost_groups();
