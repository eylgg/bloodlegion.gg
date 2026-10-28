//! The class and talent tree icons (`apps/web/static/wow/classes/<class>.jpg` and
//! `specs/<class>-<spec>.jpg`, see the backend's `wow::catalog`), from the render service by
//! the game's icon names: each class's `classicon_`, and each tree's the Classic client's
//! `TalentTab` table gives it. They need no token.

use std::path::Path;

use anyhow::{Context, Result, bail};

use super::api;

const CLASSES: &[&str] = &[
    "druid", "hunter", "mage", "paladin", "priest", "rogue", "shaman", "warlock", "warrior",
];

/// Each tree's file name and its icon.
const SPECS: &[(&str, &str)] = &[
    ("druid-balance", "spell_nature_starfall"),
    ("druid-feral", "ability_racial_bearform"),
    ("druid-restoration", "spell_nature_healingtouch"),
    ("hunter-beast-mastery", "ability_hunter_beasttaming"),
    ("hunter-marksmanship", "ability_marksmanship"),
    ("hunter-survival", "ability_hunter_swiftstrike"),
    ("mage-arcane", "spell_holy_magicalsentry"),
    ("mage-fire", "spell_fire_firebolt02"),
    ("mage-frost", "spell_frost_frostbolt02"),
    ("paladin-holy", "spell_holy_holybolt"),
    ("paladin-protection", "spell_holy_devotionaura"),
    ("paladin-retribution", "spell_holy_auraoflight"),
    ("priest-discipline", "spell_holy_wordfortitude"),
    ("priest-holy", "spell_holy_guardianspirit"),
    ("priest-shadow", "spell_shadow_shadowwordpain"),
    ("rogue-assassination", "ability_rogue_eviscerate"),
    ("rogue-combat", "ability_backstab"),
    ("rogue-subtlety", "ability_stealth"),
    ("shaman-elemental", "spell_nature_lightning"),
    ("shaman-enhancement", "spell_nature_lightningshield"),
    ("shaman-restoration", "spell_nature_magicimmunity"),
    ("warlock-affliction", "spell_shadow_deathcoil"),
    ("warlock-demonology", "spell_shadow_metamorphosis"),
    ("warlock-destruction", "spell_shadow_rainoffire"),
    ("warrior-arms", "ability_rogue_eviscerate"),
    ("warrior-fury", "ability_warrior_innerrage"),
    ("warrior-protection", "inv_shield_06"),
];

pub async fn sync(workspace: &Path, http: &reqwest::Client) -> Result<()> {
    let dir = workspace.join("apps/web/static/wow");
    let classes = CLASSES
        .iter()
        .map(|class| (format!("classes/{class}"), format!("classicon_{class}")));
    let specs = SPECS
        .iter()
        .map(|&(file, icon)| (format!("specs/{file}"), icon.to_string()));
    let mut written = 0;
    for (file, icon) in classes.chain(specs) {
        let Some(image) = api::render_icon(http, &icon).await? else {
            bail!("no icon {icon} on the render service, for {file}");
        };
        let path = dir.join(format!("{file}.jpg"));
        std::fs::write(&path, image).with_context(|| format!("writing {}", path.display()))?;
        written += 1;
    }
    eprintln!("classes: wrote {written} icons");
    Ok(())
}
