DROP TABLE IF EXISTS game_items_snapshot;

ALTER TABLE game_items
    ADD COLUMN last_modified text
        CONSTRAINT game_items_last_modified_check
        CHECK (last_modified IS NULL OR char_length(last_modified) <= 64),
    ADD COLUMN listed_at timestamp with time zone,
    ADD COLUMN detailed_at timestamp with time zone;
CREATE INDEX game_items_detailed_at_idx
    ON game_items (detailed_at NULLS FIRST);
