DROP TRIGGER characters_updated_at_trig ON characters;

ALTER TABLE characters
    ADD COLUMN is_main boolean NOT NULL DEFAULT false,
    ADD CONSTRAINT characters_main_owned_check CHECK (NOT is_main OR user_id IS NOT NULL);

UPDATE characters c
SET is_main = true
FROM users u
WHERE u.main_character_id = c.id;

CREATE UNIQUE INDEX characters_one_main_per_user_idx
    ON characters (user_id)
    WHERE is_main;

CREATE TRIGGER characters_updated_at_trig
    BEFORE UPDATE ON characters
    FOR EACH ROW
    WHEN (
        (OLD.user_id, OLD.first_name, OLD.last_name, OLD.class, OLD.race, OLD.sex, OLD.ruleset,
            OLD.level, OLD.reached_60_at, OLD.is_main)
        IS DISTINCT FROM
        (NEW.user_id, NEW.first_name, NEW.last_name, NEW.class, NEW.race, NEW.sex, NEW.ruleset,
            NEW.level, NEW.reached_60_at, NEW.is_main)
    )
    EXECUTE FUNCTION tf_set_updated_at();

DROP TRIGGER characters_owner_changed_trig ON characters;
DROP FUNCTION tf_characters_owner_changed();

ALTER TABLE users DROP COLUMN main_character_id;
ALTER TABLE characters DROP CONSTRAINT characters_id_user_id_key;
