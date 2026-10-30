//! Test raids and loot, for `characters seed`: two past raids in each zone, attended by the test
//! characters, with loot from every boss. A raid counts as seeded when only test characters came
//! and won its loot, which is how a second seed skips a zone and how unseed finds them again.

use sqlx::PgPool;
use time::{Duration, OffsetDateTime};

use super::{LootInput, RaidError, RaidInput, catalog};
use crate::Result;

/// The pieces a boss's test items are named after ("Chillhowl Helm"), three per boss, rotating.
const PIECES: [&str; 12] = [
    "Helm",
    "Mantle",
    "Cloak",
    "Band",
    "Gauntlets",
    "Girdle",
    "Treads",
    "Talisman",
    "Blade",
    "Staff",
    "Bulwark",
    "Idol",
];

/// How many raids each zone gets, a week apart.
const RAIDS_PER_ZONE: usize = 2;

/// Every so many drops, nobody takes one (disenchanted).
const UNCLAIMED_EVERY: usize = 7;

#[derive(Debug, Default)]
pub struct SeedReport {
    pub raids: u64,
    pub loot: u64,
}

#[derive(Debug, Default)]
pub struct UnseedReport {
    pub raids: u64,
    pub items: u64,
}

/// A boss's three test items, the last rare and the others epic.
fn items(boss: &str, index: usize) -> [(String, &'static str); 3] {
    std::array::from_fn(|k| {
        let piece = PIECES[(index * 3 + k) % PIECES.len()];
        (
            format!("{boss} {piece}"),
            if k == 2 { "rare" } else { "epic" },
        )
    })
}

/// Makes the test raids of each zone that has none yet, from the test characters `characters`.
/// Every boss of the zone drops two of its three items each raid, a different two each time.
pub async fn seed(pool: &PgPool, characters: &[i64]) -> Result<SeedReport, RaidError> {
    let mut report = SeedReport::default();
    if characters.is_empty() {
        return Ok(report);
    }
    let mut characters = characters.to_vec();
    characters.sort_unstable();

    let seeded = seeded_raids(pool, &characters).await?;
    let bosses = super::bosses(pool).await?;
    let raid_time = crate::guild::settings(pool).await?.default_raid_time;
    let today = OffsetDateTime::now_utc().date();
    let mut drops = 0;

    for (z, zone) in catalog::ZONES.iter().enumerate() {
        if seeded.iter().any(|(_, slug)| slug == zone.slug) {
            continue;
        }
        let zone_bosses: Vec<_> = bosses.iter().filter(|b| b.zone == zone.slug).collect();
        for r in 0..RAIDS_PER_ZONE {
            let weeks_ago = (RAIDS_PER_ZONE - r) as i64;
            let day = today - Duration::weeks(weeks_ago) + Duration::days(z as i64);
            let raid = super::create_raid(
                pool,
                &RaidInput {
                    zone: zone.slug.to_string(),
                    starts_local: day.with_time(raid_time),
                },
            )
            .await?;
            report.raids += 1;

            let size = (zone.size as usize).min(characters.len());
            let offset = (z * 7 + r * 3) % characters.len();
            let attendees: Vec<i64> = characters
                .iter()
                .cycle()
                .skip(offset)
                .take(size)
                .copied()
                .collect();
            super::add_attendees(pool, raid.id, &attendees).await?;

            for (b, boss) in zone_bosses.iter().enumerate() {
                let skipped = (b + r) % 3;
                for (k, (name, quality)) in items(&boss.name, b).into_iter().enumerate() {
                    if k == skipped {
                        continue;
                    }
                    let character_id = if (drops + 1) % UNCLAIMED_EVERY == 0 {
                        None
                    } else {
                        Some(attendees[drops % attendees.len()])
                    };
                    drops += 1;
                    super::record_loot(
                        pool,
                        raid.id,
                        &LootInput {
                            boss_id: Some(boss.id),
                            character_id,
                            item_id: None,
                            item_name: Some(name),
                            item_quality: Some(quality.to_string()),
                            game_item_id: None,
                        },
                    )
                    .await?;
                    report.loot += 1;
                }
            }
        }
    }
    Ok(report)
}

/// Removes [`seed`]'s raids (their loot goes with them), then its items that nothing else uses.
pub async fn unseed(pool: &PgPool, characters: &[i64]) -> Result<UnseedReport, RaidError> {
    let bosses = super::bosses(pool).await?;
    let names: Vec<String> = catalog::ZONES
        .iter()
        .flat_map(|zone| {
            bosses
                .iter()
                .filter(|b| b.zone == zone.slug)
                .enumerate()
                .flat_map(|(b, boss)| items(&boss.name, b))
                .map(|(name, _)| name.to_lowercase())
                .collect::<Vec<_>>()
        })
        .collect();

    let mut tx = pool.begin().await?;
    let ids: Vec<i64> = seeded_raids(&mut *tx, characters)
        .await?
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    let raids = sqlx::query!("DELETE FROM raids WHERE id = ANY($1)", &ids)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    let items = sqlx::query!(
        r#"
        DELETE FROM items i
        WHERE i.name_normalized = ANY($1)
            AND NOT EXISTS (SELECT 1 FROM loot l WHERE l.item_id = i.id)
            AND NOT EXISTS (SELECT 1 FROM loot_priorities p WHERE p.item_id = i.id)
        "#,
        &names,
    )
    .execute(&mut *tx)
    .await?
    .rows_affected();
    tx.commit().await?;
    Ok(UnseedReport { raids, items })
}

/// The raids, with their zones, that only `characters` came to and won loot from.
async fn seeded_raids(
    conn: impl sqlx::PgExecutor<'_>,
    characters: &[i64],
) -> sqlx::Result<Vec<(i64, String)>> {
    let rows = sqlx::query!(
        r#"
        SELECT r.id, r.zone FROM raids r
        WHERE EXISTS (SELECT 1 FROM raid_attendees a WHERE a.raid_id = r.id)
            AND NOT EXISTS (
                SELECT 1 FROM raid_attendees a
                WHERE a.raid_id = r.id AND a.character_id <> ALL($1)
            )
            AND NOT EXISTS (
                SELECT 1 FROM loot l
                WHERE l.raid_id = r.id AND l.character_id <> ALL($1)
            )
        ORDER BY r.id
        "#,
        characters,
    )
    .fetch_all(conn)
    .await?;
    Ok(rows.into_iter().map(|row| (row.id, row.zone)).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::characters::tests::input;
    use crate::guild::{Rank, tests::member};

    async fn test_characters(pool: &PgPool) -> Vec<i64> {
        crate::characters::seed(pool).await.unwrap();
        crate::characters::seeded_ids(pool).await.unwrap()
    }

    #[sqlx::test]
    async fn every_boss_drops_loot_once_per_zone(pool: PgPool) {
        let characters = test_characters(&pool).await;
        let report = seed(&pool, &characters).await.unwrap();
        assert_eq!(report.raids, (catalog::ZONES.len() * RAIDS_PER_ZONE) as u64);

        let bosses = super::super::bosses(&pool).await.unwrap();
        assert_eq!(report.loot, (bosses.len() * RAIDS_PER_ZONE * 2) as u64);
        for boss in &bosses {
            let detail = super::super::boss_detail(&pool, boss.id).await.unwrap();
            assert_eq!(detail.kills, RAIDS_PER_ZONE as i64, "{}", boss.name);
            // Two kills of two drops from three items: one dropped twice, two once.
            let mut counts: Vec<i64> = detail.drops.iter().map(|d| d.count).collect();
            counts.sort_unstable();
            assert_eq!(counts, [1, 1, 2], "{}", boss.name);
        }
        assert!(
            super::super::db::loot(&pool, &Default::default())
                .await
                .unwrap()
                .iter()
                .any(|l| l.character_id.is_none()),
            "some loot goes unclaimed"
        );

        let again = seed(&pool, &characters).await.unwrap();
        assert_eq!((again.raids, again.loot), (0, 0));
    }

    #[sqlx::test]
    async fn unseed_keeps_real_raids(pool: PgPool) {
        let characters = test_characters(&pool).await;
        seed(&pool, &characters).await.unwrap();

        // A real raid a test character joined, with an item of a test item's name.
        let ey = member(&pool, "Ey", Rank::Raider).await;
        let real =
            crate::characters::tests::add(&pool, &ey, input("Arthas", "Menethil", "paladin", true))
                .await
                .unwrap();
        let raid = super::super::create_raid(
            &pool,
            &RaidInput {
                zone: "onyxias-lair".into(),
                starts_local: time::macros::datetime!(2026-12-10 20:00),
            },
        )
        .await
        .unwrap();
        super::super::add_attendees(&pool, raid.id, &[real.id, characters[0]])
            .await
            .unwrap();
        let onyxia = super::super::bosses(&pool)
            .await
            .unwrap()
            .into_iter()
            .find(|b| b.name == "Onyxia")
            .unwrap();
        super::super::record_loot(
            &pool,
            raid.id,
            &LootInput {
                boss_id: Some(onyxia.id),
                character_id: Some(real.id),
                item_id: None,
                item_name: Some("Onyxia Helm".into()),
                item_quality: None,
                game_item_id: None,
            },
        )
        .await
        .unwrap();

        let report = unseed(&pool, &characters).await.unwrap();
        assert_eq!(report.raids, (catalog::ZONES.len() * RAIDS_PER_ZONE) as u64);
        let left = super::super::raids(&pool).await.unwrap();
        assert_eq!(left.iter().map(|r| r.id).collect::<Vec<_>>(), [raid.id]);
        let items: Vec<String> = sqlx::query_scalar!("SELECT name FROM items")
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(items, ["Onyxia Helm"]);

        // Only the test character who raided with Arthas stays.
        assert_eq!(crate::characters::unseed(&pool).await.unwrap().1, 1);
    }
}
