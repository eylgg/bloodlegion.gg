DELETE FROM discord_announcements;

ALTER TABLE discord_announcements DROP COLUMN user_id;
ALTER TABLE discord_announcements ADD COLUMN author text NOT NULL;
