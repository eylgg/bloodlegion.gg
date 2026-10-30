/*
# A public rank, and no friend rank

Anyone can sign in, so a new account is public until an officer gives it a rank: the lowest rank,
like the Discord server's Public category, which everyone sees. The Discord bot keeps no role for
it. The guild drops the friend rank: anyone who had it is a member.

Accounts that are members already stay members.
*/

UPDATE users SET guild_rank = 'member' WHERE guild_rank = 'friend';

ALTER TABLE users DROP CONSTRAINT users_guild_rank_check;
ALTER TABLE users
    ADD CONSTRAINT users_guild_rank_check
    CHECK (guild_rank IN ('leader', 'officer', 'raider', 'trial', 'member', 'public'));
ALTER TABLE users ALTER COLUMN guild_rank SET DEFAULT 'public';
