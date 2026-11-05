DROP TRIGGER loot_priority_characters_character_not_hidden_trig ON loot_priority_characters;
DROP TRIGGER loot_character_not_hidden_trig ON loot;
DROP TRIGGER raid_attendees_character_not_hidden_trig ON raid_attendees;
DROP FUNCTION tf_character_not_hidden();
DELETE FROM characters WHERE is_hidden;
ALTER TABLE characters
    DROP CONSTRAINT characters_hidden_from_battlenet_check,
    DROP CONSTRAINT characters_hidden_not_main_check,
    DROP CONSTRAINT characters_battlenet_account_fkey,
    DROP COLUMN battlenet_seen_at,
    DROP COLUMN battlenet_subject,
    DROP COLUMN battlenet_provider_id,
    DROP COLUMN battlenet_id,
    DROP COLUMN is_hidden;
