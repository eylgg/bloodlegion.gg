/*
# Roster uploads

Each roster the Blood Legion client sends: who sent it and when, how many members it named and
matched, and the members it could not match. And each character it changed, with its level and
time at 60 before and after, so a superuser can undo an upload with bad data: undoing puts back
the old values of the characters still as that upload left them.
*/

CREATE TABLE roster_uploads (
    id bigint
        PRIMARY KEY
        GENERATED ALWAYS AS IDENTITY,
    -- NULL once the account is gone.
    user_id bigint
        REFERENCES users(id) ON DELETE SET NULL,
    created_at timestamptz
        NOT NULL
        DEFAULT now(),
    members integer
        NOT NULL,
    matched integer
        NOT NULL,
    -- [{name, level, class}], as the upload's answer gave them.
    unmatched jsonb
        NOT NULL
        DEFAULT '[]',
    undone_at timestamptz,
    undone_by bigint
        REFERENCES users(id) ON DELETE SET NULL
);

CREATE INDEX roster_uploads_user_id_idx ON roster_uploads (user_id);
CREATE INDEX roster_uploads_undone_by_idx ON roster_uploads (undone_by);

CREATE TABLE roster_changes (
    upload_id bigint
        NOT NULL
        REFERENCES roster_uploads(id) ON DELETE CASCADE,
    character_id bigint
        NOT NULL
        REFERENCES characters(id) ON DELETE CASCADE,
    old_level smallint
        NOT NULL,
    old_reached_60_at timestamptz,
    new_level smallint
        NOT NULL,
    new_reached_60_at timestamptz,
    PRIMARY KEY (upload_id, character_id)
);

CREATE INDEX roster_changes_character_id_idx ON roster_changes (character_id);
