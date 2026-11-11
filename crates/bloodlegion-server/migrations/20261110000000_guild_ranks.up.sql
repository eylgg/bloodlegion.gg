/*
# The guild's name, and its ranks in game

The guild's name in game becomes a setting (it was `BL` in the code): WoW gives addons a guild's
name, not an id, so a roster upload is the guild's when it names this one. Each upload records the
guild it came from and, for each member it named, their rank in game (its name and number, 0 the
guild master). A character carries the guild and rank of the latest upload of the guild that named
it, and neither once one did not.

The ranks in game are matched to the site's in `guild_ranks`, one row per rank name an upload has
seen, so a member whose site rank differs from their characters' can be pointed out. `site_rank`
is null for a rank not to compare (an alt rank).
*/

ALTER TABLE guild_settings ADD COLUMN guild_name text NOT NULL DEFAULT 'BL'
    CHECK (length(btrim(guild_name)) BETWEEN 1 AND 24);

-- Every upload so far was BL's: the site took no other.
ALTER TABLE roster_uploads ADD COLUMN guild text NOT NULL DEFAULT 'BL';
ALTER TABLE roster_uploads ALTER COLUMN guild DROP DEFAULT;

ALTER TABLE roster_members
    ADD COLUMN rank_name text,
    ADD COLUMN rank_index smallint;

ALTER TABLE characters
    ADD COLUMN guild text,
    ADD COLUMN guild_rank text,
    ADD COLUMN guild_rank_index smallint;

UPDATE characters SET guild = 'BL' WHERE in_guild;

CREATE TABLE guild_ranks (
    guild text NOT NULL,
    name text NOT NULL,
    -- Its number in game when last seen: 0 is the guild master.
    rank_index smallint NOT NULL,
    site_rank text
        CHECK (site_rank IN ('leader', 'officer', 'raider', 'trial', 'member', 'public')),
    seen_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (guild, name)
);
