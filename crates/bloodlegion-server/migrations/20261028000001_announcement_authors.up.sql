/*
# Announcement authors

A post in #public-announcements is shown on the site only when its author's Discord account is
linked to a member's, and it is shown as that member's: the post keeps the member it was by, not a
Discord name. The posts stored so far have only a name, so they go; the bot reads the channel's
latest posts again when it next starts.
*/

DELETE FROM discord_announcements;

ALTER TABLE discord_announcements DROP COLUMN author;
ALTER TABLE discord_announcements
    ADD COLUMN user_id bigint NOT NULL
        CONSTRAINT discord_announcements_user_id_fkey
        REFERENCES users(id) ON DELETE CASCADE;

CREATE INDEX discord_announcements_user_id_idx ON discord_announcements (user_id);
