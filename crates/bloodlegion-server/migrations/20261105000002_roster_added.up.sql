/*
# Characters roster uploads add

A roster upload adds the members no character on the site matches: a new character nobody plays
yet, or a hidden one from a Battle.net listing brought out. Each is recorded with its upload, so
undoing the upload removes the ones it made (still nobody's, and never in a raid or loot).
*/

CREATE TABLE roster_added (
    upload_id bigint
        NOT NULL
        REFERENCES roster_uploads(id) ON DELETE CASCADE,
    character_id bigint
        NOT NULL
        REFERENCES characters(id) ON DELETE CASCADE,
    -- A hidden character brought out, rather than one made; undoing keeps it.
    revealed boolean NOT NULL,
    PRIMARY KEY (upload_id, character_id)
);

CREATE INDEX roster_added_character_id_idx ON roster_added (character_id);
