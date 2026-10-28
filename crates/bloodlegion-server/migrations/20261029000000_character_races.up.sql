/*
# Character races

Each character's race: Classic's eight and WoW: Forever's Skyborne, one for each faction. NULL
until someone records it (characters made before races were). The app checks it against its
catalog (`launch::catalog::RACES`) too; the list here mirrors it, like the classes.
*/

ALTER TABLE characters
    ADD COLUMN race text
        CONSTRAINT characters_race_check
        CHECK (race IS NULL OR race IN (
            'human', 'dwarf', 'night-elf', 'gnome', 'high-order-skyborne',
            'orc', 'undead', 'tauren', 'troll', 'windshaper-skyborne'
        ));

DROP TRIGGER characters_updated_at_trig ON characters;
CREATE TRIGGER characters_updated_at_trig
    BEFORE UPDATE ON characters
    FOR EACH ROW
    WHEN (
        (OLD.user_id, OLD.first_name, OLD.last_name, OLD.class, OLD.race, OLD.is_main)
        IS DISTINCT FROM
        (NEW.user_id, NEW.first_name, NEW.last_name, NEW.class, NEW.race, NEW.is_main)
    )
    EXECUTE FUNCTION tf_set_updated_at();
