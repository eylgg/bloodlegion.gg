/*
# A character's sex is optional

Like its race, a character's sex is NULL until someone records it: a roster upload adds characters
from the guild list, which says neither. The characters already here keep theirs.
*/

ALTER TABLE characters
    ALTER COLUMN sex DROP NOT NULL,
    ALTER COLUMN sex DROP DEFAULT;
