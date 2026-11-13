-- Hidden characters (none are).
DROP TRIGGER characters_updated_at_trig ON characters;

ALTER TABLE characters
    ADD COLUMN is_hidden boolean NOT NULL DEFAULT false,
    ADD CONSTRAINT characters_hidden_from_battlenet_check
        CHECK (NOT is_hidden OR battlenet_id IS NOT NULL),
    ADD CONSTRAINT characters_hidden_not_main_check CHECK (NOT (is_hidden AND is_main));

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

-- In the guild as a column.
ALTER TABLE characters ADD COLUMN in_guild boolean NOT NULL DEFAULT false;
UPDATE characters SET in_guild = in_guild(guild);

-- Rank names on characters and uploads.
ALTER TABLE characters DROP CONSTRAINT characters_guild_rank_check;
ALTER TABLE characters RENAME COLUMN guild_rank TO guild_rank_index;
ALTER TABLE characters ADD COLUMN guild_rank text;
UPDATE characters c SET guild_rank = COALESCE(
    (SELECT r.name FROM guild_ranks r WHERE r.guild = c.guild AND r.rank = c.guild_rank_index),
    'Rank ' || (c.guild_rank_index + 1)
)
WHERE c.guild_rank_index IS NOT NULL;

ALTER TABLE roster_members RENAME COLUMN rank TO rank_index;
ALTER TABLE roster_members ADD COLUMN rank_name text;
UPDATE roster_members m SET rank_name = r.name
FROM roster_uploads u, guild_ranks r
WHERE u.id = m.upload_id AND r.guild = u.guild AND r.rank = m.rank_index;

ALTER TABLE guild_ranks RENAME TO guild_ranks_by_number;
CREATE TABLE guild_ranks (
    guild text NOT NULL,
    name text NOT NULL,
    rank_index smallint NOT NULL,
    site_rank text
        CHECK (site_rank IN ('leader', 'officer', 'raider', 'trial', 'member', 'public')),
    seen_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (guild, name)
);
INSERT INTO guild_ranks (guild, name, rank_index, site_rank, seen_at)
SELECT guild, COALESCE(name, 'Rank ' || (rank + 1)), rank, site_rank, seen_at
FROM guild_ranks_by_number;
DROP TABLE guild_ranks_by_number;

ALTER TABLE roster_uploads DROP COLUMN records_ranks;

DROP FUNCTION in_guild(text);
