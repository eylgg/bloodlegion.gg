/*
# Connection-only providers

A provider whose accounts are only ever linked to a member's account from their profile, never a
way to sign in (Discord, beside Battle.net): it is not offered on the front page, a sign-in
through it is refused, and an account linked there does not count as a member's way in, so it
never lets them unlink their last Battle.net account. Off for every provider until set.
*/

ALTER TABLE auth_providers
    ADD COLUMN is_connection_only boolean NOT NULL DEFAULT false;
