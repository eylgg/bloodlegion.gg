/*
# No retired rank, and no Discord role sync

The guild drops the retired rank: anyone who had it is a member again.

The Discord bot no longer hands out roles (a superuser's role check lists who lacks theirs), so
nothing listens for ranks or links changing any more.
*/

UPDATE users SET guild_rank = 'member' WHERE guild_rank = 'retired';

ALTER TABLE users DROP CONSTRAINT users_guild_rank_check;
ALTER TABLE users
    ADD CONSTRAINT users_guild_rank_check
    CHECK (guild_rank IN ('leader', 'officer', 'raider', 'trial', 'member', 'friend'));

DROP TRIGGER auth_oauth2_provider_credentials_discord_roles_trig
    ON auth_oauth2_provider_credentials;
DROP TRIGGER users_discord_roles_trig ON users;
DROP FUNCTION tf_notify_discord_roles();
