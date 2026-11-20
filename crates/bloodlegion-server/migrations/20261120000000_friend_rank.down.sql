UPDATE users SET guild_rank = 'member' WHERE guild_rank = 'friend';

ALTER TABLE users DROP CONSTRAINT users_guild_rank_check;
ALTER TABLE users
    ADD CONSTRAINT users_guild_rank_check
    CHECK (guild_rank IN ('leader', 'officer', 'raider', 'trial', 'member', 'public'));

UPDATE guild_ranks SET site_rank = 'member' WHERE site_rank = 'friend';

ALTER TABLE guild_ranks DROP CONSTRAINT guild_ranks_site_rank_check;
ALTER TABLE guild_ranks
    ADD CONSTRAINT guild_ranks_site_rank_check
    CHECK (site_rank IN ('leader', 'officer', 'raider', 'trial', 'member', 'public'));
