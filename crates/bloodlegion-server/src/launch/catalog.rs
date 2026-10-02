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

/// A talent worth knowing a character has: one that changes what they bring a raid (a buff, a
/// debuff, a cooldown). `tree` is the spec slug of the talent tree it sits in; any build can take
/// it, but a character of that spec usually has it.
#[derive(Debug, Serialize)]
pub struct Talent {
    pub slug: &'static str,
    pub name: &'static str,
    pub tree: &'static str,
}

#[derive(Debug, Serialize)]
pub struct Class {
    pub slug: &'static str,
    pub name: &'static str,
    /// `#rrggbb`, from `RAID_CLASS_COLORS`.
    pub color: &'static str,
    pub specs: &'static [Spec],
    /// The notable talents, from Classic until Forever's are known.
    pub talents: &'static [Talent],
}

use Role::{Healer, Melee, Ranged, Tank};

const fn talent(slug: &'static str, name: &'static str, tree: &'static str) -> Talent {
    Talent { slug, name, tree }
}

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
        talents: &[
            talent(
                "improved-mark-of-the-wild",
                "Improved Mark of the Wild",
                "restoration",
            ),
            talent("leader-of-the-pack", "Leader of the Pack", "feral"),
            talent("moonkin-form", "Moonkin Form", "balance"),
            talent("swiftmend", "Swiftmend", "restoration"),
            talent("natures-swiftness", "Nature's Swiftness", "restoration"),
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
        talents: &[
            talent("trueshot-aura", "Trueshot Aura", "marksmanship"),
            talent(
                "improved-hunters-mark",
                "Improved Hunter's Mark",
                "marksmanship",
            ),
            talent("bestial-wrath", "Bestial Wrath", "beast-mastery"),
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
        talents: &[
            talent("improved-scorch", "Improved Scorch", "fire"),
            talent("winters-chill", "Winter's Chill", "frost"),
            talent("arcane-power", "Arcane Power", "arcane"),
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
        talents: &[
            talent("blessing-of-kings", "Blessing of Kings", "protection"),
            talent(
                "blessing-of-sanctuary",
                "Blessing of Sanctuary",
                "protection",
            ),
            talent(
                "improved-blessing-of-might",
                "Improved Blessing of Might",
                "retribution",
            ),
            talent(
                "improved-blessing-of-wisdom",
                "Improved Blessing of Wisdom",
                "holy",
            ),
            talent(
                "improved-devotion-aura",
                "Improved Devotion Aura",
                "protection",
            ),
            talent(
                "improved-concentration-aura",
                "Improved Concentration Aura",
                "holy",
            ),
            talent(
                "improved-retribution-aura",
                "Improved Retribution Aura",
                "retribution",
            ),
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
        talents: &[
            talent(
                "improved-power-word-fortitude",
                "Improved Power Word: Fortitude",
                "discipline",
            ),
            talent("divine-spirit", "Divine Spirit", "discipline"),
            talent("power-infusion", "Power Infusion", "discipline"),
            talent("shadow-weaving", "Shadow Weaving", "shadow"),
            talent("vampiric-embrace", "Vampiric Embrace", "shadow"),
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
        talents: &[
            talent(
                "improved-expose-armor",
                "Improved Expose Armor",
                "assassination",
            ),
            talent("blade-flurry", "Blade Flurry", "combat"),
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
        talents: &[
            talent("enhancing-totems", "Enhancing Totems", "enhancement"),
            talent("stormstrike", "Stormstrike", "enhancement"),
            talent("mana-tide-totem", "Mana Tide Totem", "restoration"),
            talent("natures-swiftness", "Nature's Swiftness", "restoration"),
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
        talents: &[
            talent("improved-imp", "Improved Imp", "demonology"),
            talent(
                "improved-shadow-bolt",
                "Improved Shadow Bolt",
                "destruction",
            ),
            talent("soul-link", "Soul Link", "demonology"),
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
        talents: &[
            talent("improved-battle-shout", "Improved Battle Shout", "fury"),
            talent(
                "improved-demoralizing-shout",
                "Improved Demoralizing Shout",
                "fury",
            ),
            talent("last-stand", "Last Stand", "protection"),
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
        for class in CLASSES {
            let mut talents: Vec<_> = class.talents.iter().map(|t| t.slug).collect();
            talents.sort();
            talents.dedup();
            assert_eq!(
                talents.len(),
                class.talents.len(),
                "{} repeats a talent",
                class.slug
            );
            for talent in class.talents {
                assert!(
                    class.specs.iter().any(|s| s.slug == talent.tree),
                    "{} talent {} is in no tree of the class",
                    class.slug,
                    talent.slug
                );
            }
        }
        assert!(find("shaman").is_some());
        assert!(find("monk").is_none());
    }
}
