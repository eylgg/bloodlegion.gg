-- Usernames may be as short as two characters (`Ey`). Same shape otherwise: a letter, then
-- letters and digits, at most 32.
ALTER TABLE users DROP CONSTRAINT users_username_check;
ALTER TABLE users ADD CONSTRAINT users_username_check
    CHECK (
        char_length(username) >= 2
        AND char_length(username) <= 32
        AND username ~ '^[A-Za-z][A-Za-z0-9]*$'
    );
