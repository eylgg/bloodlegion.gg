-- Fails while two items share a name; rename one first.
DROP INDEX items_name_normalized_idx;
DROP INDEX items_name_normalized_unlinked_idx;
ALTER TABLE items ADD CONSTRAINT items_name_normalized_key UNIQUE (name_normalized);

DROP TABLE IF EXISTS game_item_icons;
DROP TABLE IF EXISTS game_items;
