/*
# Rulesets

WoW: Forever has rulesets instead of realms: Normal, PvP, Roleplay, RP-PvP, and Hardcore (once it
opens). The guild plays one faction on one ruleset, settings that start as Horde on PvP, and each
character records its ruleset, the guild's unless an officer says otherwise. The characters
already here are PvP, the guild's.
*/

ALTER TABLE guild_settings
    ADD COLUMN faction text NOT NULL DEFAULT 'horde'
        CONSTRAINT guild_settings_faction_check CHECK (faction IN ('alliance', 'horde')),
    ADD COLUMN ruleset text NOT NULL DEFAULT 'pvp'
        CONSTRAINT guild_settings_ruleset_check
        CHECK (ruleset IN ('normal', 'pvp', 'roleplay', 'roleplay-pvp', 'hardcore'));

ALTER TABLE characters
    ADD COLUMN ruleset text NOT NULL DEFAULT 'pvp'
        CONSTRAINT characters_ruleset_check
        CHECK (ruleset IN ('normal', 'pvp', 'roleplay', 'roleplay-pvp', 'hardcore'));

DROP TRIGGER characters_updated_at_trig ON characters;
CREATE TRIGGER characters_updated_at_trig
    BEFORE UPDATE ON characters
    FOR EACH ROW
    WHEN (
        (OLD.user_id, OLD.first_name, OLD.last_name, OLD.class, OLD.race, OLD.sex, OLD.ruleset,
            OLD.level, OLD.reached_60_at, OLD.is_main, OLD.is_hidden)
        IS DISTINCT FROM
        (NEW.user_id, NEW.first_name, NEW.last_name, NEW.class, NEW.race, NEW.sex, NEW.ruleset,
            NEW.level, NEW.reached_60_at, NEW.is_main, NEW.is_hidden)
    )
    EXECUTE FUNCTION tf_set_updated_at();
