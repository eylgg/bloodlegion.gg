/*
# Battle.net characters

Every Battle.net sign-in reads the account's WoW: Forever characters, and each is a character
here like any other. One that matches a site character by name is linked to it (and goes to the
account's member when nobody plays it yet); any other is added hidden: only superusers see it,
until an officer adds a character of that name, which brings it out instead. Each sign-in brings
the hidden ones up to date and removes those Battle.net no longer lists.
*/

ALTER TABLE characters
    -- Seen by superusers only: from a Battle.net listing, not yet on the site's roster.
    ADD COLUMN is_hidden boolean NOT NULL DEFAULT false,
    -- Battle.net's id for the character, and the account (sign-in identity) that last listed it.
    ADD COLUMN battlenet_id bigint
        CONSTRAINT characters_battlenet_id_key UNIQUE,
    ADD COLUMN battlenet_provider_id bigint,
    ADD COLUMN battlenet_subject text,
    ADD COLUMN battlenet_seen_at timestamp with time zone,
    ADD CONSTRAINT characters_battlenet_account_fkey
        FOREIGN KEY (battlenet_provider_id, battlenet_subject)
        REFERENCES auth_oauth2_provider_tokens(provider_id, subject) ON DELETE SET NULL,
    ADD CONSTRAINT characters_hidden_not_main_check CHECK (NOT (is_hidden AND is_main)),
    ADD CONSTRAINT characters_hidden_from_battlenet_check
        CHECK (NOT is_hidden OR battlenet_id IS NOT NULL);

CREATE INDEX characters_battlenet_account_idx
    ON characters (battlenet_provider_id, battlenet_subject);

-- A hidden character is in no raid, loot, or loot priority: officers never see it to put it there.
CREATE FUNCTION tf_character_not_hidden()
    RETURNS trigger
    LANGUAGE plpgsql
AS $$
BEGIN
    IF NEW.character_id IS NOT NULL
        AND EXISTS (SELECT 1 FROM characters WHERE id = NEW.character_id AND is_hidden) THEN
        RAISE EXCEPTION 'character % is hidden', NEW.character_id
            USING ERRCODE = 'foreign_key_violation',
                  CONSTRAINT = TG_TABLE_NAME || '_character_id_fkey';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER raid_attendees_character_not_hidden_trig
    BEFORE INSERT OR UPDATE OF character_id ON raid_attendees
    FOR EACH ROW EXECUTE FUNCTION tf_character_not_hidden();
CREATE TRIGGER loot_character_not_hidden_trig
    BEFORE INSERT OR UPDATE OF character_id ON loot
    FOR EACH ROW EXECUTE FUNCTION tf_character_not_hidden();
CREATE TRIGGER loot_priority_characters_character_not_hidden_trig
    BEFORE INSERT OR UPDATE OF character_id ON loot_priority_characters
    FOR EACH ROW EXECUTE FUNCTION tf_character_not_hidden();
