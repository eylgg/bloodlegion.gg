-- Launch sign-ups for WoW: Forever: the characters each member plans to create when the game
-- launches (their main and their alts, with the specs they hope to play), so names are spoken for
-- and the guild can see its shape before day one. Self-reported reservations, not real
-- characters: those will come from Blizzard's API once it serves Forever, into their own table.
CREATE TABLE launch_characters (
    id bigint
        PRIMARY KEY
        GENERATED ALWAYS AS IDENTITY,
    user_id bigint
        NOT NULL
        REFERENCES users(id) ON DELETE CASCADE,
    -- As the member typed it. WoW names are 2-12 letters, accents included; the app checks the
    -- letters-only rule (a Unicode class this file cannot express collation-independently).
    name text
        NOT NULL
        CONSTRAINT launch_characters_name_check
        CHECK (char_length(name) >= 2 AND char_length(name) <= 12),
    -- Lowercase form: one character per name per member, regardless of case.
    name_normalized text
        NOT NULL
        GENERATED ALWAYS AS (lower(name)) STORED,
    -- The Forever classes. The app's catalog (launch::catalog) holds the names, colors, and specs.
    class text
        NOT NULL
        CONSTRAINT launch_characters_class_check
        CHECK (
            class IN (
                'druid', 'hunter', 'mage', 'paladin', 'priest',
                'rogue', 'shaman', 'warlock', 'warrior'
            )
        ),
    -- Spec slugs from that class's three trees; the app checks they belong to the class.
    specs text[]
        NOT NULL
        DEFAULT '{}'
        CONSTRAINT launch_characters_specs_check
        CHECK (cardinality(specs) <= 3),
    is_main boolean NOT NULL DEFAULT false,
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    updated_at timestamp with time zone NOT NULL DEFAULT now(),

    CONSTRAINT launch_characters_user_id_name_normalized_key UNIQUE (user_id, name_normalized)
);

-- At most one main per member. Partial unique indexes are non-deferrable, so moving the main
-- demotes the old one before promoting the new.
CREATE UNIQUE INDEX launch_characters_one_main_per_user_idx
    ON launch_characters (user_id)
    WHERE is_main;

-- Gated on the explicit non-generated columns (NEW.* would include the generated
-- name_normalized, which a BEFORE trigger's WHEN cannot reference).
CREATE TRIGGER launch_characters_updated_at_trig
    BEFORE UPDATE ON launch_characters
    FOR EACH ROW
    WHEN (
        (OLD.name, OLD.class, OLD.specs, OLD.is_main)
        IS DISTINCT FROM
        (NEW.name, NEW.class, NEW.specs, NEW.is_main)
    )
    EXECUTE FUNCTION tf_set_updated_at();
