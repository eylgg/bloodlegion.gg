/*
# Any number of specs

A character may play any of its class's specs, each with the notable talents it takes, and at most
one of them is its main (or none: not everyone has decided). This replaces the two fixed slots
(primary and secondary). A raid attendee plays one of the character's specs that night; with none
chosen, the main, or the only one.
*/

CREATE TABLE character_specs (
    character_id bigint
        NOT NULL
        REFERENCES characters(id) ON DELETE CASCADE,
    -- A spec slug of the character's class, as the app's catalog (launch::catalog) checks.
    spec text
        NOT NULL
        CONSTRAINT character_specs_spec_check
        CHECK (char_length(spec) > 0 AND char_length(spec) <= 32),
    talents text[]
        NOT NULL
        DEFAULT '{}'
        CONSTRAINT character_specs_talents_check
        CHECK (cardinality(talents) <= 32),
    is_main boolean NOT NULL DEFAULT false,
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    updated_at timestamp with time zone NOT NULL DEFAULT now(),

    PRIMARY KEY (character_id, spec)
);

-- At most one main per character.
CREATE UNIQUE INDEX character_specs_one_main_idx
    ON character_specs (character_id)
    WHERE is_main;

CREATE TRIGGER character_specs_updated_at_trig
    BEFORE UPDATE ON character_specs
    FOR EACH ROW
    WHEN (OLD.* IS DISTINCT FROM NEW.*)
    EXECUTE FUNCTION tf_set_updated_at();

-- The two slots become rows: the primary is the main.
INSERT INTO character_specs (character_id, spec, talents, is_main)
SELECT id, primary_spec, primary_talents, true
FROM characters
WHERE primary_spec IS NOT NULL;

INSERT INTO character_specs (character_id, spec, talents, is_main)
SELECT id, secondary_spec, secondary_talents, false
FROM characters
WHERE secondary_spec IS NOT NULL
    AND secondary_spec IS DISTINCT FROM primary_spec;

-- The spec each attendee plays that night, NULL for their main (or only) one. Removing a spec
-- from the character clears it here rather than removing them from the raid: the composite key's
-- SET NULL names just the spec column (PG15+).
ALTER TABLE raid_attendees
    ADD COLUMN spec text,
    ADD CONSTRAINT raid_attendees_character_id_spec_fkey
        FOREIGN KEY (character_id, spec)
        REFERENCES character_specs (character_id, spec)
        ON DELETE SET NULL (spec);

UPDATE raid_attendees a
SET spec = c.secondary_spec
FROM characters c
WHERE c.id = a.character_id
    AND a.uses_secondary
    AND c.secondary_spec IS NOT NULL
    AND c.secondary_spec IS DISTINCT FROM c.primary_spec;

ALTER TABLE raid_attendees DROP COLUMN uses_secondary;

DROP TRIGGER characters_updated_at_trig ON characters;
ALTER TABLE characters
    DROP CONSTRAINT characters_talents_need_spec_check,
    DROP COLUMN primary_spec,
    DROP COLUMN primary_talents,
    DROP COLUMN secondary_spec,
    DROP COLUMN secondary_talents;
CREATE TRIGGER characters_updated_at_trig
    BEFORE UPDATE ON characters
    FOR EACH ROW
    WHEN (
        (OLD.user_id, OLD.first_name, OLD.last_name, OLD.class, OLD.is_main)
        IS DISTINCT FROM
        (NEW.user_id, NEW.first_name, NEW.last_name, NEW.class, NEW.is_main)
    )
    EXECUTE FUNCTION tf_set_updated_at();
