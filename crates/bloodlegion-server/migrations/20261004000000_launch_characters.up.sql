-- Launch sign-ups for WoW: Forever: the classes each member plans to play when the game launches
-- (their main and their alts, with the specs they hope to play), so the guild can see its shape
-- before day one. Unnamed on purpose: Forever characters have a first and a last name, and few
-- members will have picked theirs yet; the member's username stands in. Self-reported plans, not
-- real characters: those will come from Blizzard's API once it serves Forever, into their own
-- table.
CREATE TABLE launch_characters (
    id bigint
        PRIMARY KEY
        GENERATED ALWAYS AS IDENTITY,
    user_id bigint
        NOT NULL
        REFERENCES users(id) ON DELETE CASCADE,
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

    -- One sign-up per class per member: without names, a class is what tells them apart.
    CONSTRAINT launch_characters_user_id_class_key UNIQUE (user_id, class)
);

-- At most one main per member. Partial unique indexes are non-deferrable, so moving the main
-- demotes the old one before promoting the new.
CREATE UNIQUE INDEX launch_characters_one_main_per_user_idx
    ON launch_characters (user_id)
    WHERE is_main;

CREATE TRIGGER launch_characters_updated_at_trig
    BEFORE UPDATE ON launch_characters
    FOR EACH ROW
    WHEN (
        (OLD.class, OLD.specs, OLD.is_main)
        IS DISTINCT FROM
        (NEW.class, NEW.specs, NEW.is_main)
    )
    EXECUTE FUNCTION tf_set_updated_at();
