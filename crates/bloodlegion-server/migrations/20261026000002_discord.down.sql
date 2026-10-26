DROP TRIGGER auth_oauth2_provider_credentials_discord_roles_trig
    ON auth_oauth2_provider_credentials;
DROP TRIGGER users_discord_roles_trig ON users;
DROP FUNCTION tf_notify_discord_roles();
DROP TABLE discord_announcements;
DROP TABLE discord_objects;
DROP TABLE discord_settings;
DROP FUNCTION tf_notify_discord_settings();
