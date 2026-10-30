ALTER TABLE bosses DROP COLUMN position;

ALTER TABLE loot_priorities DROP CONSTRAINT loot_priorities_zone_fkey;
ALTER TABLE raids DROP CONSTRAINT raids_zone_fkey;
ALTER TABLE bosses DROP CONSTRAINT bosses_zone_fkey;

CREATE OR REPLACE FUNCTION raid_zone_size(zone text)
    RETURNS integer
    LANGUAGE sql
    IMMUTABLE
AS $$
    SELECT CASE zone
        WHEN 'barrow-deeps' THEN 10
        WHEN 'hyjal-summit' THEN 20
        WHEN 'onyxias-lair' THEN 40
    END;
$$;

ALTER TABLE bosses
    ADD CONSTRAINT bosses_zone_check CHECK (raid_zone_size(zone) IS NOT NULL);
ALTER TABLE raids
    ADD CONSTRAINT raids_zone_check CHECK (raid_zone_size(zone) IS NOT NULL);
ALTER TABLE loot_priorities
    ADD CONSTRAINT loot_priorities_zone_check CHECK (raid_zone_size(zone) IS NOT NULL);

DROP TABLE raid_zones;
