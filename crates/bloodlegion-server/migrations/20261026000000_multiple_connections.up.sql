/*
# Multiple connections

Whether a member may link more than one account at a provider: Battle.net, where many keep
characters on several, but not Discord (one person, one account). Off for every provider until
set, so a new provider takes one account per member.
*/

ALTER TABLE auth_providers
    ADD COLUMN is_multiple_connections_allowed boolean NOT NULL DEFAULT false;
