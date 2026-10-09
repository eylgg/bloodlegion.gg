/*
# Loot priorities

The officers' plan for who gets what, as the guild kept it in a spreadsheet: for each item a
zone drops (or a kind of item, "caster trinket"), the characters in line for it, first to last.
Officers alone see and edit it.
*/

CREATE TABLE loot_priorities (
    id bigint
        PRIMARY KEY
        GENERATED ALWAYS AS IDENTITY,
    zone text
        NOT NULL
        CONSTRAINT loot_priorities_zone_check
        CHECK (raid_zone_size(zone) IS NOT NULL),
    -- The boss that drops it, when known; NULL for trash, or a kind of item from anywhere.
    boss_id bigint
        REFERENCES bosses(id) ON DELETE SET NULL,
    -- An item, or a kind of item by name ("caster trinket"): one or the other.
    item_id bigint
        REFERENCES items(id) ON DELETE CASCADE,
    label text
        CONSTRAINT loot_priorities_label_check
        CHECK (label IS NULL OR (char_length(label) > 0 AND char_length(label) <= 64)),
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    updated_at timestamp with time zone NOT NULL DEFAULT now(),

    CONSTRAINT loot_priorities_item_or_label_check CHECK ((item_id IS NULL) <> (label IS NULL)),
    CONSTRAINT loot_priorities_zone_item_id_key UNIQUE (zone, item_id)
);

CREATE INDEX loot_priorities_boss_id_idx
    ON loot_priorities (boss_id);
CREATE INDEX loot_priorities_item_id_idx
    ON loot_priorities (item_id);

CREATE TRIGGER loot_priorities_updated_at_trig
    BEFORE UPDATE ON loot_priorities
    FOR EACH ROW
    WHEN (OLD.* IS DISTINCT FROM NEW.*)
    EXECUTE FUNCTION tf_set_updated_at();

-- The characters in line for one, first (position 1) to last, each with an optional note (the
-- spec they want it for, "dm").
CREATE TABLE loot_priority_characters (
    priority_id bigint
        NOT NULL
        REFERENCES loot_priorities(id) ON DELETE CASCADE,
    character_id bigint
        NOT NULL
        REFERENCES characters(id) ON DELETE CASCADE,
    position smallint
        NOT NULL
        CONSTRAINT loot_priority_characters_position_check
        CHECK (position > 0),
    note text
        CONSTRAINT loot_priority_characters_note_check
        CHECK (note IS NULL OR (char_length(note) > 0 AND char_length(note) <= 32)),

    PRIMARY KEY (priority_id, character_id),
    CONSTRAINT loot_priority_characters_priority_id_position_key UNIQUE (priority_id, position)
);

CREATE INDEX loot_priority_characters_character_id_idx
    ON loot_priority_characters (character_id);
