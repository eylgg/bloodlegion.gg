//! What each class, spec, and talent brings a raid: the buffs, debuffs, and utility the raid
//! builder checks a composition for. Classic's, as a starting point to correct as Forever's
//! become known: each entry is data, so a change is an edit here.
//!
//! A character provides an effect when its class matches and, if the provider names one, the
//! spec it plays that night or a talent that spec has (see `launch::catalog` for the talents).

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Buff,
    Debuff,
    Utility,
}

/// Who a buff reaches: the whole raid, or only the provider's own group of five.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    Raid,
    Party,
}

#[derive(Debug, Serialize)]
pub struct Provider {
    pub class: &'static str,
    /// Only while playing this spec.
    pub spec: Option<&'static str>,
    /// Only with this talent.
    pub talent: Option<&'static str>,
}

#[derive(Debug, Serialize)]
pub struct Effect {
    pub slug: &'static str,
    pub name: &'static str,
    pub kind: Kind,
    pub scope: Scope,
    /// What it does, for grouping: `Stats`, `Armor reduction`, `Combat res`.
    pub category: &'static str,
    pub providers: &'static [Provider],
    /// Talents that make it better (Improved Battle Shout).
    pub improved_by: &'static [Provider],
    /// Effects sharing a key compete for the same casters: a paladin keeps one blessing on each
    /// class and one aura up, a warlock one curse on the target.
    pub exclusive: Option<&'static str>,
    pub note: Option<&'static str>,
}

const fn class(class: &'static str) -> Provider {
    Provider {
        class,
        spec: None,
        talent: None,
    }
}

const fn talent(class: &'static str, talent: &'static str) -> Provider {
    Provider {
        class,
        spec: None,
        talent: Some(talent),
    }
}

const NONE: &[Provider] = &[];

use Kind::{Buff, Debuff, Utility};
use Scope::{Party, Raid};

/// One effect; most fields default.
const fn effect(
    slug: &'static str,
    name: &'static str,
    kind: Kind,
    scope: Scope,
    category: &'static str,
    providers: &'static [Provider],
) -> Effect {
    Effect {
        slug,
        name,
        kind,
        scope,
        category,
        providers,
        improved_by: NONE,
        exclusive: None,
        note: None,
    }
}

const fn improved(mut effect: Effect, by: &'static [Provider]) -> Effect {
    effect.improved_by = by;
    effect
}

const fn exclusive(mut effect: Effect, key: &'static str) -> Effect {
    effect.exclusive = Some(key);
    effect
}

const fn noted(mut effect: Effect, note: &'static str) -> Effect {
    effect.note = Some(note);
    effect
}

pub const EFFECTS: &[Effect] = &[
    // --- raid-wide buffs ---
    effect(
        "arcane-intellect",
        "Arcane Intellect",
        Buff,
        Raid,
        "Stats",
        &[class("mage")],
    ),
    improved(
        effect(
            "fortitude",
            "Power Word: Fortitude",
            Buff,
            Raid,
            "Stats",
            &[class("priest")],
        ),
        &[talent("priest", "improved-power-word-fortitude")],
    ),
    effect(
        "divine-spirit",
        "Divine Spirit",
        Buff,
        Raid,
        "Stats",
        &[talent("priest", "divine-spirit")],
    ),
    effect(
        "shadow-protection",
        "Shadow Protection",
        Buff,
        Raid,
        "Resistances",
        &[class("priest")],
    ),
    improved(
        effect(
            "mark-of-the-wild",
            "Mark of the Wild",
            Buff,
            Raid,
            "Stats",
            &[class("druid")],
        ),
        &[talent("druid", "improved-mark-of-the-wild")],
    ),
    exclusive(
        improved(
            effect(
                "blessing-of-might",
                "Blessing of Might",
                Buff,
                Raid,
                "Attack power",
                &[class("paladin")],
            ),
            &[talent("paladin", "improved-blessing-of-might")],
        ),
        "blessing",
    ),
    exclusive(
        improved(
            effect(
                "blessing-of-wisdom",
                "Blessing of Wisdom",
                Buff,
                Raid,
                "Mana",
                &[class("paladin")],
            ),
            &[talent("paladin", "improved-blessing-of-wisdom")],
        ),
        "blessing",
    ),
    exclusive(
        effect(
            "blessing-of-kings",
            "Blessing of Kings",
            Buff,
            Raid,
            "Stats",
            &[talent("paladin", "blessing-of-kings")],
        ),
        "blessing",
    ),
    exclusive(
        effect(
            "blessing-of-salvation",
            "Blessing of Salvation",
            Buff,
            Raid,
            "Threat",
            &[class("paladin")],
        ),
        "blessing",
    ),
    exclusive(
        effect(
            "blessing-of-light",
            "Blessing of Light",
            Buff,
            Raid,
            "Healing",
            &[class("paladin")],
        ),
        "blessing",
    ),
    exclusive(
        effect(
            "blessing-of-sanctuary",
            "Blessing of Sanctuary",
            Buff,
            Raid,
            "Damage reduction",
            &[talent("paladin", "blessing-of-sanctuary")],
        ),
        "blessing",
    ),
    // --- group-only buffs ---
    improved(
        effect(
            "battle-shout",
            "Battle Shout",
            Buff,
            Party,
            "Attack power",
            &[class("warrior")],
        ),
        &[talent("warrior", "improved-battle-shout")],
    ),
    effect(
        "trueshot-aura",
        "Trueshot Aura",
        Buff,
        Party,
        "Attack power",
        &[talent("hunter", "trueshot-aura")],
    ),
    effect(
        "leader-of-the-pack",
        "Leader of the Pack",
        Buff,
        Party,
        "Critical strike",
        &[talent("druid", "leader-of-the-pack")],
    ),
    effect(
        "moonkin-aura",
        "Moonkin Aura",
        Buff,
        Party,
        "Spell",
        &[talent("druid", "moonkin-form")],
    ),
    improved(
        effect(
            "blood-pact",
            "Blood Pact",
            Buff,
            Party,
            "Stats",
            &[class("warlock")],
        ),
        &[talent("warlock", "improved-imp")],
    ),
    improved(
        effect(
            "strength-of-earth",
            "Strength of Earth Totem",
            Buff,
            Party,
            "Stats",
            &[class("shaman")],
        ),
        &[talent("shaman", "enhancing-totems")],
    ),
    improved(
        effect(
            "grace-of-air",
            "Grace of Air Totem",
            Buff,
            Party,
            "Stats",
            &[class("shaman")],
        ),
        &[talent("shaman", "enhancing-totems")],
    ),
    effect(
        "windfury-totem",
        "Windfury Totem",
        Buff,
        Party,
        "Attack speed",
        &[class("shaman")],
    ),
    effect(
        "mana-spring-totem",
        "Mana Spring Totem",
        Buff,
        Party,
        "Mana",
        &[class("shaman")],
    ),
    effect(
        "tranquil-air-totem",
        "Tranquil Air Totem",
        Buff,
        Party,
        "Threat",
        &[class("shaman")],
    ),
    effect(
        "mana-tide-totem",
        "Mana Tide Totem",
        Buff,
        Party,
        "Mana",
        &[talent("shaman", "mana-tide-totem")],
    ),
    exclusive(
        improved(
            effect(
                "devotion-aura",
                "Devotion Aura",
                Buff,
                Party,
                "Armor",
                &[class("paladin")],
            ),
            &[talent("paladin", "improved-devotion-aura")],
        ),
        "aura",
    ),
    exclusive(
        improved(
            effect(
                "concentration-aura",
                "Concentration Aura",
                Buff,
                Party,
                "Healing",
                &[class("paladin")],
            ),
            &[talent("paladin", "improved-concentration-aura")],
        ),
        "aura",
    ),
    exclusive(
        improved(
            effect(
                "retribution-aura",
                "Retribution Aura",
                Buff,
                Party,
                "Damage",
                &[class("paladin")],
            ),
            &[talent("paladin", "improved-retribution-aura")],
        ),
        "aura",
    ),
    effect(
        "vampiric-embrace",
        "Vampiric Embrace",
        Buff,
        Party,
        "Healing",
        &[talent("priest", "vampiric-embrace")],
    ),
    // --- debuffs on the target ---
    exclusive(
        effect(
            "sunder-armor",
            "Sunder Armor",
            Debuff,
            Raid,
            "Armor reduction",
            &[class("warrior")],
        ),
        "armor",
    ),
    exclusive(
        improved(
            effect(
                "expose-armor",
                "Expose Armor",
                Debuff,
                Raid,
                "Armor reduction",
                &[class("rogue")],
            ),
            &[talent("rogue", "improved-expose-armor")],
        ),
        "armor",
    ),
    effect(
        "faerie-fire",
        "Faerie Fire",
        Debuff,
        Raid,
        "Armor reduction",
        &[class("druid")],
    ),
    exclusive(
        effect(
            "curse-of-recklessness",
            "Curse of Recklessness",
            Debuff,
            Raid,
            "Armor reduction",
            &[class("warlock")],
        ),
        "curse",
    ),
    exclusive(
        effect(
            "curse-of-the-elements",
            "Curse of the Elements",
            Debuff,
            Raid,
            "Spell vulnerability",
            &[class("warlock")],
        ),
        "curse",
    ),
    exclusive(
        effect(
            "curse-of-shadow",
            "Curse of Shadow",
            Debuff,
            Raid,
            "Spell vulnerability",
            &[class("warlock")],
        ),
        "curse",
    ),
    effect(
        "shadow-weaving",
        "Shadow Weaving",
        Debuff,
        Raid,
        "Spell vulnerability",
        &[talent("priest", "shadow-weaving")],
    ),
    effect(
        "improved-shadow-bolt",
        "Improved Shadow Bolt",
        Debuff,
        Raid,
        "Spell vulnerability",
        &[talent("warlock", "improved-shadow-bolt")],
    ),
    effect(
        "improved-scorch",
        "Improved Scorch",
        Debuff,
        Raid,
        "Spell vulnerability",
        &[talent("mage", "improved-scorch")],
    ),
    effect(
        "winters-chill",
        "Winter's Chill",
        Debuff,
        Raid,
        "Spell vulnerability",
        &[talent("mage", "winters-chill")],
    ),
    effect(
        "stormstrike",
        "Stormstrike",
        Debuff,
        Raid,
        "Spell vulnerability",
        &[talent("shaman", "stormstrike")],
    ),
    improved(
        effect(
            "hunters-mark",
            "Hunter's Mark",
            Debuff,
            Raid,
            "Attack power",
            &[class("hunter")],
        ),
        &[talent("hunter", "improved-hunters-mark")],
    ),
    effect(
        "judgement-of-wisdom",
        "Judgement of Wisdom",
        Debuff,
        Raid,
        "Mana",
        &[class("paladin")],
    ),
    effect(
        "judgement-of-light",
        "Judgement of Light",
        Debuff,
        Raid,
        "Healing",
        &[class("paladin")],
    ),
    improved(
        effect(
            "demoralizing-shout",
            "Demoralizing Shout",
            Debuff,
            Raid,
            "Damage reduction",
            &[class("warrior")],
        ),
        &[talent("warrior", "improved-demoralizing-shout")],
    ),
    effect(
        "thunder-clap",
        "Thunder Clap",
        Debuff,
        Raid,
        "Damage reduction",
        &[class("warrior")],
    ),
    // --- utility ---
    effect(
        "rebirth",
        "Rebirth",
        Utility,
        Raid,
        "Combat res",
        &[class("druid")],
    ),
    noted(
        effect(
            "soulstone",
            "Soulstone",
            Utility,
            Raid,
            "Combat res",
            &[class("warlock")],
        ),
        "Cast before the pull.",
    ),
    effect(
        "innervate",
        "Innervate",
        Utility,
        Raid,
        "Cooldown",
        &[class("druid")],
    ),
    effect(
        "power-infusion",
        "Power Infusion",
        Utility,
        Raid,
        "Cooldown",
        &[talent("priest", "power-infusion")],
    ),
    effect(
        "remove-curse",
        "Remove Curse",
        Utility,
        Raid,
        "Dispel",
        &[class("mage"), class("druid")],
    ),
    effect(
        "dispel-magic",
        "Dispel Magic",
        Utility,
        Raid,
        "Dispel",
        &[class("priest"), class("paladin")],
    ),
    effect(
        "cure-poison",
        "Cure Poison",
        Utility,
        Raid,
        "Dispel",
        &[class("druid"), class("shaman"), class("paladin")],
    ),
    effect(
        "cure-disease",
        "Cure Disease",
        Utility,
        Raid,
        "Dispel",
        &[class("priest"), class("shaman"), class("paladin")],
    ),
    effect(
        "tranquilizing-shot",
        "Tranquilizing Shot",
        Utility,
        Raid,
        "Dispel",
        &[class("hunter")],
    ),
    effect(
        "interrupt",
        "Interrupt",
        Utility,
        Raid,
        "Interrupt",
        &[
            class("rogue"),
            class("warrior"),
            class("shaman"),
            class("mage"),
        ],
    ),
    effect(
        "tremor-totem",
        "Tremor Totem",
        Utility,
        Party,
        "Fear",
        &[class("shaman")],
    ),
    effect(
        "crowd-control",
        "Crowd control",
        Utility,
        Raid,
        "Crowd control",
        &[
            class("mage"),
            class("rogue"),
            class("warlock"),
            class("hunter"),
            class("druid"),
            class("priest"),
        ],
    ),
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::launch::catalog;

    #[test]
    fn every_provider_names_a_real_class_spec_and_talent() {
        let mut slugs: Vec<_> = EFFECTS.iter().map(|e| e.slug).collect();
        slugs.sort();
        slugs.dedup();
        assert_eq!(slugs.len(), EFFECTS.len(), "an effect slug repeats");
        for effect in EFFECTS {
            assert!(
                !effect.providers.is_empty(),
                "{} has no provider",
                effect.slug
            );
            for provider in effect.providers.iter().chain(effect.improved_by) {
                let class = catalog::find(provider.class)
                    .unwrap_or_else(|| panic!("{}: no class {}", effect.slug, provider.class));
                if let Some(spec) = provider.spec {
                    assert!(
                        class.specs.iter().any(|s| s.slug == spec),
                        "{}: {spec}",
                        effect.slug
                    );
                }
                if let Some(talent) = provider.talent {
                    assert!(
                        class.talents.iter().any(|t| t.slug == talent),
                        "{}: no {} talent {talent}",
                        effect.slug,
                        class.slug
                    );
                }
            }
        }
    }
}
