DROP TRIGGER characters_updated_at_trig ON characters;
ALTER TABLE characters DROP COLUMN race;
CREATE TRIGGER characters_updated_at_trig
    BEFORE UPDATE ON characters
    FOR EACH ROW
    WHEN (
        (OLD.user_id, OLD.first_name, OLD.last_name, OLD.class, OLD.is_main)
        IS DISTINCT FROM
        (NEW.user_id, NEW.first_name, NEW.last_name, NEW.class, NEW.is_main)
    )
    EXECUTE FUNCTION tf_set_updated_at();
