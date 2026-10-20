/*
# The item mirror, loaded from a snapshot

The mirror no longer syncs from Battle.net itself. `cargo xtask blizzard sync` writes a snapshot
into the repository (`data/game-items`), the server embeds it, and every migrate (the server runs one on
start) loads it when it changes. So the site works offline, and every deploy has the items of its
own commit.

The columns that drove the old sync's refreshes go; the snapshot's hash takes their place.
*/

DROP INDEX game_items_detailed_at_idx;
ALTER TABLE game_items
    DROP COLUMN last_modified,
    DROP COLUMN listed_at,
    DROP COLUMN detailed_at;

-- The hash of the snapshot last loaded: one row, once a snapshot has been.
CREATE TABLE game_items_snapshot (
    id boolean
        PRIMARY KEY
        DEFAULT true
        CONSTRAINT game_items_snapshot_id_check
        CHECK (id),
    sha256 text
        NOT NULL
        CONSTRAINT game_items_snapshot_sha256_check
        CHECK (char_length(sha256) = 64),
    loaded_at timestamp with time zone NOT NULL DEFAULT now()
);
