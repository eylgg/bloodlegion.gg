DROP TABLE guild_ranks;

ALTER TABLE characters
    DROP COLUMN guild,
    DROP COLUMN guild_rank,
    DROP COLUMN guild_rank_index;

ALTER TABLE roster_members
    DROP COLUMN rank_name,
    DROP COLUMN rank_index;

ALTER TABLE roster_uploads DROP COLUMN guild;

ALTER TABLE guild_settings DROP COLUMN guild_name;
