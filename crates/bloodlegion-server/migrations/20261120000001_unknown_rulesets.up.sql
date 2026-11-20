/*
# Rulesets nobody knows

A character's ruleset is null until someone knows it: nothing takes the guild's anymore, and the
guild's ruleset setting goes, as nothing reads it. Battle.net's listing of an account's characters
has no ruleset, nor does the addon's roster, so the characters they add start without one and the
armory fills it in (right after a sign-in, or an armory fill). The armory has no character below
level 10, so theirs stays unknown until then; an officer can still pick one by hand.

The rulesets the site guessed so far go: those below level 10 (the armory can't say), and those of
the characters a Battle.net sign-in added (linked to Battle.net, never named by a roster upload).
The rest are corrected by an armory fill.
*/

ALTER TABLE characters
    ALTER COLUMN ruleset DROP NOT NULL,
    ALTER COLUMN ruleset DROP DEFAULT;

UPDATE characters c
SET ruleset = NULL
WHERE c.level < 10
   OR (c.battlenet_subject IS NOT NULL
       AND NOT EXISTS (SELECT FROM roster_members m WHERE m.character_id = c.id)
       AND NOT EXISTS (SELECT FROM roster_added a WHERE a.character_id = c.id));

ALTER TABLE guild_settings DROP COLUMN ruleset;
