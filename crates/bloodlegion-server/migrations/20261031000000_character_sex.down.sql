DROP TRIGGER characters_updated_at_trig ON characters;
ALTER TABLE characters DROP COLUMN sex;
CREATE TRIGGER characters_updated_at_trig
    BEFORE UPDATE ON characters
    FOR EACH ROW
    WHEN (
        (OLD.user_id, OLD.first_name, OLD.last_name, OLD.class, OLD.race, OLD.is_main)
        IS DISTINCT FROM
        (NEW.user_id, NEW.first_name, NEW.last_name, NEW.class, NEW.race, NEW.is_main)
    )
    EXECUTE FUNCTION tf_set_updated_at();
