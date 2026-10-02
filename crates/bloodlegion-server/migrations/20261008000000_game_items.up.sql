/*
# The game's item database, mirrored

A local copy of the items Blizzard's Game Data API serves (game_items::sync keeps it current), so
the site shows items, icons, and tooltips from its own database rather than a third party's. Until
the API serves WoW: Forever it mirrors Classic Era, Forever's nearest relative.

The guild's own items (`items`, what was won) link to it through `game_item_id`; an item the mirror
does not know (a Forever newcomer) still works by name alone.
*/

CREATE TABLE game_items (
    -- The game's own item id.
    id integer
        PRIMARY KEY
        CONSTRAINT game_items_id_check
        CHECK (id > 0),
    name text
        NOT NULL
        CONSTRAINT game_items_name_check
        CHECK (char_length(name) > 0 AND char_length(name) <= 128),
    name_normalized text
        NOT NULL
        GENERATED ALWAYS AS (lower(name)) STORED,
    -- The API's quality, lowercased: poor through legendary (and the later game's artifact and
    -- heirloom, which Classic has none of).
    quality text
        NOT NULL
        CONSTRAINT game_items_quality_check
        CHECK (char_length(quality) > 0 AND char_length(quality) <= 32),
    item_level integer NOT NULL,
    required_level integer NOT NULL,
    -- The slot, as the API types it, lowercased (`feet`, `non_equip`), and as it names it (`Feet`).
    inventory_type text
        NOT NULL
        CONSTRAINT game_items_inventory_type_check
        CHECK (char_length(inventory_type) > 0 AND char_length(inventory_type) <= 32),
    slot text
        NOT NULL
        CONSTRAINT game_items_slot_check
        CHECK (char_length(slot) <= 64),
    item_class text
        NOT NULL
        CONSTRAINT game_items_item_class_check
        CHECK (char_length(item_class) <= 64),
    item_subclass text
        NOT NULL
        CONSTRAINT game_items_item_subclass_check
        CHECK (char_length(item_subclass) <= 64),
    is_equippable boolean NOT NULL,
    -- The icon's file name (`inv_boots_07`); its image is in game_item_icons once downloaded.
    icon text
        CONSTRAINT game_items_icon_check
        CHECK (icon IS NULL OR (char_length(icon) > 0 AND char_length(icon) <= 128)),
    -- The API's `preview_item`: everything a tooltip shows, with the game's own display strings.
    -- NULL until the item's own page has been fetched (the search listing lacks it).
    preview jsonb
        CONSTRAINT game_items_preview_check
        CHECK (preview IS NULL OR preview IS JSON OBJECT),
    -- The item page's Last-Modified, sent back as If-Modified-Since so an unchanged item costs a
    -- 304 on refresh.
    last_modified text
        CONSTRAINT game_items_last_modified_check
        CHECK (last_modified IS NULL OR char_length(last_modified) <= 64),
    -- When the search listing last included it, and when its own page was last fetched.
    listed_at timestamp with time zone,
    detailed_at timestamp with time zone,
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    updated_at timestamp with time zone NOT NULL DEFAULT now()
);

-- Prefix and substring lookups for the item picker.
CREATE INDEX game_items_name_normalized_idx
    ON game_items (name_normalized text_pattern_ops);

-- The sync's queue: items whose page is missing or stale.
CREATE INDEX game_items_detailed_at_idx
    ON game_items (detailed_at NULLS FIRST);

CREATE TRIGGER game_items_updated_at_trig
    BEFORE UPDATE ON game_items
    FOR EACH ROW
    WHEN (
        (OLD.name, OLD.quality, OLD.item_level, OLD.required_level, OLD.inventory_type, OLD.slot,
         OLD.item_class, OLD.item_subclass, OLD.is_equippable, OLD.icon, OLD.preview)
        IS DISTINCT FROM
        (NEW.name, NEW.quality, NEW.item_level, NEW.required_level, NEW.inventory_type, NEW.slot,
         NEW.item_class, NEW.item_subclass, NEW.is_equippable, NEW.icon, NEW.preview)
    )
    EXECUTE FUNCTION tf_set_updated_at();

-- Icon images, by file name: many items share one. Downloaded once from Blizzard's render
-- service and served from here.
CREATE TABLE game_item_icons (
    name text
        PRIMARY KEY
        CONSTRAINT game_item_icons_name_check
        CHECK (char_length(name) > 0 AND char_length(name) <= 128),
    content_type text
        NOT NULL
        CONSTRAINT game_item_icons_content_type_check
        CHECK (content_type IN ('image/jpeg', 'image/png')),
    image bytea
        NOT NULL
        CONSTRAINT game_item_icons_image_check
        CHECK (octet_length(image) > 0 AND octet_length(image) <= 262144),
    created_at timestamp with time zone NOT NULL DEFAULT now()
);

-- The game reuses names (the two Bindings of the Windseeker), so a name is unique only among the
-- items no game item backs; a linked item is told apart by its id.
ALTER TABLE items DROP CONSTRAINT items_name_normalized_key;
CREATE UNIQUE INDEX items_name_normalized_unlinked_idx
    ON items (name_normalized)
    WHERE game_item_id IS NULL;
CREATE INDEX items_name_normalized_idx
    ON items (name_normalized);
