/*
# Characters in the guild

Whether the guild's roster (a roster upload from the addon) has named the character: one nobody
plays on the site yet is still a guild member's, so it shows on the roster and races to 60 like a
member's. A character an officer recorded for no one (a pug who won loot) is not. The characters
uploads already matched or added are.
*/

ALTER TABLE characters ADD COLUMN in_guild boolean NOT NULL DEFAULT false;

UPDATE characters SET in_guild = true
WHERE id IN (SELECT character_id FROM roster_added)
   OR id IN (SELECT character_id FROM roster_changes);
