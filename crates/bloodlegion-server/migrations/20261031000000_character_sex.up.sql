/*
# Character sex

Each character's sex, which picks its race's portrait. Required like its class; the characters
already here, and any made without one (a seeded pug), are female until someone says otherwise.
*/

ALTER TABLE characters
    ADD COLUMN sex text NOT NULL DEFAULT 'female'
        CONSTRAINT characters_sex_check CHECK (sex IN ('female', 'male'));

DROP TRIGGER characters_updated_at_trig ON characters;
CREATE TRIGGER characters_updated_at_trig
    BEFORE UPDATE ON characters
    FOR EACH ROW
    WHEN (
        (OLD.user_id, OLD.first_name, OLD.last_name, OLD.class, OLD.race, OLD.sex, OLD.is_main)
        IS DISTINCT FROM
        (NEW.user_id, NEW.first_name, NEW.last_name, NEW.class, NEW.race, NEW.sex, NEW.is_main)
    )
    EXECUTE FUNCTION tf_set_updated_at();
