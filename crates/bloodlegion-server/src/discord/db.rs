use sqlx::{PgConnection, PgPool};
use time::OffsetDateTime;

use super::DISCORD_PROVIDER_SLUG;
use crate::crypto::Ciphertext;
use crate::guild::Rank;
use crate::users::UserId;

/// How the bot reaches the server, as stored: either may be unset.
pub struct Settings {
    pub guild_id: Option<u64>,
    pub bot_token: Option<Ciphertext>,
}

pub async fn settings(pool: &PgPool) -> sqlx::Result<Settings> {
    // A named row: `query!`'s own derives `Debug`, which a ciphertext does not have.
    struct Row {
        guild_id: Option<i64>,
        bot_token: Option<Ciphertext>,
    }
    let row = sqlx::query_as!(
        Row,
        r#"
        SELECT guild_id, bot_token_ciphertext AS "bot_token: Ciphertext"
        FROM discord_settings
        "#
    )
    .fetch_one(pool)
    .await?;
    Ok(Settings {
        guild_id: row.guild_id.map(|id| id as u64),
        bot_token: row.bot_token,
    })
}

pub async fn set_guild_id(pool: &PgPool, guild_id: Option<u64>) -> sqlx::Result<()> {
    sqlx::query!(
        "UPDATE discord_settings SET guild_id = $1",
        guild_id.map(|id| id as i64),
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn set_bot_token(pool: &PgPool, bot_token: Option<&Ciphertext>) -> sqlx::Result<()> {
    sqlx::query!(
        "UPDATE discord_settings SET bot_token_ciphertext = $1",
        bot_token.map(AsRef::<str>::as_ref),
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// What the bot last said about itself.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Status {
    /// The bot's name in Discord, as it signed in.
    pub bot_user: Option<String>,
    /// When it started; `None` while it is not running.
    #[serde(with = "time::serde::iso8601::option")]
    pub running_since: Option<OffsetDateTime>,
    /// When the instance running it last wrote (every few seconds while it runs).
    #[serde(with = "time::serde::iso8601::option")]
    pub seen_at: Option<OffsetDateTime>,
    /// When it last checked its roles and categories.
    #[serde(with = "time::serde::iso8601::option")]
    pub checked_at: Option<OffsetDateTime>,
    pub last_error: Option<String>,
    #[serde(with = "time::serde::iso8601::option")]
    pub last_error_at: Option<OffsetDateTime>,
}

pub async fn status(pool: &PgPool) -> sqlx::Result<Status> {
    sqlx::query_as!(
        Status,
        "SELECT bot_user, running_since, seen_at, checked_at, last_error, last_error_at
         FROM discord_status"
    )
    .fetch_one(pool)
    .await
}

/// The bot started, signed in as `bot_user`, and set up its roles and categories.
pub async fn status_started(pool: &PgPool, bot_user: &str) -> sqlx::Result<()> {
    sqlx::query!(
        "UPDATE discord_status
         SET bot_user = $1, running_since = now(), seen_at = now(), checked_at = now()",
        bot_user,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// The bot checked its roles and categories again.
pub async fn status_checked(pool: &PgPool) -> sqlx::Result<()> {
    sqlx::query!("UPDATE discord_status SET checked_at = now()")
        .execute(pool)
        .await?;
    Ok(())
}

/// The running bot is still there. On the lock's own connection, so it also proves the lock is.
pub async fn status_seen(conn: &mut PgConnection) -> sqlx::Result<()> {
    sqlx::query!("UPDATE discord_status SET seen_at = now()")
        .execute(conn)
        .await?;
    Ok(())
}

/// The bot stopped, with the error that stopped it if one did.
pub async fn status_stopped(pool: &PgPool, error: Option<&str>) -> sqlx::Result<()> {
    sqlx::query!(
        "UPDATE discord_status SET
             running_since = NULL,
             last_error = COALESCE($1, last_error),
             last_error_at = CASE WHEN $1::text IS NULL THEN last_error_at ELSE now() END",
        error,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// The Discord id of what the bot made under `key`, or `None` before it has.
pub async fn object_id(pool: &PgPool, key: &str) -> sqlx::Result<Option<u64>> {
    let id = sqlx::query_scalar!("SELECT discord_id FROM discord_objects WHERE key = $1", key)
        .fetch_optional(pool)
        .await?;
    Ok(id.map(|id| id as u64))
}

pub async fn set_object_id(pool: &PgPool, key: &str, id: u64) -> sqlx::Result<()> {
    sqlx::query!(
        "INSERT INTO discord_objects (key, discord_id) VALUES ($1, $2)
         ON CONFLICT (key) DO UPDATE SET discord_id = EXCLUDED.discord_id",
        key,
        id as i64,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// A member's linked Discord account: its Discord user id (the credential's subject), and the
/// member's username and rank.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct LinkedAccount {
    pub subject: String,
    pub username: String,
    pub rank: Rank,
}

/// Every linked Discord account of a member whose account is not disabled.
pub async fn linked_accounts(pool: &PgPool) -> sqlx::Result<Vec<LinkedAccount>> {
    sqlx::query_as!(
        LinkedAccount,
        r#"
        SELECT c.subject, u.username, u.guild_rank AS "rank: Rank"
        FROM auth_oauth2_provider_credentials c
        JOIN auth_providers p ON p.id = c.provider_id
        JOIN users u ON u.id = c.user_id
        WHERE p.slug = $1 AND c.disconnected_at IS NULL AND u.disabled_at IS NULL
        ORDER BY u.username
        "#,
        DISCORD_PROVIDER_SLUG,
    )
    .fetch_all(pool)
    .await
}

/// The member whose linked Discord account `discord_user` is, if their account is not disabled.
pub async fn linked_member(pool: &PgPool, discord_user: u64) -> sqlx::Result<Option<UserId>> {
    sqlx::query_scalar!(
        r#"
        SELECT u.id AS "id: UserId"
        FROM auth_oauth2_provider_credentials c
        JOIN auth_providers p ON p.id = c.provider_id
        JOIN users u ON u.id = c.user_id
        WHERE p.slug = $1
          AND c.subject = $2
          AND c.disconnected_at IS NULL
          AND u.disabled_at IS NULL
        "#,
        DISCORD_PROVIDER_SLUG,
        discord_user.to_string(),
    )
    .fetch_optional(pool)
    .await
}

/// A post in #public-announcements, as the bot read it.
#[derive(Debug, Clone, PartialEq)]
pub struct Post {
    pub message_id: u64,
    /// The Discord Markdown as written.
    pub content: String,
    pub mentions: serde_json::Value,
    pub posted_at: OffsetDateTime,
    pub edited_at: Option<OffsetDateTime>,
}

/// Stores a post by `author`, or updates it when it is stored already (by whoever it was by then).
pub async fn upsert_announcement(pool: &PgPool, author: UserId, post: &Post) -> sqlx::Result<()> {
    sqlx::query!(
        r#"
        INSERT INTO discord_announcements
            (message_id, user_id, content, mentions, posted_at, edited_at)
        VALUES ($1, $2, $3, $4, $5, $6)
        ON CONFLICT (message_id) DO UPDATE SET
            content = EXCLUDED.content,
            mentions = EXCLUDED.mentions,
            edited_at = EXCLUDED.edited_at
        "#,
        post.message_id as i64,
        author.0,
        post.content,
        post.mentions,
        post.posted_at,
        post.edited_at,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Updates a post if it is stored, and stores nothing otherwise: for a post whose author has no
/// linked account (now), which is shown only if it was posted while they had one.
pub async fn update_announcement(pool: &PgPool, post: &Post) -> sqlx::Result<()> {
    sqlx::query!(
        r#"
        UPDATE discord_announcements
        SET content = $2, mentions = $3, edited_at = $4
        WHERE message_id = $1
        "#,
        post.message_id as i64,
        post.content,
        post.mentions,
        post.edited_at,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// A stored post, under the name of the member it is by.
#[derive(Debug, Clone, PartialEq)]
pub struct Announcement {
    pub post: Post,
    pub author: String,
}

pub async fn delete_announcements(pool: &PgPool, message_ids: &[u64]) -> sqlx::Result<()> {
    let ids: Vec<i64> = message_ids.iter().map(|&id| id as i64).collect();
    sqlx::query!(
        "DELETE FROM discord_announcements WHERE message_id = ANY($1)",
        &ids,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// After reading the channel's latest posts (`seen`), drops what was deleted while the bot was
/// away: every stored post from the oldest one read on that is not among them, or every stored
/// post not among them when `whole_channel` (the read reached the channel's first post).
pub async fn prune_announcements(
    pool: &PgPool,
    seen: &[u64],
    since: Option<OffsetDateTime>,
    whole_channel: bool,
) -> sqlx::Result<()> {
    let ids: Vec<i64> = seen.iter().map(|&id| id as i64).collect();
    sqlx::query!(
        r#"
        DELETE FROM discord_announcements
        WHERE message_id <> ALL($1)
          AND ($3 OR ($2::timestamptz IS NOT NULL AND posted_at >= $2))
        "#,
        &ids,
        since,
        whole_channel,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// How many posts are stored, and when the latest was posted.
pub async fn announcement_totals(pool: &PgPool) -> sqlx::Result<(i64, Option<OffsetDateTime>)> {
    let row = sqlx::query!(
        r#"SELECT count(*) AS "stored!", max(posted_at) AS latest FROM discord_announcements"#
    )
    .fetch_one(pool)
    .await?;
    Ok((row.stored, row.latest))
}

/// The latest `limit` posts, newest first.
pub async fn latest_announcements(pool: &PgPool, limit: i64) -> sqlx::Result<Vec<Announcement>> {
    let rows = sqlx::query!(
        r#"
        SELECT a.message_id, u.username, a.content, a.mentions, a.posted_at, a.edited_at
        FROM discord_announcements a
        JOIN users u ON u.id = a.user_id
        ORDER BY a.posted_at DESC, a.message_id DESC
        LIMIT $1
        "#,
        limit,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| Announcement {
            post: Post {
                message_id: row.message_id as u64,
                content: row.content,
                mentions: row.mentions,
                posted_at: row.posted_at,
                edited_at: row.edited_at,
            },
            author: row.username,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn post(id: u64, minutes: i64) -> Post {
        Post {
            message_id: id,
            content: format!("post {id}"),
            mentions: serde_json::json!({}),
            posted_at: OffsetDateTime::UNIX_EPOCH + time::Duration::minutes(minutes),
            edited_at: None,
        }
    }

    fn ids(posts: &[Announcement]) -> Vec<u64> {
        posts.iter().map(|stored| stored.post.message_id).collect()
    }

    #[sqlx::test]
    async fn keeps_announcements_newest_first_and_follows_edits(pool: PgPool) {
        let ey = member(&pool, "Ey", "officer").await;
        for (id, minutes) in [(1, 1), (2, 2), (3, 3)] {
            upsert_announcement(&pool, ey, &post(id, minutes))
                .await
                .unwrap();
        }
        assert_eq!(
            ids(&latest_announcements(&pool, 2).await.unwrap()),
            vec![3, 2]
        );

        let mut edited = post(2, 2);
        edited.content = "edited".into();
        edited.edited_at = Some(OffsetDateTime::UNIX_EPOCH + time::Duration::hours(1));
        upsert_announcement(&pool, ey, &edited).await.unwrap();
        let latest = latest_announcements(&pool, 10).await.unwrap();
        assert_eq!(
            latest[1],
            Announcement {
                post: edited,
                author: "Ey".into()
            }
        );

        delete_announcements(&pool, &[3]).await.unwrap();
        assert_eq!(
            ids(&latest_announcements(&pool, 10).await.unwrap()),
            vec![2, 1]
        );
    }

    #[sqlx::test]
    async fn prunes_only_what_the_read_covered(pool: PgPool) {
        let ey = member(&pool, "Ey", "officer").await;
        for (id, minutes) in [(1, 1), (2, 2), (3, 3), (4, 4)] {
            upsert_announcement(&pool, ey, &post(id, minutes))
                .await
                .unwrap();
        }
        // The read went back to post 3 and did not see 4: 4 was deleted, 1 and 2 are older.
        let since = Some(post(3, 3).posted_at);
        prune_announcements(&pool, &[3], since, false)
            .await
            .unwrap();
        assert_eq!(
            ids(&latest_announcements(&pool, 10).await.unwrap()),
            vec![3, 2, 1]
        );
        // A read of the whole channel that saw only 1: the rest went.
        prune_announcements(&pool, &[1], None, true).await.unwrap();
        assert_eq!(
            ids(&latest_announcements(&pool, 10).await.unwrap()),
            vec![1]
        );
        // An empty channel, read whole: nothing stays.
        prune_announcements(&pool, &[], None, true).await.unwrap();
        assert!(latest_announcements(&pool, 10).await.unwrap().is_empty());
    }

    async fn member(pool: &PgPool, username: &str, rank: &str) -> crate::users::UserId {
        let mut tx = pool.begin().await.unwrap();
        let user = crate::users::insert_user(
            &mut tx,
            &crate::users::CreateUserPayload {
                username: username.into(),
                email: None,
                first_name: None,
                last_name: None,
                is_superuser: false,
            },
            false,
        )
        .await
        .unwrap();
        sqlx::query!(
            "UPDATE users SET guild_rank = $2 WHERE id = $1",
            user.id.0,
            rank
        )
        .execute(&mut *tx)
        .await
        .unwrap();
        tx.commit().await.unwrap();
        user.id
    }

    #[sqlx::test]
    async fn finds_the_rank_behind_each_linked_discord_account(pool: PgPool) {
        let link = crate::auth::oauth2::providers::link_for_test;
        let officer = member(&pool, "Ey", "officer").await;
        let raider = member(&pool, "Raid", "raider").await;
        let gone = member(&pool, "Gone", "member").await;
        link(&pool, DISCORD_PROVIDER_SLUG, "111", officer).await;
        link(&pool, DISCORD_PROVIDER_SLUG, "222", raider).await;
        link(&pool, DISCORD_PROVIDER_SLUG, "333", gone).await;
        // A Battle.net account is not a Discord one.
        link(&pool, "battlenet", "Raid#1234", raider).await;
        sqlx::query!("UPDATE users SET disabled_at = now() WHERE id = $1", gone.0)
            .execute(&pool)
            .await
            .unwrap();

        let linked = linked_accounts(&pool).await.unwrap();
        let account = |subject: &str, username: &str, rank| LinkedAccount {
            subject: subject.into(),
            username: username.into(),
            rank,
        };
        assert_eq!(
            linked,
            vec![
                account("111", "Ey", Rank::Officer),
                account("222", "Raid", Rank::Raider)
            ]
        );
    }

    #[sqlx::test]
    async fn finds_the_member_behind_a_discord_account(pool: PgPool) {
        let link = crate::auth::oauth2::providers::link_for_test;
        let ey = member(&pool, "Ey", "officer").await;
        let gone = member(&pool, "Gone", "member").await;
        link(&pool, DISCORD_PROVIDER_SLUG, "111", ey).await;
        link(&pool, DISCORD_PROVIDER_SLUG, "222", gone).await;
        sqlx::query!("UPDATE users SET disabled_at = now() WHERE id = $1", gone.0)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(linked_member(&pool, 111).await.unwrap(), Some(ey));
        assert_eq!(linked_member(&pool, 222).await.unwrap(), None);
        assert_eq!(linked_member(&pool, 333).await.unwrap(), None);
    }

    #[sqlx::test]
    async fn an_update_without_a_member_stores_nothing_new(pool: PgPool) {
        let ey = member(&pool, "Ey", "officer").await;
        upsert_announcement(&pool, ey, &post(1, 1)).await.unwrap();
        // 1 is edited after its author unlinked; 2 was never by a linked member.
        let mut edited = post(1, 1);
        edited.content = "edited".into();
        update_announcement(&pool, &edited).await.unwrap();
        update_announcement(&pool, &post(2, 2)).await.unwrap();

        let latest = latest_announcements(&pool, 10).await.unwrap();
        assert_eq!(
            latest,
            vec![Announcement {
                post: edited,
                author: "Ey".into()
            }]
        );
    }

    #[sqlx::test]
    async fn keeps_the_settings_apart(pool: PgPool) {
        let stored = settings(&pool).await.unwrap();
        assert_eq!((stored.guild_id, stored.bot_token.is_some()), (None, false));
        set_guild_id(&pool, Some(123)).await.unwrap();
        set_bot_token(&pool, Some(&Ciphertext::for_test("sealed")))
            .await
            .unwrap();
        let stored = settings(&pool).await.unwrap();
        assert_eq!(stored.guild_id, Some(123));
        assert_eq!(stored.bot_token.as_ref().map(AsRef::as_ref), Some("sealed"));
        // Clearing one leaves the other.
        set_bot_token(&pool, None).await.unwrap();
        let stored = settings(&pool).await.unwrap();
        assert_eq!(
            (stored.guild_id, stored.bot_token.is_some()),
            (Some(123), false)
        );
    }

    #[sqlx::test]
    async fn keeps_the_bots_word_on_itself(pool: PgPool) {
        assert_eq!(status(&pool).await.unwrap().running_since, None);
        status_started(&pool, "Blood Legion#0001").await.unwrap();
        let running = status(&pool).await.unwrap();
        assert_eq!(running.bot_user.as_deref(), Some("Blood Legion#0001"));
        assert!(running.running_since.is_some() && running.seen_at.is_some());
        // A restart for new settings leaves the last error alone; a failure records its own.
        status_stopped(&pool, None).await.unwrap();
        status_stopped(&pool, Some("the gateway closed"))
            .await
            .unwrap();
        status_stopped(&pool, None).await.unwrap();
        let stopped = status(&pool).await.unwrap();
        assert_eq!(stopped.running_since, None);
        assert_eq!(stopped.last_error.as_deref(), Some("the gateway closed"));
        assert!(stopped.last_error_at.is_some());
    }

    #[sqlx::test]
    async fn remembers_what_it_made(pool: PgPool) {
        assert_eq!(object_id(&pool, "role:officer").await.unwrap(), None);
        set_object_id(&pool, "role:officer", u64::MAX >> 1)
            .await
            .unwrap();
        set_object_id(&pool, "role:officer", 42).await.unwrap();
        assert_eq!(object_id(&pool, "role:officer").await.unwrap(), Some(42));
    }
}
