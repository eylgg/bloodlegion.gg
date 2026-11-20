/*
# The friend rank again

Friends of the guild have a rank of their own once more, between the members and the public: they
see the guild's pages, as everyone but the public does. The Discord bot keeps a role for them, and a
Friends category open to everyone but the public; the Members category stays closed to them.

Nobody's rank changes: an officer makes someone a friend.
*/

ALTER TABLE users DROP CONSTRAINT users_guild_rank_check;
ALTER TABLE users
    ADD CONSTRAINT users_guild_rank_check
    CHECK (guild_rank IN ('leader', 'officer', 'raider', 'trial', 'member', 'friend', 'public'));

-- A rank in game can stand for it too. The check was made unnamed, so it is found, not named.
DO $$
DECLARE
    found text;
BEGIN
    FOR found IN
        SELECT conname FROM pg_constraint
        WHERE conrelid = 'guild_ranks'::regclass
          AND contype = 'c'
          AND pg_get_constraintdef(oid) LIKE '%site_rank%'
    LOOP
        EXECUTE format('ALTER TABLE guild_ranks DROP CONSTRAINT %I', found);
    END LOOP;
END
$$;
ALTER TABLE guild_ranks
    ADD CONSTRAINT guild_ranks_site_rank_check
    CHECK (site_rank IN ('leader', 'officer', 'raider', 'trial', 'member', 'friend', 'public'));
