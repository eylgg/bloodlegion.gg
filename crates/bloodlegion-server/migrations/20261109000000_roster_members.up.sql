/*
# Who each roster upload named

Every character a roster upload named (matched or added), so `characters.in_guild` is the guild as
the latest upload saw it: set for the characters it named, cleared for the rest, and put back to
the upload before when one is undone. Uploads before this recorded no members
(`records_members` false) and never decide it.
*/

CREATE TABLE roster_members (
    upload_id bigint
        NOT NULL
        REFERENCES roster_uploads(id) ON DELETE CASCADE,
    character_id bigint
        NOT NULL
        REFERENCES characters(id) ON DELETE CASCADE,
    PRIMARY KEY (upload_id, character_id)
);

CREATE INDEX roster_members_character_id_idx ON roster_members (character_id);

ALTER TABLE roster_uploads ADD COLUMN records_members boolean NOT NULL DEFAULT false;
