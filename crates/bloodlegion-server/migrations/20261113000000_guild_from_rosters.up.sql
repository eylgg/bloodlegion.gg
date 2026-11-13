/*
# The guild from roster uploads

What the guild's latest roster upload saw is all the site keeps of who is in the guild:

- **In the guild** is a character's guild being the one in the settings (`in_guild(guild)`), not a
  column of its own. Renaming the guild there takes everyone out until an upload of the new name.
- **A guild comes with its rank**: `characters.guild` and `characters.guild_rank` are both set or
  both null, and the rank is the game's number for it (0 the guild master), as the armory and
  Blizzard's guild roster give it. Only uploads that sent every member's rank (`records_ranks`,
  the addon from 0.5.0) set them, so the characters older uploads put in the guild without a rank
  are out of it until the next upload.
- **Rank names are the guild's own**, which only the game shows (the addon sends them): kept once
  per rank in `guild_ranks`, keyed by number, with the site rank each stands for, so renaming a
  rank in game keeps its match. Uploads keep only each member's number (`roster_members.rank`).
- **No hidden characters**: a character a Battle.net sign-in listed that the site did not have was
  kept off every page. It is now just its player's, on their page and off the roster until an
  upload names it; the hidden ones there are come out as such.
*/

CREATE FUNCTION in_guild(guild text) RETURNS boolean
LANGUAGE sql STABLE
AS $$ SELECT COALESCE(guild = (SELECT guild_name FROM guild_settings), false) $$;

-- The uploads that sent every member's rank.
ALTER TABLE roster_uploads ADD COLUMN records_ranks boolean NOT NULL DEFAULT false;

UPDATE roster_uploads r SET records_ranks = true
WHERE r.records_members
  AND EXISTS (SELECT 1 FROM roster_members m WHERE m.upload_id = r.id)
  AND NOT EXISTS (
      SELECT 1 FROM roster_members m
      WHERE m.upload_id = r.id AND (m.rank_name IS NULL OR m.rank_index IS NULL)
  );

-- Ranks by number, each with its latest name and match.
ALTER TABLE guild_ranks RENAME TO guild_ranks_by_name;

CREATE TABLE guild_ranks (
    guild text NOT NULL,
    -- The game's number for it: 0 is the guild master.
    rank smallint NOT NULL CHECK (rank BETWEEN 0 AND 9),
    -- Its name in game, from the latest upload that sent it.
    name text,
    site_rank text
        CHECK (site_rank IN ('leader', 'officer', 'raider', 'trial', 'member', 'public')),
    seen_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (guild, rank)
);

INSERT INTO guild_ranks (guild, rank, name, site_rank, seen_at)
SELECT DISTINCT ON (guild, rank_index) guild, rank_index, name, site_rank, seen_at
FROM guild_ranks_by_name
WHERE rank_index BETWEEN 0 AND 9
ORDER BY guild, rank_index, seen_at DESC;

DROP TABLE guild_ranks_by_name;

ALTER TABLE roster_members DROP COLUMN rank_name;
ALTER TABLE roster_members RENAME COLUMN rank_index TO rank;

-- A character's guild and rank number, both or neither.
UPDATE characters SET guild = NULL, guild_rank = NULL, guild_rank_index = NULL
WHERE guild IS NULL OR guild_rank_index IS NULL;

ALTER TABLE characters DROP COLUMN guild_rank;
ALTER TABLE characters RENAME COLUMN guild_rank_index TO guild_rank;
ALTER TABLE characters ADD CONSTRAINT characters_guild_rank_check
    CHECK ((guild IS NULL) = (guild_rank IS NULL));

ALTER TABLE characters DROP COLUMN in_guild;

-- No hidden characters.
DROP TRIGGER raid_attendees_character_not_hidden_trig ON raid_attendees;
DROP TRIGGER loot_character_not_hidden_trig ON loot;
DROP TRIGGER loot_priority_characters_character_not_hidden_trig ON loot_priority_characters;
DROP FUNCTION tf_character_not_hidden();

DROP TRIGGER characters_updated_at_trig ON characters;

ALTER TABLE characters
    DROP CONSTRAINT characters_hidden_from_battlenet_check,
    DROP CONSTRAINT characters_hidden_not_main_check,
    DROP COLUMN is_hidden;

CREATE TRIGGER characters_updated_at_trig
    BEFORE UPDATE ON characters
    FOR EACH ROW
    WHEN (
        (OLD.user_id, OLD.first_name, OLD.last_name, OLD.class, OLD.race, OLD.sex, OLD.ruleset,
            OLD.level, OLD.reached_60_at, OLD.is_main)
        IS DISTINCT FROM
        (NEW.user_id, NEW.first_name, NEW.last_name, NEW.class, NEW.race, NEW.sex, NEW.ruleset,
            NEW.level, NEW.reached_60_at, NEW.is_main)
    )
    EXECUTE FUNCTION tf_set_updated_at();
