DROP TABLE IF EXISTS question_answers;
DROP TABLE IF EXISTS questions;
DROP TABLE IF EXISTS loot;
DROP FUNCTION IF EXISTS tf_loot_boss_zone_consistent();
DROP TABLE IF EXISTS raid_attendees;
DROP FUNCTION IF EXISTS tf_raid_attendees_within_size();
DROP TABLE IF EXISTS raids;
DROP FUNCTION IF EXISTS tf_raids_within_size();
DROP FUNCTION IF EXISTS tf_raids_zone_loot_consistent();
DROP TABLE IF EXISTS items;
DROP TABLE IF EXISTS bosses;
DROP FUNCTION IF EXISTS raid_zone_size(text);
DROP TABLE IF EXISTS character_notes;
DROP TABLE IF EXISTS characters;

DROP TRIGGER users_updated_at_trig ON users;
CREATE TRIGGER users_updated_at_trig
    BEFORE UPDATE ON users
    FOR EACH ROW
    WHEN (
        (OLD.username, OLD.is_superuser, OLD.first_name, OLD.last_name, OLD.disabled_at)
        IS DISTINCT FROM
        (NEW.username, NEW.is_superuser, NEW.first_name, NEW.last_name, NEW.disabled_at)
    )
    EXECUTE FUNCTION tf_set_updated_at();
ALTER TABLE users DROP COLUMN guild_rank;
