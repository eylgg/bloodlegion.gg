/*
# The guild: ranks, characters, raids, and loot

Ported from the old Django site (Project Blood Legion), which tracked loot on a Classic realm, and
reshaped for WoW: Forever:

- Characters have a first and a last name. There are no realms: Forever has rulesets, and the
  guild plays on one (PvP), so a character is its name, unique across the site.
- Raid zones are the app's catalog (raids::catalog), checked here like launch_characters' classes;
  raid_zone_size() mirrors the sizes so the database can cap a raid's attendance.
- The old site's lockouts (a zone plus a weekly reset number) are gone: a raid is one scheduled
  night in a zone, with the characters who came and the loot they won.
*/

/*
## Ranks

The old site's ranks, in order. Leaders and officers run raids and record loot (as does any
superuser); raiders and trials make up the raiding roster.
*/

ALTER TABLE users
    ADD COLUMN guild_rank text
        NOT NULL
        DEFAULT 'member'
        CONSTRAINT users_guild_rank_check
        CHECK (
            guild_rank IN (
                'leader', 'officer', 'raider', 'trial', 'member', 'friend', 'retired'
            )
        );

DROP TRIGGER users_updated_at_trig ON users;
CREATE TRIGGER users_updated_at_trig
    BEFORE UPDATE ON users
    FOR EACH ROW
    WHEN (
        (OLD.username, OLD.is_superuser, OLD.first_name, OLD.last_name, OLD.disabled_at,
         OLD.guild_rank)
        IS DISTINCT FROM
        (NEW.username, NEW.is_superuser, NEW.first_name, NEW.last_name, NEW.disabled_at,
         NEW.guild_rank)
    )
    EXECUTE FUNCTION tf_set_updated_at();

/*
## Characters
*/

CREATE TABLE characters (
    id bigint
        PRIMARY KEY
        GENERATED ALWAYS AS IDENTITY,
    -- The member who plays it. NULL for a character no account claims (a pug who won loot), which
    -- an officer recorded.
    user_id bigint
        REFERENCES users(id) ON DELETE CASCADE,
    -- Letters only, as the app checks (characters::name); here, the bounds and no spaces or digits.
    first_name text
        NOT NULL
        CONSTRAINT characters_first_name_check
        CHECK (
            char_length(first_name) >= 2
            AND char_length(first_name) <= 24
            AND first_name !~ '[[:space:]0-9]'
        ),
    last_name text
        NOT NULL
        CONSTRAINT characters_last_name_check
        CHECK (
            char_length(last_name) >= 2
            AND char_length(last_name) <= 24
            AND last_name !~ '[[:space:]0-9]'
        ),
    -- Lowercase full name; carries uniqueness the way users.username_normalized does. STORED, so
    -- it can be indexed (PG18 defaults generated columns to VIRTUAL).
    name_normalized text
        NOT NULL
        GENERATED ALWAYS AS (lower(first_name || ' ' || last_name)) STORED,
    class text
        NOT NULL
        CONSTRAINT characters_class_check
        CHECK (
            class IN (
                'druid', 'hunter', 'mage', 'paladin', 'priest',
                'rogue', 'shaman', 'warlock', 'warrior'
            )
        ),
    is_main boolean NOT NULL DEFAULT false,
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    updated_at timestamp with time zone NOT NULL DEFAULT now(),

    CONSTRAINT characters_name_normalized_key UNIQUE (name_normalized),
    -- An unclaimed character is nobody's main.
    CONSTRAINT characters_main_owned_check CHECK (NOT is_main OR user_id IS NOT NULL)
);

CREATE INDEX characters_user_id_idx
    ON characters (user_id);

-- At most one main per member. Partial unique indexes are non-deferrable, so moving the main
-- demotes the old one before promoting the new.
CREATE UNIQUE INDEX characters_one_main_per_user_idx
    ON characters (user_id)
    WHERE is_main;

CREATE TRIGGER characters_updated_at_trig
    BEFORE UPDATE ON characters
    FOR EACH ROW
    WHEN (
        (OLD.user_id, OLD.first_name, OLD.last_name, OLD.class, OLD.is_main)
        IS DISTINCT FROM
        (NEW.user_id, NEW.first_name, NEW.last_name, NEW.class, NEW.is_main)
    )
    EXECUTE FUNCTION tf_set_updated_at();

-- A character's notes (the old site's "notes": what they are saving for, their availability),
-- written by its owner and read by officers.
CREATE TABLE character_notes (
    character_id bigint
        PRIMARY KEY
        REFERENCES characters(id) ON DELETE CASCADE,
    body text
        NOT NULL
        CONSTRAINT character_notes_body_check
        CHECK (char_length(body) > 0 AND char_length(body) <= 10000),
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    updated_at timestamp with time zone NOT NULL DEFAULT now()
);

CREATE TRIGGER character_notes_updated_at_trig
    BEFORE UPDATE ON character_notes
    FOR EACH ROW
    WHEN (OLD.* IS DISTINCT FROM NEW.*)
    EXECUTE FUNCTION tf_set_updated_at();

/*
## Zones, bosses, and items
*/

-- The raid size of each zone in the app's catalog (raids::catalog), NULL for anything else. A test
-- checks the two agree.
CREATE FUNCTION raid_zone_size(zone text)
    RETURNS integer
    LANGUAGE sql
    IMMUTABLE
AS $$
    SELECT CASE zone
        WHEN 'barrow-deeps' THEN 10
        WHEN 'hyjal-summit' THEN 20
        WHEN 'onyxias-lair' THEN 40
    END;
$$;

-- A zone's bosses, entered by officers as the guild meets them: Forever's raids are new, and there
-- is no database of their encounters to load. Listed in the order they were added.
CREATE TABLE bosses (
    id bigint
        PRIMARY KEY
        GENERATED ALWAYS AS IDENTITY,
    zone text
        NOT NULL
        CONSTRAINT bosses_zone_check
        CHECK (raid_zone_size(zone) IS NOT NULL),
    name text
        NOT NULL
        CONSTRAINT bosses_name_check
        CHECK (char_length(name) > 0 AND char_length(name) <= 64),
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    updated_at timestamp with time zone NOT NULL DEFAULT now(),

    CONSTRAINT bosses_zone_name_key UNIQUE (zone, name)
);

CREATE TRIGGER bosses_updated_at_trig
    BEFORE UPDATE ON bosses
    FOR EACH ROW
    WHEN (OLD.* IS DISTINCT FROM NEW.*)
    EXECUTE FUNCTION tf_set_updated_at();

INSERT INTO bosses (zone, name) VALUES ('onyxias-lair', 'Onyxia');

-- Items, added as they drop. game_item_id is the game's own item id, for tooltips and links, once
-- officers know it.
CREATE TABLE items (
    id bigint
        PRIMARY KEY
        GENERATED ALWAYS AS IDENTITY,
    name text
        NOT NULL
        CONSTRAINT items_name_check
        CHECK (char_length(name) > 0 AND char_length(name) <= 128),
    name_normalized text
        NOT NULL
        GENERATED ALWAYS AS (lower(name)) STORED,
    quality text
        NOT NULL
        CONSTRAINT items_quality_check
        CHECK (quality IN ('poor', 'common', 'uncommon', 'rare', 'epic', 'legendary')),
    game_item_id integer
        CONSTRAINT items_game_item_id_check
        CHECK (game_item_id IS NULL OR game_item_id > 0),
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    updated_at timestamp with time zone NOT NULL DEFAULT now(),

    CONSTRAINT items_name_normalized_key UNIQUE (name_normalized),
    CONSTRAINT items_game_item_id_key UNIQUE (game_item_id)
);

CREATE TRIGGER items_updated_at_trig
    BEFORE UPDATE ON items
    FOR EACH ROW
    WHEN (
        (OLD.name, OLD.quality, OLD.game_item_id)
        IS DISTINCT FROM
        (NEW.name, NEW.quality, NEW.game_item_id)
    )
    EXECUTE FUNCTION tf_set_updated_at();

/*
## Raids
*/

CREATE TABLE raids (
    id bigint
        PRIMARY KEY
        GENERATED ALWAYS AS IDENTITY,
    zone text
        NOT NULL
        CONSTRAINT raids_zone_check
        CHECK (raid_zone_size(zone) IS NOT NULL),
    -- Optional, to tell apart two groups in one zone on one night ("Group 2").
    title text
        CONSTRAINT raids_title_check
        CHECK (title IS NULL OR (char_length(title) > 0 AND char_length(title) <= 64)),
    starts_at timestamp with time zone NOT NULL,
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    updated_at timestamp with time zone NOT NULL DEFAULT now()
);

CREATE INDEX raids_starts_at_idx
    ON raids (starts_at);

CREATE TRIGGER raids_updated_at_trig
    BEFORE UPDATE ON raids
    FOR EACH ROW
    WHEN (OLD.* IS DISTINCT FROM NEW.*)
    EXECUTE FUNCTION tf_set_updated_at();

CREATE TABLE raid_attendees (
    raid_id bigint
        NOT NULL
        REFERENCES raids(id) ON DELETE CASCADE,
    character_id bigint
        NOT NULL
        REFERENCES characters(id) ON DELETE CASCADE,
    created_at timestamp with time zone NOT NULL DEFAULT now(),

    PRIMARY KEY (raid_id, character_id)
);

CREATE INDEX raid_attendees_character_id_idx
    ON raid_attendees (character_id);

-- A raid holds at most its zone's size. Locks the raid row, so two officers adding the last seat
-- at once serialize and the second is refused.
CREATE FUNCTION tf_raid_attendees_within_size()
    RETURNS trigger
    LANGUAGE plpgsql
AS $$
DECLARE
    size integer;
BEGIN
    SELECT raid_zone_size(zone) INTO size FROM raids WHERE id = NEW.raid_id FOR UPDATE;
    IF (SELECT count(*) FROM raid_attendees WHERE raid_id = NEW.raid_id) >= size THEN
        RAISE EXCEPTION 'raid % is full (% characters)', NEW.raid_id, size
            USING ERRCODE = 'check_violation',
                  CONSTRAINT = 'raid_attendees_within_size_check';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER raid_attendees_within_size_trig
    BEFORE INSERT ON raid_attendees
    FOR EACH ROW
    EXECUTE FUNCTION tf_raid_attendees_within_size();

-- Moving a raid to a smaller zone must still fit everyone who came.
CREATE FUNCTION tf_raids_within_size()
    RETURNS trigger
    LANGUAGE plpgsql
AS $$
BEGIN
    IF (SELECT count(*) FROM raid_attendees WHERE raid_id = NEW.id) > raid_zone_size(NEW.zone) THEN
        RAISE EXCEPTION 'raid % has more characters than % holds', NEW.id, NEW.zone
            USING ERRCODE = 'check_violation',
                  CONSTRAINT = 'raids_within_size_check';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER raids_within_size_trig
    BEFORE UPDATE OF zone ON raids
    FOR EACH ROW
    WHEN (OLD.zone IS DISTINCT FROM NEW.zone)
    EXECUTE FUNCTION tf_raids_within_size();

/*
## Loot
*/

CREATE TABLE loot (
    id bigint
        PRIMARY KEY
        GENERATED ALWAYS AS IDENTITY,
    raid_id bigint
        NOT NULL
        REFERENCES raids(id) ON DELETE CASCADE,
    -- NULL for a trash drop.
    boss_id bigint
        REFERENCES bosses(id) ON DELETE CASCADE,
    item_id bigint
        NOT NULL
        REFERENCES items(id) ON DELETE CASCADE,
    -- NULL when nobody took it (disenchanted, or left in the guild bank).
    character_id bigint
        REFERENCES characters(id) ON DELETE CASCADE,
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    updated_at timestamp with time zone NOT NULL DEFAULT now()
);

CREATE INDEX loot_raid_id_idx
    ON loot (raid_id);
CREATE INDEX loot_boss_id_idx
    ON loot (boss_id);
CREATE INDEX loot_item_id_idx
    ON loot (item_id);
CREATE INDEX loot_character_id_idx
    ON loot (character_id);

CREATE TRIGGER loot_updated_at_trig
    BEFORE UPDATE ON loot
    FOR EACH ROW
    WHEN (OLD.* IS DISTINCT FROM NEW.*)
    EXECUTE FUNCTION tf_set_updated_at();

-- A boss drop comes from a boss of the raid's own zone.
CREATE FUNCTION tf_loot_boss_zone_consistent()
    RETURNS trigger
    LANGUAGE plpgsql
AS $$
BEGIN
    IF NEW.boss_id IS NOT NULL
        AND (SELECT zone FROM bosses WHERE id = NEW.boss_id)
            IS DISTINCT FROM (SELECT zone FROM raids WHERE id = NEW.raid_id)
    THEN
        RAISE EXCEPTION 'boss % is not in the zone of raid %', NEW.boss_id, NEW.raid_id
            USING ERRCODE = 'check_violation',
                  CONSTRAINT = 'loot_boss_zone_consistent_check';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER loot_boss_zone_consistent_trig
    BEFORE INSERT OR UPDATE OF boss_id, raid_id ON loot
    FOR EACH ROW
    EXECUTE FUNCTION tf_loot_boss_zone_consistent();

-- A raid that already has boss loot keeps its zone (else its drops would name another zone's
-- bosses).
CREATE FUNCTION tf_raids_zone_loot_consistent()
    RETURNS trigger
    LANGUAGE plpgsql
AS $$
BEGIN
    IF EXISTS (SELECT 1 FROM loot WHERE raid_id = NEW.id AND boss_id IS NOT NULL) THEN
        RAISE EXCEPTION 'raid % has boss loot; its zone cannot change', NEW.id
            USING ERRCODE = 'check_violation',
                  CONSTRAINT = 'raids_zone_loot_consistent_check';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER raids_zone_loot_consistent_trig
    BEFORE UPDATE OF zone ON raids
    FOR EACH ROW
    WHEN (OLD.zone IS DISTINCT FROM NEW.zone)
    EXECUTE FUNCTION tf_raids_zone_loot_consistent();

/*
## Questions

The old site's yes/no questions: officers ask the guild ("Can you make Thursdays?"), each member
answers once and may change their mind.
*/

CREATE TABLE questions (
    id bigint
        PRIMARY KEY
        GENERATED ALWAYS AS IDENTITY,
    title text
        NOT NULL
        CONSTRAINT questions_title_check
        CHECK (char_length(title) > 0 AND char_length(title) <= 120),
    body text
        NOT NULL
        DEFAULT ''
        CONSTRAINT questions_body_check
        CHECK (char_length(body) <= 4000),
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    updated_at timestamp with time zone NOT NULL DEFAULT now()
);

CREATE TRIGGER questions_updated_at_trig
    BEFORE UPDATE ON questions
    FOR EACH ROW
    WHEN (OLD.* IS DISTINCT FROM NEW.*)
    EXECUTE FUNCTION tf_set_updated_at();

CREATE TABLE question_answers (
    question_id bigint
        NOT NULL
        REFERENCES questions(id) ON DELETE CASCADE,
    user_id bigint
        NOT NULL
        REFERENCES users(id) ON DELETE CASCADE,
    choice boolean NOT NULL,
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    updated_at timestamp with time zone NOT NULL DEFAULT now(),

    PRIMARY KEY (question_id, user_id)
);

CREATE TRIGGER question_answers_updated_at_trig
    BEFORE UPDATE ON question_answers
    FOR EACH ROW
    WHEN (OLD.* IS DISTINCT FROM NEW.*)
    EXECUTE FUNCTION tf_set_updated_at();
