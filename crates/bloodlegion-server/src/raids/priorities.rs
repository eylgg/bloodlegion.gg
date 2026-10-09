//! Loot priorities: the officers' plan for who gets what. For each item a zone drops (or a kind of
//! item, "caster trinket"), the characters in line for it, first to last. Officers alone read and
//! write it.

use sqlx::types::Json;
use sqlx::{PgConnection, PgPool};

use super::{LootInput, RaidError, classify, external, resolve_item, zone};
use crate::Result;

/// The most characters one line holds.
pub const LINE_LIMIT: usize = 100;

/// One item (or kind of item) and who is in line for it.
#[derive(Debug, serde::Serialize)]
pub struct Priority {
    pub id: i64,
    pub zone: String,
    pub boss_id: Option<i64>,
    pub boss_name: Option<String>,
    /// A kind of item ("caster trinket"), when it is not one item.
    pub label: Option<String>,
    pub item_id: Option<i64>,
    pub item_name: Option<String>,
    pub item_quality: Option<String>,
    pub game_item_id: Option<i32>,
    pub item_icon: Option<String>,
    /// First in line first.
    pub characters: Json<Vec<InLine>>,
}

/// A character in line for an item.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct InLine {
    pub character_id: i64,
    pub user_id: Option<i64>,
    pub username: Option<String>,
    pub first_name: String,
    pub last_name: String,
    pub class: String,
    /// What they want it for ("dm"), as the officers put it.
    pub note: Option<String>,
    /// Whether they have won the item already.
    pub received: bool,
}

/// A new line: for an item (as loot names one: by id, mirror id, or name) or a kind of item
/// (`label`), in a zone, maybe from a boss there.
#[derive(Debug, serde::Deserialize)]
pub struct PriorityInput {
    pub zone: String,
    #[serde(default)]
    pub boss_id: Option<i64>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub item_id: Option<i64>,
    #[serde(default)]
    pub item_name: Option<String>,
    #[serde(default)]
    pub item_quality: Option<String>,
    #[serde(default)]
    pub game_item_id: Option<i32>,
}

/// What can change about a line once made: the boss it is under.
#[derive(Debug, serde::Deserialize)]
pub struct PriorityUpdate {
    #[serde(default)]
    pub boss_id: Option<i64>,
}

/// One place in a line, as `PUT /api/loot-priorities/{id}/characters` takes it.
#[derive(Debug, serde::Deserialize)]
pub struct LineInput {
    pub character_id: i64,
    #[serde(default)]
    pub note: Option<String>,
}

/// Every line, zone by zone, boss by boss (items before kinds of item, then oldest first).
pub async fn priorities(pool: &PgPool) -> sqlx::Result<Vec<Priority>> {
    select(pool, None).await
}

pub async fn create(pool: &PgPool, input: &PriorityInput) -> Result<Priority, RaidError> {
    let zone = zone(&input.zone)?;
    let mut tx = pool.begin().await?;
    check_boss(&mut tx, input.boss_id, zone.slug).await?;
    let label = input
        .label
        .as_deref()
        .map(str::trim)
        .filter(|l| !l.is_empty());
    let id = match label {
        Some(label) => {
            if label.chars().count() > 64 {
                return external(RaidError::InvalidPriority);
            }
            insert(&mut tx, zone.slug, input.boss_id, None, Some(label)).await?
        }
        None => {
            let item = LootInput {
                boss_id: None,
                character_id: None,
                item_id: input.item_id,
                item_name: input.item_name.clone(),
                item_quality: input.item_quality.clone(),
                game_item_id: input.game_item_id,
            };
            let item_id = resolve_item(&mut tx, &item).await?;
            insert(&mut tx, zone.slug, input.boss_id, Some(item_id), None).await?
        }
    };
    tx.commit().await?;
    Ok(find(pool, id).await?.expect("the line was just made"))
}

pub async fn update(pool: &PgPool, id: i64, input: &PriorityUpdate) -> Result<Priority, RaidError> {
    let mut tx = pool.begin().await?;
    let Some(zone) = sqlx::query_scalar!(
        "SELECT zone FROM loot_priorities WHERE id = $1 FOR UPDATE",
        id
    )
    .fetch_optional(&mut *tx)
    .await?
    else {
        return external(RaidError::PriorityNotFound);
    };
    check_boss(&mut tx, input.boss_id, &zone).await?;
    sqlx::query!(
        "UPDATE loot_priorities SET boss_id = $2 WHERE id = $1",
        id,
        input.boss_id
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(find(pool, id).await?.expect("the line was just updated"))
}

pub async fn delete(pool: &PgPool, id: i64) -> Result<(), RaidError> {
    let result = sqlx::query!("DELETE FROM loot_priorities WHERE id = $1", id)
        .execute(pool)
        .await?;
    if result.rows_affected() == 0 {
        return external(RaidError::PriorityNotFound);
    }
    Ok(())
}

/// Replaces who is in line, first to last.
pub async fn set_line(pool: &PgPool, id: i64, line: &[LineInput]) -> Result<Priority, RaidError> {
    let mut seen = std::collections::HashSet::new();
    let mut notes = Vec::with_capacity(line.len());
    for place in line {
        let note = place
            .note
            .as_deref()
            .map(str::trim)
            .filter(|n| !n.is_empty());
        if !seen.insert(place.character_id) || note.is_some_and(|n| n.chars().count() > 32) {
            return external(RaidError::InvalidLine);
        }
        notes.push(note.map(str::to_string));
    }
    if line.len() > LINE_LIMIT {
        return external(RaidError::InvalidLine);
    }
    let mut tx = pool.begin().await?;
    let exists = sqlx::query_scalar!(
        "SELECT id FROM loot_priorities WHERE id = $1 FOR UPDATE",
        id
    )
    .fetch_optional(&mut *tx)
    .await?;
    if exists.is_none() {
        return external(RaidError::PriorityNotFound);
    }
    sqlx::query!(
        "DELETE FROM loot_priority_characters WHERE priority_id = $1",
        id
    )
    .execute(&mut *tx)
    .await?;
    let characters: Vec<i64> = line.iter().map(|p| p.character_id).collect();
    sqlx::query!(
        r#"
        INSERT INTO loot_priority_characters (priority_id, character_id, position, note)
        SELECT $1, character_id, position::smallint, note
        FROM unnest($2::bigint[], $3::text[]) WITH ORDINALITY AS l (character_id, note, position)
        "#,
        id,
        &characters,
        &notes as &[Option<String>],
    )
    .execute(&mut *tx)
    .await
    .map_err(classify)?;
    tx.commit().await?;
    Ok(find(pool, id).await?.expect("the line exists"))
}

/// A boss, when one is named, must be in the zone.
async fn check_boss(
    conn: &mut PgConnection,
    boss_id: Option<i64>,
    zone: &str,
) -> Result<(), RaidError> {
    let Some(boss_id) = boss_id else {
        return Ok(());
    };
    let found = sqlx::query_scalar!("SELECT zone FROM bosses WHERE id = $1", boss_id)
        .fetch_optional(&mut *conn)
        .await?;
    match found {
        None => external(RaidError::BossNotFound),
        Some(z) if z != zone => external(RaidError::BossNotInZone),
        Some(_) => Ok(()),
    }
}

async fn insert(
    conn: &mut PgConnection,
    zone: &str,
    boss_id: Option<i64>,
    item_id: Option<i64>,
    label: Option<&str>,
) -> Result<i64, RaidError> {
    sqlx::query_scalar!(
        r#"
        INSERT INTO loot_priorities (zone, boss_id, item_id, label)
        VALUES ($1, $2, $3, $4)
        RETURNING id
        "#,
        zone,
        boss_id,
        item_id,
        label,
    )
    .fetch_one(&mut *conn)
    .await
    .map_err(classify)
}

async fn find(pool: &PgPool, id: i64) -> sqlx::Result<Option<Priority>> {
    Ok(select(pool, Some(id)).await?.pop())
}

async fn select(pool: &PgPool, id: Option<i64>) -> sqlx::Result<Vec<Priority>> {
    sqlx::query_as!(
        Priority,
        r#"
        SELECT p.id, p.zone, p.boss_id, b.name AS "boss_name?", p.label, p.item_id,
               i.name AS "item_name?", i.quality AS "item_quality?",
               i.game_item_id AS "game_item_id?", g.icon AS "item_icon?",
               COALESCE((
                   SELECT jsonb_agg(jsonb_build_object(
                       'character_id', c.id,
                       'user_id', c.user_id,
                       'username', u.username,
                       'first_name', c.first_name,
                       'last_name', c.last_name,
                       'class', c.class,
                       'note', pc.note,
                       'received', EXISTS (
                           SELECT 1 FROM loot l
                           WHERE l.item_id = p.item_id AND l.character_id = c.id
                       )
                   ) ORDER BY pc.position)
                   FROM loot_priority_characters pc
                   JOIN characters c ON c.id = pc.character_id
                   LEFT JOIN users u ON u.id = c.user_id
                   WHERE pc.priority_id = p.id
               ), '[]') AS "characters!: Json<Vec<InLine>>"
        FROM loot_priorities p
        LEFT JOIN bosses b ON b.id = p.boss_id
        LEFT JOIN items i ON i.id = p.item_id
        LEFT JOIN game_items g ON g.id = i.game_item_id
        WHERE $1::bigint IS NULL OR p.id = $1
        ORDER BY p.zone, b.id NULLS LAST, p.label IS NOT NULL, p.id
        "#,
        id,
    )
    .fetch_all(pool)
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Error;
    use crate::characters::tests::input as character;
    use crate::guild::{Rank, tests::member};
    use crate::raids::{
        BossInput, RaidInput, add_attendees, create_boss, create_raid, record_loot,
    };

    fn item(zone: &str, boss_id: Option<i64>, name: &str) -> PriorityInput {
        PriorityInput {
            zone: zone.into(),
            boss_id,
            label: None,
            item_id: None,
            item_name: Some(name.into()),
            item_quality: None,
            game_item_id: None,
        }
    }

    fn line(ids: &[i64]) -> Vec<LineInput> {
        ids.iter()
            .map(|&character_id| LineInput {
                character_id,
                note: None,
            })
            .collect()
    }

    async fn characters(pool: &PgPool, names: &[&str]) -> Vec<i64> {
        let officer = member(pool, "Officer", Rank::Officer).await;
        let mut ids = Vec::new();
        for name in names {
            let mut input = character(name, "Line", "mage", false);
            input.user_id = Some(None);
            ids.push(
                crate::characters::create(pool, &officer, &input)
                    .await
                    .unwrap()
                    .id,
            );
        }
        ids
    }

    #[sqlx::test]
    async fn a_line_keeps_its_order_and_who_has_won(pool: PgPool) {
        let ids = characters(&pool, &["Ana", "Bea", "Cy"]).await;
        let onyxia = sqlx::query_scalar!("SELECT id FROM bosses WHERE name = 'Onyxia'")
            .fetch_one(&pool)
            .await
            .unwrap();
        let priority = create(&pool, &item("onyxias-lair", Some(onyxia), "Head of Onyxia"))
            .await
            .unwrap();
        assert_eq!(priority.boss_name.as_deref(), Some("Onyxia"));
        assert_eq!(priority.item_name.as_deref(), Some("Head of Onyxia"));

        let mut places = line(&[ids[2], ids[0], ids[1]]);
        places[0].note = Some(" fury ".into());
        let priority = set_line(&pool, priority.id, &places).await.unwrap();
        let order: Vec<_> = priority.characters.iter().map(|c| c.character_id).collect();
        assert_eq!(order, [ids[2], ids[0], ids[1]]);
        assert_eq!(priority.characters[0].note.as_deref(), Some("fury"));
        assert!(priority.characters.iter().all(|c| !c.received));

        // Cy wins it: still in line, marked as having it.
        let raid = create_raid(
            &pool,
            &RaidInput {
                zone: "onyxias-lair".into(),
                starts_local: time::macros::datetime!(2026-12-10 20:00),
            },
        )
        .await
        .unwrap();
        add_attendees(&pool, raid.id, &ids).await.unwrap();
        record_loot(
            &pool,
            raid.id,
            &LootInput {
                boss_id: Some(onyxia),
                character_id: Some(ids[2]),
                item_id: priority.item_id,
                item_name: None,
                item_quality: None,
                game_item_id: None,
            },
        )
        .await
        .unwrap();
        let priority = find(&pool, priority.id).await.unwrap().unwrap();
        assert!(priority.characters[0].received);
        assert!(!priority.characters[1].received);

        // Reordering replaces the line.
        let priority = set_line(&pool, priority.id, &line(&[ids[1]]))
            .await
            .unwrap();
        assert_eq!(priority.characters.len(), 1);
        assert_eq!(priority.characters[0].character_id, ids[1]);
    }

    #[sqlx::test]
    async fn lines_are_checked(pool: PgPool) {
        let ids = characters(&pool, &["Ana"]).await;
        let priority = create(&pool, &item("hyjal-summit", None, "Warglaive"))
            .await
            .unwrap();

        // One line per item in a zone.
        assert!(matches!(
            create(&pool, &item("hyjal-summit", None, "Warglaive")).await,
            Err(Error::External(RaidError::PriorityExists))
        ));
        // A boss of another zone.
        let boss = create_boss(
            &pool,
            &BossInput {
                zone: "barrow-deeps".into(),
                name: "Barrow King".into(),
            },
        )
        .await
        .unwrap();
        assert!(matches!(
            create(&pool, &item("hyjal-summit", Some(boss.id), "Other")).await,
            Err(Error::External(RaidError::BossNotInZone))
        ));
        assert!(matches!(
            update(
                &pool,
                priority.id,
                &PriorityUpdate {
                    boss_id: Some(boss.id)
                }
            )
            .await,
            Err(Error::External(RaidError::BossNotInZone))
        ));
        // Someone twice, or a character that does not exist.
        assert!(matches!(
            set_line(&pool, priority.id, &line(&[ids[0], ids[0]])).await,
            Err(Error::External(RaidError::InvalidLine))
        ));
        assert!(matches!(
            set_line(&pool, priority.id, &line(&[ids[0] + 1000])).await,
            Err(Error::External(RaidError::UnknownCharacter))
        ));

        // A kind of item, not an item.
        let kind = create(
            &pool,
            &PriorityInput {
                label: Some("caster trinket".into()),
                ..item("hyjal-summit", None, "")
            },
        )
        .await
        .unwrap();
        assert_eq!(kind.label.as_deref(), Some("caster trinket"));
        assert_eq!(kind.item_id, None);

        // Items come before kinds of item.
        let all = priorities(&pool).await.unwrap();
        assert_eq!(
            all.iter().map(|p| p.id).collect::<Vec<_>>(),
            [priority.id, kind.id]
        );

        delete(&pool, kind.id).await.unwrap();
        assert!(matches!(
            delete(&pool, kind.id).await,
            Err(Error::External(RaidError::PriorityNotFound))
        ));
    }
}
