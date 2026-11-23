/*
# Mains belong to accounts

A main is the account's, not a flag on a character: `users.main_character_id` is the member's main,
or null for none. It must be one of their own characters (a foreign key on the pair), a deleted
character stops being anyone's main, and a character that changes hands stops being its old
player's main (`tf_characters_owner_changed`). Each account's main is the character that was
flagged as it.
*/

ALTER TABLE characters
    ADD CONSTRAINT characters_id_user_id_key UNIQUE (id, user_id);

ALTER TABLE users
    ADD COLUMN main_character_id bigint,
    ADD CONSTRAINT users_main_character_fkey
        FOREIGN KEY (main_character_id, id) REFERENCES characters (id, user_id)
        ON DELETE SET NULL (main_character_id);

CREATE INDEX users_main_character_id_idx ON users (main_character_id);

UPDATE users u
SET main_character_id = c.id
FROM characters c
WHERE c.user_id = u.id AND c.is_main;

CREATE FUNCTION tf_characters_owner_changed() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    UPDATE users SET main_character_id = NULL
    WHERE id = OLD.user_id AND main_character_id = OLD.id;
    RETURN NEW;
END;
$$;

CREATE TRIGGER characters_owner_changed_trig
    BEFORE UPDATE OF user_id ON characters
    FOR EACH ROW
    WHEN (OLD.user_id IS DISTINCT FROM NEW.user_id)
    EXECUTE FUNCTION tf_characters_owner_changed();

DROP TRIGGER characters_updated_at_trig ON characters;

DROP INDEX characters_one_main_per_user_idx;
ALTER TABLE characters
    DROP CONSTRAINT characters_main_owned_check,
    DROP COLUMN is_main;

CREATE TRIGGER characters_updated_at_trig
    BEFORE UPDATE ON characters
    FOR EACH ROW
    WHEN (
        (OLD.user_id, OLD.first_name, OLD.last_name, OLD.class, OLD.race, OLD.sex, OLD.ruleset,
            OLD.level, OLD.reached_60_at)
        IS DISTINCT FROM
        (NEW.user_id, NEW.first_name, NEW.last_name, NEW.class, NEW.race, NEW.sex, NEW.ruleset,
            NEW.level, NEW.reached_60_at)
    )
    EXECUTE FUNCTION tf_set_updated_at();
