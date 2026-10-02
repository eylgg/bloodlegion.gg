DROP TRIGGER raids_updated_at_trig ON raids;
DROP INDEX raids_starts_at_idx;

-- Back to plain instants: materialize the generated column under a new name, then swap.
ALTER TABLE raids ADD COLUMN starts_at_instant timestamp with time zone;
UPDATE raids SET starts_at_instant = starts_at;
ALTER TABLE raids DROP COLUMN starts_at;
ALTER TABLE raids RENAME COLUMN starts_at_instant TO starts_at;
ALTER TABLE raids ALTER COLUMN starts_at SET NOT NULL;
ALTER TABLE raids DROP COLUMN starts_local, DROP COLUMN time_zone;

CREATE INDEX raids_starts_at_idx
    ON raids (starts_at);

CREATE TRIGGER raids_updated_at_trig
    BEFORE UPDATE ON raids
    FOR EACH ROW
    WHEN (OLD.* IS DISTINCT FROM NEW.*)
    EXECUTE FUNCTION tf_set_updated_at();

DROP TABLE IF EXISTS guild_settings;
