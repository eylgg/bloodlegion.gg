UPDATE users SET guild_rank = 'member' WHERE guild_rank = 'public';

ALTER TABLE users ALTER COLUMN guild_rank SET DEFAULT 'member';
ALTER TABLE users DROP CONSTRAINT users_guild_rank_check;
ALTER TABLE users
    ADD CONSTRAINT users_guild_rank_check
    CHECK (guild_rank IN ('leader', 'officer', 'raider', 'trial', 'member', 'friend'));
