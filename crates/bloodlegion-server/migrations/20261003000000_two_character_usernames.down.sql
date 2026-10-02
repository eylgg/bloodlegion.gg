-- Fails if a two-character username exists; rename it first.
ALTER TABLE users DROP CONSTRAINT users_username_check;
ALTER TABLE users ADD CONSTRAINT users_username_check
    CHECK (
        char_length(username) >= 3
        AND char_length(username) <= 32
        AND username ~ '^[A-Za-z][A-Za-z0-9]*$'
    );
