/*
# Groups only

A raid is its zone and its start: raids of one zone in a week are told apart by order, not by a
title. Everyone on a raid stands in one of its groups; there is no bench (the roster beside the
raid is where people wait).
*/

DROP TRIGGER raids_updated_at_trig ON raids;
ALTER TABLE raids DROP COLUMN title;
CREATE TRIGGER raids_updated_at_trig
    BEFORE UPDATE ON raids
    FOR EACH ROW
    WHEN (
        (OLD.zone, OLD.starts_local, OLD.time_zone)
        IS DISTINCT FROM
        (NEW.zone, NEW.starts_local, NEW.time_zone)
    )
    EXECUTE FUNCTION tf_set_updated_at();

-- The raid's first free place, group by group, slot by slot; none when every group is full.
CREATE FUNCTION raid_free_slot(raid bigint)
    RETURNS TABLE (group_number smallint, slot smallint)
    LANGUAGE sql
    STABLE
AS $$
    SELECT g::smallint, s::smallint
    FROM raids r,
         generate_series(1, raid_zone_size(r.zone) / 5) AS g,
         generate_series(1, 5) AS s
    WHERE r.id = raid
        AND NOT EXISTS (
            SELECT 1 FROM raid_attendees a
            WHERE a.raid_id = raid AND a.group_number = g AND a.slot = s
        )
    ORDER BY g, s
    LIMIT 1;
$$;

-- Everyone benched takes the first free place (a raid holds at most its zone's size, so there is
-- always one).
DO $$
DECLARE
    benched record;
BEGIN
    FOR benched IN
        SELECT raid_id, character_id FROM raid_attendees
        WHERE group_number IS NULL
        ORDER BY raid_id, created_at, character_id
    LOOP
        UPDATE raid_attendees a
        SET (group_number, slot) = (SELECT f.group_number, f.slot FROM raid_free_slot(benched.raid_id) f)
        WHERE a.raid_id = benched.raid_id AND a.character_id = benched.character_id;
    END LOOP;
END;
$$;

-- Swapping two people is two updates; the slot stays unique as of the commit.
ALTER TABLE raid_attendees
    DROP CONSTRAINT raid_attendees_placed_check,
    DROP CONSTRAINT raid_attendees_raid_id_group_number_slot_key,
    ALTER COLUMN group_number SET NOT NULL,
    ALTER COLUMN slot SET NOT NULL,
    ADD CONSTRAINT raid_attendees_raid_id_group_number_slot_key
        UNIQUE (raid_id, group_number, slot) DEFERRABLE INITIALLY DEFERRED;

-- Moving a raid to a smaller zone repacks whoever stood in a group it no longer has into the
-- groups it keeps (the size check has made sure they fit).
DROP TRIGGER raids_bench_lost_groups_trig ON raids;
DROP FUNCTION tf_raids_bench_lost_groups();

CREATE FUNCTION tf_raids_repack_lost_groups()
    RETURNS trigger
    LANGUAGE plpgsql
AS $$
DECLARE
    lost record;
BEGIN
    FOR lost IN
        SELECT character_id FROM raid_attendees
        WHERE raid_id = NEW.id AND group_number > raid_zone_size(NEW.zone) / 5
        ORDER BY group_number, slot
    LOOP
        UPDATE raid_attendees a
        SET (group_number, slot) = (SELECT f.group_number, f.slot FROM raid_free_slot(NEW.id) f)
        WHERE a.raid_id = NEW.id AND a.character_id = lost.character_id;
    END LOOP;
    RETURN NULL;
END;
$$;

CREATE TRIGGER raids_repack_lost_groups_trig
    AFTER UPDATE OF zone ON raids
    FOR EACH ROW
    WHEN (OLD.zone IS DISTINCT FROM NEW.zone)
    EXECUTE FUNCTION tf_raids_repack_lost_groups();
