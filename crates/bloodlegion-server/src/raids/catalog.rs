//! WoW: Forever's raid zones, smallest first, with their sizes and bosses. They are a file in the
//! repository (`data/raids.json`), embedded, and synced to the database on every migrate
//! ([`sync`]) like the item snapshot, so each deploy has the raids of its own commit. The database
//! keeps each zone's size (`raid_zones`) so it can cap a raid's attendance.

use std::collections::HashSet;
use std::sync::LazyLock;

use anyhow::Context;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

/// The raid zones, smallest first, each with its bosses in encounter order.
const RAIDS: &str = include_str!("../../data/raids.json");

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Zone {
    pub slug: String,
    pub name: String,
    /// How many characters the raid holds: whole groups of five.
    pub size: i64,
    /// In encounter order.
    #[serde(skip_serializing)]
    pub bosses: Vec<String>,
}

/// The zones of `data/raids.json`, which a test checks parses.
pub static ZONES: LazyLock<Vec<Zone>> =
    LazyLock::new(|| parse(RAIDS).expect("data/raids.json is valid"));

pub fn find(slug: &str) -> Option<&'static Zone> {
    ZONES.iter().find(|zone| zone.slug == slug)
}

/// Reads a raids file: unique slugs, sizes in whole groups, unique bosses within a zone.
fn parse(raids: &str) -> anyhow::Result<Vec<Zone>> {
    let zones: Vec<Zone> = serde_json::from_str(raids).context("reading data/raids.json")?;
    let mut slugs = HashSet::new();
    for zone in &zones {
        anyhow::ensure!(slugs.insert(&zone.slug), "{}: listed twice", zone.slug);
        anyhow::ensure!(
            zone.size > 0 && zone.size % 5 == 0,
            "{}: the size must be whole groups of five",
            zone.slug
        );
        let mut bosses = HashSet::new();
        for boss in &zone.bosses {
            anyhow::ensure!(bosses.insert(boss), "{}: {boss} listed twice", zone.slug);
        }
    }
    Ok(zones)
}

/// What [`sync`] did to the zones or the bosses.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Synced {
    /// Ones the database did not have yet.
    pub added: u64,
    /// Ones no longer listed, gone.
    pub removed: u64,
    /// Ones no longer listed, kept because something refers to them.
    pub kept: u64,
}

/// What [`sync`] did.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct SyncedRaids {
    pub zones: Synced,
    pub bosses: Synced,
}

/// Makes the database's zones and bosses the file's, in order. A zone or boss the file no longer
/// has goes, unless something refers to it (a zone's raids, bosses, or priorities; a boss's loot or
/// priorities): then it stays, with no position, so its history keeps its name.
pub async fn sync(pool: &PgPool) -> sqlx::Result<SyncedRaids> {
    sync_zones(pool, &ZONES).await
}

/// The advisory lock that keeps one sync running across instances.
const SYNC_LOCK_KEY: i64 = 0x7261_6964; // "raid"

async fn sync_zones(pool: &PgPool, listed: &[Zone]) -> sqlx::Result<SyncedRaids> {
    let (mut slugs, mut zone_names, mut sizes, mut zone_positions) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let (mut zones, mut names, mut positions) = (Vec::new(), Vec::new(), Vec::new());
    for (position, zone) in (0..).zip(listed) {
        slugs.push(zone.slug.clone());
        zone_names.push(zone.name.clone());
        sizes.push(zone.size as i32);
        zone_positions.push(position);
        for (position, name) in (0..).zip(&zone.bosses) {
            zones.push(zone.slug.clone());
            names.push(name.clone());
            positions.push(position);
        }
    }
    let mut tx = pool.begin().await?;
    // Instances starting together sync one at a time: the others then find nothing to change.
    sqlx::query!("SELECT pg_advisory_xact_lock($1)", SYNC_LOCK_KEY)
        .execute(&mut *tx)
        .await?;
    let mut synced = SyncedRaids::default();
    synced.zones.added = sqlx::query_scalar!(
        r#"
        INSERT INTO raid_zones (slug, name, size, position)
        SELECT * FROM UNNEST($1::text[], $2::text[], $3::int[], $4::int[])
        ON CONFLICT (slug) DO UPDATE
            SET name = EXCLUDED.name, size = EXCLUDED.size, position = EXCLUDED.position
        RETURNING (xmax = 0) AS "inserted!"
        "#,
        &slugs,
        &zone_names,
        &sizes,
        &zone_positions,
    )
    .fetch_all(&mut *tx)
    .await?
    .into_iter()
    .filter(|&inserted| inserted)
    .count() as u64;
    synced.bosses.added = sqlx::query_scalar!(
        r#"
        INSERT INTO bosses (zone, name, position)
        SELECT * FROM UNNEST($1::text[], $2::text[], $3::int[])
        ON CONFLICT (zone, name) DO UPDATE SET position = EXCLUDED.position
        RETURNING (xmax = 0) AS "inserted!"
        "#,
        &zones,
        &names,
        &positions,
    )
    .fetch_all(&mut *tx)
    .await?
    .into_iter()
    .filter(|&inserted| inserted)
    .count() as u64;
    synced.bosses.removed = sqlx::query!(
        r#"
        DELETE FROM bosses b
        WHERE (b.zone, b.name) NOT IN (SELECT * FROM UNNEST($1::text[], $2::text[]))
            AND NOT EXISTS (SELECT 1 FROM loot l WHERE l.boss_id = b.id)
            AND NOT EXISTS (SELECT 1 FROM loot_priorities p WHERE p.boss_id = b.id)
        "#,
        &zones,
        &names,
    )
    .execute(&mut *tx)
    .await?
    .rows_affected();
    synced.bosses.kept = sqlx::query!(
        r#"
        UPDATE bosses b SET position = NULL
        WHERE (b.zone, b.name) NOT IN (SELECT * FROM UNNEST($1::text[], $2::text[]))
        "#,
        &zones,
        &names,
    )
    .execute(&mut *tx)
    .await?
    .rows_affected();
    synced.zones.removed = sqlx::query!(
        r#"
        DELETE FROM raid_zones z
        WHERE NOT (z.slug = ANY($1))
            AND NOT EXISTS (SELECT 1 FROM raids r WHERE r.zone = z.slug)
            AND NOT EXISTS (SELECT 1 FROM bosses b WHERE b.zone = z.slug)
            AND NOT EXISTS (SELECT 1 FROM loot_priorities p WHERE p.zone = z.slug)
        "#,
        &slugs,
    )
    .execute(&mut *tx)
    .await?
    .rows_affected();
    synced.zones.kept = sqlx::query!(
        "UPDATE raid_zones SET position = NULL WHERE NOT (slug = ANY($1))",
        &slugs,
    )
    .execute(&mut *tx)
    .await?
    .rows_affected();
    tx.commit().await?;
    Ok(synced)
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn onyxia_raid(pool: &PgPool) -> crate::raids::Raid {
        crate::raids::create_raid(
            pool,
            &crate::raids::RaidInput {
                zone: "onyxias-lair".into(),
                starts_local: time::macros::datetime!(2026-12-10 20:00),
            },
        )
        .await
        .unwrap()
    }

    #[sqlx::test]
    async fn the_bosses_follow_the_file(pool: PgPool) {
        // Officers' bosses from before: one without loot, one with loot, one with a priority.
        sqlx::query!(
            "INSERT INTO bosses (zone, name) VALUES ('barrow-deeps', 'Barrow King'), \
             ('onyxias-lair', 'Old Onyxia'), ('hyjal-summit', 'Old Bandalar')"
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query!(
            "INSERT INTO loot_priorities (zone, boss_id, label) \
             SELECT zone, id, 'caster trinket' FROM bosses WHERE name = 'Old Bandalar'"
        )
        .execute(&pool)
        .await
        .unwrap();
        let raid = onyxia_raid(&pool).await;
        sqlx::query!("INSERT INTO items (name, quality) VALUES ('Head of Onyxia', 'epic')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query!(
            "INSERT INTO loot (raid_id, boss_id, item_id) SELECT $1, b.id, i.id FROM bosses b, items i \
             WHERE b.name = 'Old Onyxia'",
            raid.id
        )
        .execute(&pool)
        .await
        .unwrap();

        let synced = sync(&pool).await.unwrap();
        assert_eq!(
            synced.bosses,
            Synced {
                added: 0,
                removed: 1,
                kept: 2
            }
        );
        let names: Vec<_> = crate::raids::bosses(&pool)
            .await
            .unwrap()
            .into_iter()
            .map(|b| b.name)
            .collect();
        // Each zone's in encounter order, then the ones kept for their loot or priority.
        assert_eq!(
            names.len(),
            ZONES.iter().map(|z| z.bosses.len()).sum::<usize>() + 2
        );
        assert_eq!(names[names.len() - 2..], ["Old Bandalar", "Old Onyxia"]);
        assert!(!names.contains(&"Barrow King".to_string()));
        for zone in ZONES.iter() {
            let order: Vec<_> = names.iter().filter(|n| zone.bosses.contains(n)).collect();
            assert_eq!(
                order,
                zone.bosses.iter().collect::<Vec<_>>(),
                "{}",
                zone.slug
            );
        }

        // A boss missing from the database comes back; a second sync changes nothing.
        sqlx::query!("DELETE FROM bosses WHERE name = 'Chillhowl'")
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(sync(&pool).await.unwrap().bosses.added, 1);
        assert_eq!(
            sync(&pool).await.unwrap(),
            SyncedRaids {
                zones: Synced::default(),
                bosses: Synced {
                    added: 0,
                    removed: 0,
                    kept: 2
                }
            }
        );
    }

    #[sqlx::test]
    async fn the_zones_follow_the_file(pool: PgPool) {
        // Onyxia's Lair has a raid, so it stays when the file drops it; the Barrow Deeps go.
        onyxia_raid(&pool).await;
        let listed = parse(
            r#"[
                {"slug": "hyjal-summit", "name": "Mount Hyjal", "size": 25, "bosses": ["Bandalar"]},
                {"slug": "molten-core", "name": "Molten Core", "size": 40, "bosses": ["Ragnaros"]}
            ]"#,
        )
        .unwrap();
        let synced = sync_zones(&pool, &listed).await.unwrap();
        assert_eq!(
            synced.zones,
            Synced {
                added: 1,
                removed: 1,
                kept: 1
            }
        );
        let zones: Vec<(String, String, i32, Option<i32>)> = sqlx::query!(
            "SELECT slug, name, size, position FROM raid_zones ORDER BY position NULLS LAST"
        )
        .fetch_all(&pool)
        .await
        .unwrap()
        .into_iter()
        .map(|z| (z.slug, z.name, z.size, z.position))
        .collect();
        assert_eq!(
            zones,
            [
                ("hyjal-summit".into(), "Mount Hyjal".into(), 25, Some(0)),
                ("molten-core".into(), "Molten Core".into(), 40, Some(1)),
                ("onyxias-lair".into(), "Onyxia's Lair".into(), 40, None),
            ]
        );
        let bosses: Vec<String> =
            sqlx::query_scalar!("SELECT name FROM bosses ORDER BY zone, position")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(bosses, ["Bandalar", "Ragnaros"]);
    }

    #[test]
    fn the_file_is_valid() {
        assert!(!ZONES.is_empty());
        for zone in ZONES.iter() {
            assert!(!zone.bosses.is_empty(), "{}", zone.slug);
        }
        for bad in [
            r#"[{"slug": "a", "name": "A", "size": 10, "bosses": []}, {"slug": "a", "name": "B", "size": 10, "bosses": []}]"#,
            r#"[{"slug": "a", "name": "A", "size": 12, "bosses": []}]"#,
            r#"[{"slug": "a", "name": "A", "size": 10, "bosses": ["X", "X"]}]"#,
            r#"[{"slug": "a", "name": "A", "size": 10}]"#,
        ] {
            assert!(parse(bad).is_err(), "{bad}");
        }
    }

    #[sqlx::test]
    async fn the_database_knows_every_zone_and_its_size(pool: PgPool) {
        sync(&pool).await.unwrap();
        for zone in ZONES.iter() {
            let size: Option<i32> = sqlx::query_scalar!("SELECT raid_zone_size($1)", zone.slug)
                .fetch_one(&pool)
                .await
                .unwrap();
            assert_eq!(size.map(i64::from), Some(zone.size), "{}", zone.slug);
        }
        let unknown: Option<i32> = sqlx::query_scalar!("SELECT raid_zone_size('molten-core')")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(unknown, None);
    }
}
