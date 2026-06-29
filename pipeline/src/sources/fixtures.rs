//! Bundled JSON fixtures for content the pipeline can't reliably scrape.
//!
//! ## Why this module exists
//!
//! Phase E of `docs/80-crafter-helper-v2-plan.md` requires the bundle to
//! carry desecrated mods (per-class, ≥ 11 for Body Armour). poe2db.tw
//! publishes the pools, but each page changes layout per-patch and each
//! scraper run requires
//! flaky network access. Live scraping makes the registry-coverage tests
//! unreliable and ties pipeline reproducibility to remote uptime.
//!
//! Instead, we ship a curated JSON fixture in `pipeline/data/` for each
//! class of mod. The fixture is hand-maintained from poe2db's published
//! tables (cross-checked against Craft of Exile's PoE2 trees) and lives
//! under version control. A future "real-scrape" path can populate the
//! same JSON shape at build time without changing the bundle ingestion.
//!
//! ## Adding new entries
//!
//! `desecrated_mods.json` is regenerated from poe2db's per-class
//! "Desecrated Modifiers" tables (0.5 "Return of the Ancients": equipment
//! mods all sit at ilvl 65 under the Amanamu/Kurgal/Ulaman lords; armour
//! classes are suffix-only; jewels carry the lord-less "Lightless" pool at
//! ilvl 1). The pipeline reads the JSON at build time via [`include_str!`].
//! Each desecrated entry requires:
//!
//! 1. A unique stable engine `ModId` (PascalCase: `Desecrated` + lord +
//!    family stem).
//! 2. The lord owning the bone pool (`Amanamu` / `Kurgal` / `Ulaman`),
//!    omitted for the jewel "Lightless" pool.
//! 3. The affix slot (`Prefix` / `Suffix`).
//! 4. The list of allowed item-class ids it can roll on.
//! 5. The `required_level` (matches in-game ilvl gate).
//! 6. The `stats` list with `(stat_id, min, max)` per stat output.
//!
//! `vaal_implicits.json` is retired as of 0.5: the real corruption pool
//! (`Corruption*` mods) ships via RePoE-fork, so the file stays empty.

use poc2_data::SourceRevision;
use serde::{Deserialize, Serialize};

const DESECRATED_MODS_JSON: &str = include_str!("../../data/desecrated_mods.json");
const VAAL_IMPLICITS_JSON: &str = include_str!("../../data/vaal_implicits.json");
const ALLOYS_JSON: &str = include_str!("../../data/alloys.json");
const EMOTIONS_JSON: &str = include_str!("../../data/emotions.json");

/// One stat output of a fixture mod.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixtureStat {
    pub stat_id: String,
    pub min: f64,
    pub max: f64,
}

/// One desecrated-mod fixture entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DesecratedFixtureEntry {
    pub id: String,
    pub name: String,
    /// `Amanamu` / `Kurgal` / `Ulaman` for equipment; `None` for the jewel
    /// "Lightless" pool, which has no owning lord.
    #[serde(default)]
    pub lord: Option<String>,
    pub affix: String,
    pub classes: Vec<String>,
    pub tier: u32,
    pub required_level: u32,
    /// Cleaned mod text with `(min-max)` ranges, for display.
    #[serde(default)]
    pub text: Option<String>,
    pub stats: Vec<FixtureStat>,
}

/// One Vaal-corruption implicit fixture entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaalImplicitFixtureEntry {
    pub id: String,
    pub name: String,
    pub classes: Vec<String>,
    pub required_level: u32,
    pub stats: Vec<FixtureStat>,
}

/// One class-specific crafted-mod target of a Verisium Alloy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlloyTargetEntry {
    /// Engine item-class id (PascalCase, e.g. `"Ring"`).
    pub class: String,
    /// The RePoE `Alloy*` mod id this alloy grants on that class.
    pub engine_mod_id: String,
}

/// One Verisium Alloy fixture entry (PoE2 0.5). Curated from poe2db's
/// per-alloy Class/Modifier tables joined against RePoE `Alloy*` mod ids
/// (132 class-targets across the 13 alloys; join script in docs/83 notes).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlloyFixtureEntry {
    /// GGPK metadata id, e.g. `Metadata/Items/Currency/CurrencyVerisiumAlloy1`.
    pub metadata_id: String,
    /// Stable engine currency id (PascalCase, e.g. `"RunicAlloy"`).
    pub id: String,
    /// Display name, e.g. `"Runic Alloy"`.
    pub name: String,
    #[serde(default)]
    pub drop_level: Option<u32>,
    pub targets: Vec<AlloyTargetEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DesecratedFile {
    entries: Vec<DesecratedFixtureEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AlloyFile {
    entries: Vec<AlloyFixtureEntry>,
}

/// One base-specific crafted-mod target of a Distilled Emotion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmotionTargetEntry {
    /// Jewel base name ("Ruby", "Time-Lost Sapphire", "Diamond", …).
    pub base: String,
    #[serde(default)]
    pub affix: String,
    /// Verbatim modifier text from poe2db (always present, for display).
    pub modifier: String,
    /// The RePoE mod id this emotion grants on that base. `None` when the
    /// mod is not yet exported upstream (entry stays display-only).
    #[serde(default)]
    pub engine_mod_id: Option<String>,
}

/// One Liquid / Potent / Ancient Emotion fixture entry (PoE2 0.5 jewel
/// crafting). Curated from poe2db item cards joined against the bundle's
/// jewel mod pool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmotionFixtureEntry {
    pub metadata_id: String,
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub drop_level: Option<u32>,
    /// "liquid" | "potent" | "ancient" | "ancient_potent".
    pub kind: String,
    pub targets: Vec<EmotionTargetEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EmotionFile {
    entries: Vec<EmotionFixtureEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct VaalFile {
    entries: Vec<VaalImplicitFixtureEntry>,
}

/// Parsed fixture bundle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixtureSnapshot {
    pub desecrated: Vec<DesecratedFixtureEntry>,
    pub vaal_implicits: Vec<VaalImplicitFixtureEntry>,
    pub alloys: Vec<AlloyFixtureEntry>,
    pub emotions: Vec<EmotionFixtureEntry>,
    pub revision: SourceRevision,
}

impl FixtureSnapshot {
    pub fn count_summary(&self) -> String {
        format!(
            "fixtures: {} desecrated mods, {} vaal implicits, {} alloys, {} emotions",
            self.desecrated.len(),
            self.vaal_implicits.len(),
            self.alloys.len(),
            self.emotions.len(),
        )
    }
}

/// Load the bundled fixtures. Returns an error only when the JSON shipped
/// inside the binary is malformed — a true infallible path in practice
/// since the JSON is checked at every test run.
pub fn load() -> Result<FixtureSnapshot, serde_json::Error> {
    let desecrated: DesecratedFile = serde_json::from_str(DESECRATED_MODS_JSON)?;
    let vaal: VaalFile = serde_json::from_str(VAAL_IMPLICITS_JSON)?;
    let alloys: AlloyFile = serde_json::from_str(ALLOYS_JSON)?;
    let emotions: EmotionFile = serde_json::from_str(EMOTIONS_JSON)?;
    Ok(FixtureSnapshot {
        desecrated: desecrated.entries,
        vaal_implicits: vaal.entries,
        alloys: alloys.entries,
        emotions: emotions.entries,
        revision: SourceRevision {
            name: "fixtures.embedded".into(),
            revision: format!(
                "desecrated={} vaal_implicits={} alloys={} emotions={}",
                DESECRATED_MODS_JSON.len(),
                VAAL_IMPLICITS_JSON.len(),
                ALLOYS_JSON.len(),
                EMOTIONS_JSON.len()
            ),
            url: None,
            fetched_at: String::new(),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixtures_parse_cleanly() {
        let snap = load().expect("fixtures parse");
        assert!(
            !snap.desecrated.is_empty(),
            "desecrated fixture must not be empty"
        );
        // The Vaal fixture is retired (0.5): the real corruption pool ships
        // via RePoE-fork `Corruption*` mods. Entries must stay empty so no
        // fabricated implicit ever re-enters the bundle through this path.
        assert!(
            snap.vaal_implicits.is_empty(),
            "vaal implicit fixture is retired and must stay empty"
        );
    }

    #[test]
    fn body_armour_desecrated_meets_phase_e_minimum() {
        let snap = load().expect("fixtures parse");
        let body = snap
            .desecrated
            .iter()
            .filter(|e| e.classes.iter().any(|c| c == "BodyArmour"))
            .count();
        // Plan §5.E.3: BodyArmour expects ≥ 11 desecrated mods (poe2db 0.5
        // ships 13 across the attribute pages).
        assert!(
            body >= 11,
            "BodyArmour desecrated coverage too low: got {body}, want ≥ 11"
        );
    }

    /// 0.5 invariant from poe2db: armour-slot desecrated pools contain
    /// suffixes only, and every equipment entry sits at ilvl 65 while the
    /// jewel "Lightless" pool (no lord) sits at ilvl 1.
    #[test]
    fn desecrated_pool_matches_poe2db_invariants() {
        let snap = load().expect("fixtures parse");
        let suffix_only = [
            "BodyArmour",
            "Helmet",
            "Gloves",
            "Boots",
            "Shield",
            "Buckler",
        ];
        for e in &snap.desecrated {
            if e.classes.iter().any(|c| suffix_only.contains(&c.as_str())) {
                assert_eq!(
                    e.affix, "Suffix",
                    "{}: armour-class desecrated mods are suffix-only",
                    e.id
                );
            }
            let is_jewel = e.classes.iter().all(|c| c == "Jewel");
            if is_jewel {
                assert_eq!(e.required_level, 1, "{}: jewel pool is ilvl 1", e.id);
                assert!(e.lord.is_none(), "{}: Lightless pool has no lord", e.id);
            } else {
                assert_eq!(e.required_level, 65, "{}: equipment pool is ilvl 65", e.id);
                assert!(
                    matches!(e.lord.as_deref(), Some("Amanamu" | "Kurgal" | "Ulaman")),
                    "{}: equipment desecrated mods belong to a lord",
                    e.id
                );
            }
        }
    }
}
