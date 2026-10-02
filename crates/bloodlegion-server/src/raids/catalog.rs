//! WoW: Forever's raid zones and their sizes, smallest first. The database mirrors the sizes in
//! `raid_zone_size()` so it can cap a raid's attendance; a test keeps the two in step.

use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct Zone {
    pub slug: &'static str,
    pub name: &'static str,
    /// How many characters the raid holds.
    pub size: i64,
}

pub const ZONES: &[Zone] = &[
    Zone {
        slug: "barrow-deeps",
        name: "Barrow Deeps",
        size: 10,
    },
    Zone {
        slug: "hyjal-summit",
        name: "Hyjal Summit",
        size: 20,
    },
    Zone {
        slug: "onyxias-lair",
        name: "Onyxia's Lair",
        size: 40,
    },
];

pub fn find(slug: &str) -> Option<&'static Zone> {
    ZONES.iter().find(|zone| zone.slug == slug)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[sqlx::test]
    async fn the_database_knows_every_zone_and_its_size(pool: sqlx::PgPool) {
        for zone in ZONES {
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
