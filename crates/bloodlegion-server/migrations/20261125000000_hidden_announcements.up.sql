/*
# Hidden announcements

A superuser can hide a post from #public-announcements on the site without deleting it in
Discord. The bot's catch-ups and edits leave it hidden; superusers still see it, marked, to show
it again.
*/

ALTER TABLE discord_announcements ADD COLUMN hidden_at timestamp with time zone;
