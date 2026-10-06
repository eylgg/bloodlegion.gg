DROP TRIGGER raids_repack_lost_groups_trig ON raids;
DROP FUNCTION tf_raids_repack_lost_groups();

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

ALTER TABLE raid_attendees
    DROP CONSTRAINT raid_attendees_raid_id_group_number_slot_key,
    ALTER COLUMN group_number DROP NOT NULL,
    ALTER COLUMN slot DROP NOT NULL,
    ADD CONSTRAINT raid_attendees_placed_check
        CHECK ((group_number IS NULL) = (slot IS NULL)),
    ADD CONSTRAINT raid_attendees_raid_id_group_number_slot_key
        UNIQUE (raid_id, group_number, slot);

DROP FUNCTION raid_free_slot(bigint);

DROP TRIGGER raids_updated_at_trig ON raids;
ALTER TABLE raids
    ADD COLUMN title text
        CONSTRAINT raids_title_check
        CHECK (title IS NULL OR (char_length(title) > 0 AND char_length(title) <= 64));
CREATE TRIGGER raids_updated_at_trig
    BEFORE UPDATE ON raids
    FOR EACH ROW
    WHEN (
        (OLD.zone, OLD.title, OLD.starts_local, OLD.time_zone)
        IS DISTINCT FROM
        (NEW.zone, NEW.title, NEW.starts_local, NEW.time_zone)
    )
    EXECUTE FUNCTION tf_set_updated_at();
