//! The WoW: Forever classes: the nine original classes, each with its three Classic talent trees
//! as the specs a member can pick. Icons are Blizzard's: class icons from its web CDN, and each
//! spec's icon the one the Classic client's `TalentTab` table gives its tree; they are vendored
//! under `apps/web/static/wow`, named `classes/<class>.jpg` and `specs/<class>-<spec>.jpg`. Colors are Blizzard's own `RAID_CLASS_COLORS` (the values the
//! game client defines in `ChrClasses.db2`), using the modern Shaman blue: Forever lets both
//! factions play Paladin and Shaman, which is why Classic's shared pink was retired in 2.0.1.

use serde::Serialize;

/// A spec's place in a raid, for the composition board: the in-game tank and healer roles, with
/// damage split by range the way raid leaders plan. Feral covers both bear and cat, so it fills
/// two.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Tank,
    Healer,
    Melee,
    Ranged,
}

#[derive(Debug, Serialize)]
pub struct Spec {
    pub slug: &'static str,
    pub name: &'static str,
    pub roles: &'static [Role],
}

#[derive(Debug, Serialize)]
pub struct Class {
    pub slug: &'static str,
    pub name: &'static str,
    /// `#rrggbb`, from `RAID_CLASS_COLORS`.
    pub color: &'static str,
    pub specs: &'static [Spec],
}

use Role::{Healer, Melee, Ranged, Tank};

const fn spec(slug: &'static str, name: &'static str, roles: &'static [Role]) -> Spec {
    Spec { slug, name, roles }
}

pub const CLASSES: &[Class] = &[
    Class {
        slug: "druid",
        name: "Druid",
        color: "#ff7c0a",
        specs: &[
            spec("balance", "Balance", &[Ranged]),
            spec("feral", "Feral Combat", &[Tank, Melee]),
            spec("restoration", "Restoration", &[Healer]),
        ],
    },
    Class {
        slug: "hunter",
        name: "Hunter",
        color: "#aad372",
        specs: &[
            spec("beast-mastery", "Beast Mastery", &[Ranged]),
            spec("marksmanship", "Marksmanship", &[Ranged]),
            spec("survival", "Survival", &[Ranged]),
        ],
    },
    Class {
        slug: "mage",
        name: "Mage",
        color: "#3fc7eb",
        specs: &[
            spec("arcane", "Arcane", &[Ranged]),
            spec("fire", "Fire", &[Ranged]),
            spec("frost", "Frost", &[Ranged]),
        ],
    },
    Class {
        slug: "paladin",
        name: "Paladin",
        color: "#f48cba",
        specs: &[
            spec("holy", "Holy", &[Healer]),
            spec("protection", "Protection", &[Tank]),
            spec("retribution", "Retribution", &[Melee]),
        ],
    },
    Class {
        slug: "priest",
        name: "Priest",
        color: "#ffffff",
        specs: &[
            spec("discipline", "Discipline", &[Healer]),
            spec("holy", "Holy", &[Healer]),
            spec("shadow", "Shadow", &[Ranged]),
        ],
    },
    Class {
        slug: "rogue",
        name: "Rogue",
        color: "#fff468",
        specs: &[
            spec("assassination", "Assassination", &[Melee]),
            spec("combat", "Combat", &[Melee]),
            spec("subtlety", "Subtlety", &[Melee]),
        ],
    },
    Class {
        slug: "shaman",
        name: "Shaman",
        color: "#0070dd",
        specs: &[
            spec("elemental", "Elemental", &[Ranged]),
            spec("enhancement", "Enhancement", &[Melee]),
            spec("restoration", "Restoration", &[Healer]),
        ],
    },
    Class {
        slug: "warlock",
        name: "Warlock",
        color: "#8788ee",
        specs: &[
            spec("affliction", "Affliction", &[Ranged]),
            spec("demonology", "Demonology", &[Ranged]),
            spec("destruction", "Destruction", &[Ranged]),
        ],
    },
    Class {
        slug: "warrior",
        name: "Warrior",
        color: "#c69b6d",
        specs: &[
            spec("arms", "Arms", &[Melee]),
            spec("fury", "Fury", &[Melee]),
            spec("protection", "Protection", &[Tank]),
        ],
    },
];

pub fn find(slug: &str) -> Option<&'static Class> {
    CLASSES.iter().find(|class| class.slug == slug)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_class_has_three_distinct_specs_and_a_color() {
        assert_eq!(CLASSES.len(), 9);
        for class in CLASSES {
            assert_eq!(class.specs.len(), 3, "{}", class.slug);
            let mut slugs: Vec<_> = class.specs.iter().map(|s| s.slug).collect();
            slugs.dedup();
            assert_eq!(slugs.len(), 3, "{} has duplicate specs", class.slug);
            assert!(class.color.len() == 7 && class.color.starts_with('#'));
            assert!(class.specs.iter().all(|s| !s.roles.is_empty()));
        }
        assert!(find("shaman").is_some());
        assert!(find("monk").is_none());
    }
}
