DROP TRIGGER IF EXISTS raids_bench_lost_groups_trig ON raids;
DROP FUNCTION IF EXISTS tf_raids_bench_lost_groups();
DROP TRIGGER IF EXISTS raid_attendees_group_in_zone_trig ON raid_attendees;
DROP FUNCTION IF EXISTS tf_raid_attendees_group_in_zone();
ALTER TABLE raid_attendees
    DROP CONSTRAINT raid_attendees_raid_id_group_number_slot_key,
    DROP CONSTRAINT raid_attendees_placed_check,
    DROP COLUMN uses_secondary,
    DROP COLUMN slot,
    DROP COLUMN group_number;

DROP TRIGGER characters_updated_at_trig ON characters;
CREATE TRIGGER characters_updated_at_trig
    BEFORE UPDATE ON characters
    FOR EACH ROW
    WHEN (
        (OLD.user_id, OLD.first_name, OLD.last_name, OLD.class, OLD.is_main)
        IS DISTINCT FROM
        (NEW.user_id, NEW.first_name, NEW.last_name, NEW.class, NEW.is_main)
    )
    EXECUTE FUNCTION tf_set_updated_at();
ALTER TABLE characters
    DROP CONSTRAINT characters_talents_need_spec_check,
    DROP COLUMN secondary_talents,
    DROP COLUMN secondary_spec,
    DROP COLUMN primary_talents,
    DROP COLUMN primary_spec;
