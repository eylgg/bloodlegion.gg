DROP TRIGGER characters_updated_at_trig ON characters;
ALTER TABLE characters DROP COLUMN ruleset;
ALTER TABLE guild_settings DROP COLUMN ruleset, DROP COLUMN faction;
CREATE TRIGGER characters_updated_at_trig
    BEFORE UPDATE ON characters
    FOR EACH ROW
    WHEN (
        (OLD.user_id, OLD.first_name, OLD.last_name, OLD.class, OLD.race, OLD.sex, OLD.level,
            OLD.reached_60_at, OLD.is_main)
        IS DISTINCT FROM
        (NEW.user_id, NEW.first_name, NEW.last_name, NEW.class, NEW.race, NEW.sex, NEW.level,
            NEW.reached_60_at, NEW.is_main)
    )
    EXECUTE FUNCTION tf_set_updated_at();
