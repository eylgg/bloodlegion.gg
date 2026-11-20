ALTER TABLE guild_settings
    ADD COLUMN ruleset text NOT NULL DEFAULT 'pvp'
        CONSTRAINT guild_settings_ruleset_check
        CHECK (ruleset IN ('normal', 'pvp', 'roleplay', 'roleplay-pvp', 'hardcore'));

UPDATE characters
SET ruleset = (SELECT ruleset FROM guild_settings)
WHERE ruleset IS NULL;

ALTER TABLE characters
    ALTER COLUMN ruleset SET DEFAULT 'pvp',
    ALTER COLUMN ruleset SET NOT NULL;
