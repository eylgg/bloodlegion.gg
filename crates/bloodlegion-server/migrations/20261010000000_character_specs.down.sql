-- Back to two slots: the main (else the first by name) is the primary, the next the secondary;
-- any third spec is lost.
DROP TRIGGER characters_updated_at_trig ON characters;
ALTER TABLE characters
    ADD COLUMN primary_spec text
        CONSTRAINT characters_primary_spec_check
        CHECK (primary_spec IS NULL OR (char_length(primary_spec) > 0 AND char_length(primary_spec) <= 32)),
    ADD COLUMN primary_talents text[] NOT NULL DEFAULT '{}'
        CONSTRAINT characters_primary_talents_check
        CHECK (cardinality(primary_talents) <= 32),
    ADD COLUMN secondary_spec text
        CONSTRAINT characters_secondary_spec_check
        CHECK (secondary_spec IS NULL OR (char_length(secondary_spec) > 0 AND char_length(secondary_spec) <= 32)),
    ADD COLUMN secondary_talents text[] NOT NULL DEFAULT '{}'
        CONSTRAINT characters_secondary_talents_check
        CHECK (cardinality(secondary_talents) <= 32);

WITH ranked AS (
    SELECT character_id, spec, talents,
           row_number() OVER (PARTITION BY character_id ORDER BY is_main DESC, spec) AS n
    FROM character_specs
)
UPDATE characters c
SET primary_spec = p.spec, primary_talents = p.talents,
    secondary_spec = s.spec, secondary_talents = COALESCE(s.talents, '{}')
FROM ranked p
LEFT JOIN ranked s ON s.character_id = p.character_id AND s.n = 2
WHERE p.character_id = c.id AND p.n = 1;

ALTER TABLE characters
    ADD CONSTRAINT characters_talents_need_spec_check
    CHECK (
        (primary_spec IS NOT NULL OR cardinality(primary_talents) = 0)
        AND (secondary_spec IS NOT NULL OR cardinality(secondary_talents) = 0)
    );
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

ALTER TABLE raid_attendees ADD COLUMN uses_secondary boolean NOT NULL DEFAULT false;
UPDATE raid_attendees a
SET uses_secondary = true
FROM characters c
WHERE c.id = a.character_id AND a.spec IS NOT NULL AND a.spec = c.secondary_spec;
ALTER TABLE raid_attendees
    DROP CONSTRAINT raid_attendees_character_id_spec_fkey,
    DROP COLUMN spec;

DROP TABLE character_specs;
