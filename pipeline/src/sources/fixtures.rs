//! Bundled JSON fixtures for content the pipeline can't reliably scrape.
//!
//! ## Why this module exists
//!
//! Phase E of `docs/80-crafter-helper-v2-plan.md` requires the bundle to
//! carry desecrated mods (per-class, ≥ 11 for Body Armour) and Vaal-
//! corruption implicits (≥ 9 for Body Armour). poe2db.tw publishes both,
//! but each page changes layout per-patch and each scraper run requires
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
//! Edit the JSON files under `pipeline/data/`. Each file is human-edited;
//! the pipeline reads them at build time via [`include_str!`]. Adding a
//! new desecrated mod requires:
//!
//! 1. A unique stable engine `ModId` (PascalCase, prefixed by class).
//! 2. The lord owning the bone pool (`Amanamu` / `Kurgal` / `Ulaman`).
//! 3. The affix slot (`Prefix` / `Suffix`).
//! 4. The list of allowed item-class ids it can roll on.
//! 5. The `required_level` (matches in-game ilvl gate).
//! 6. The `stats` list with `(stat_id, min, max)` per stat output.

use poc2_data::SourceRevision;
use serde::{Deserialize, Serialize};

const DESECRATED_MODS_JSON: &str = include_str!("../../data/desecrated_mods.json");
const VAAL_IMPLICITS_JSON: &str = include_str!("../../data/vaal_implicits.json");

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
    pub lord: String,
    pub affix: String,
    pub classes: Vec<String>,
    pub tier: u32,
    pub required_level: u32,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DesecratedFile {
    entries: Vec<DesecratedFixtureEntry>,
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
    pub revision: SourceRevision,
}

impl FixtureSnapshot {
    pub fn count_summary(&self) -> String {
        format!(
            "fixtures: {} desecrated mods, {} vaal implicits",
            self.desecrated.len(),
            self.vaal_implicits.len(),
        )
    }
}

/// Load the bundled fixtures. Returns an error only when the JSON shipped
/// inside the binary is malformed — a true infallible path in practice
/// since the JSON is checked at every test run.
pub fn load() -> Result<FixtureSnapshot, serde_json::Error> {
    let desecrated: DesecratedFile = serde_json::from_str(DESECRATED_MODS_JSON)?;
    let vaal: VaalFile = serde_json::from_str(VAAL_IMPLICITS_JSON)?;
    Ok(FixtureSnapshot {
        desecrated: desecrated.entries,
        vaal_implicits: vaal.entries,
        revision: SourceRevision {
            name: "fixtures.embedded".into(),
            revision: format!(
                "desecrated={} vaal_implicits={}",
                DESECRATED_MODS_JSON.len(),
                VAAL_IMPLICITS_JSON.len()
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
        assert!(
            !snap.vaal_implicits.is_empty(),
            "vaal implicit fixture must not be empty"
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
        // Plan §5.E.3: BodyArmour expects ≥ 11 desecrated mods.
        assert!(
            body >= 11,
            "BodyArmour desecrated coverage too low: got {body}, want ≥ 11"
        );
    }

    #[test]
    fn body_armour_vaal_implicits_meet_phase_e_minimum() {
        let snap = load().expect("fixtures parse");
        let body = snap
            .vaal_implicits
            .iter()
            .filter(|e| e.classes.iter().any(|c| c == "BodyArmour"))
            .count();
        // Plan §5.E.3: BodyArmour expects ≥ 9 Vaal implicits.
        assert!(
            body >= 9,
            "BodyArmour Vaal implicit coverage too low: got {body}, want ≥ 9"
        );
    }
}
