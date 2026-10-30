/*
# Raid zones and bosses follow the data file

The raid zones and their bosses are a file in the repository (`data/raids.json`), synced on every
migrate like the item snapshot (`raids::catalog::sync`), rather than written into this schema or
entered by officers.

- `raid_zones` holds each zone's name and size; `raid_zone_size()` reads it, so the database still
  caps a raid's attendance, and the zone columns that checked the old hard-coded list reference it.
  The zones are inserted here as well, so the rows already present satisfy the references; the sync
  keeps them up to date from then on.
- `position` is a zone's place in the file (smallest first), and a boss's in its zone's encounter
  order. A zone or boss the file no longer has, kept because something refers to it, has none and
  sorts last.
*/

CREATE TABLE raid_zones (
    slug text
        PRIMARY KEY
        CONSTRAINT raid_zones_slug_check
        CHECK (slug ~ '^[a-z0-9]+(-[a-z0-9]+)*$'),
    name text
        NOT NULL
        CONSTRAINT raid_zones_name_check
        CHECK (char_length(name) > 0 AND char_length(name) <= 64),
    -- Whole groups of five.
    size integer
        NOT NULL
        CONSTRAINT raid_zones_size_check
        CHECK (size > 0 AND size % 5 = 0),
    position integer
);

INSERT INTO raid_zones (slug, name, size, position) VALUES
    ('barrow-deeps', 'Barrow Deeps', 10, 0),
    ('hyjal-summit', 'Hyjal Summit', 20, 1),
    ('onyxias-lair', 'Onyxia''s Lair', 40, 2);

ALTER TABLE bosses DROP CONSTRAINT bosses_zone_check;
ALTER TABLE raids DROP CONSTRAINT raids_zone_check;
ALTER TABLE loot_priorities DROP CONSTRAINT loot_priorities_zone_check;

-- The size of a zone, NULL for one the database does not have.
CREATE OR REPLACE FUNCTION raid_zone_size(zone text)
    RETURNS integer
    LANGUAGE sql
    STABLE
AS $$
    SELECT size FROM raid_zones WHERE slug = zone;
$$;

ALTER TABLE bosses
    ADD CONSTRAINT bosses_zone_fkey FOREIGN KEY (zone) REFERENCES raid_zones(slug);
ALTER TABLE raids
    ADD CONSTRAINT raids_zone_fkey FOREIGN KEY (zone) REFERENCES raid_zones(slug);
ALTER TABLE loot_priorities
    ADD CONSTRAINT loot_priorities_zone_fkey FOREIGN KEY (zone) REFERENCES raid_zones(slug);

ALTER TABLE bosses ADD COLUMN position integer;
