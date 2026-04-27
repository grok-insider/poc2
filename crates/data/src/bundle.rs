//! Top-level bundle container.

use poc2_engine::{
    BaseType, ItemClass, ItemClassId, ModDefinition, ModKind, PatchVersion, Tag,
    ENGINE_SCHEMA_VERSION,
};
use serde::{Deserialize, Serialize};

use crate::concepts::{ConceptDefinition, ConceptMap};
use crate::error::{DataError, DataResult};
use crate::sources::SourceRevisions;
use crate::synergy::{SynergyEdge, SynergyOverride};
use crate::weights::WeightObservation;
use crate::BUNDLE_SCHEMA_VERSION;

/// Header section — versioning, build provenance, source revisions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleHeader {
    pub schema_version: u32,
    pub engine_schema: u32,
    pub game_patch: PatchVersion,
    /// ISO 8601 UTC timestamp.
    pub built_at: String,
    /// Build pipeline identifier (e.g., `pipeline@<git-sha>`).
    pub built_by: String,
    pub sources: SourceRevisions,
}

impl BundleHeader {
    pub fn validate(&self) -> DataResult<()> {
        if self.schema_version != BUNDLE_SCHEMA_VERSION {
            return Err(DataError::SchemaVersionMismatch {
                bundle: self.schema_version,
                expected: BUNDLE_SCHEMA_VERSION,
            });
        }
        if self.engine_schema != ENGINE_SCHEMA_VERSION {
            return Err(DataError::EngineSchemaMismatch {
                bundle: self.engine_schema,
                expected: ENGINE_SCHEMA_VERSION,
            });
        }
        Ok(())
    }
}

// -------------------------------------------------------------------------
// Per-section types — currencies / omens / essences / bones / catalysts
// land here once their engine types stabilize in M2.4-M2.6. For now, each
// is an opaque `serde_json::Value` so the bundle round-trips without
// constraining future schema.
// -------------------------------------------------------------------------

/// A pluggable section of the bundle. Used for content not yet in the
/// engine's typed surface (currencies/omens/essences/bones/catalysts come
/// in M2.4-M2.6). The pipeline emits these as JSON; the engine parses on
/// demand once typed.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BundleSection {
    /// Schema version of this section, independent of bundle schema.
    pub section_version: u32,
    /// Raw JSON content. Replaced with strongly-typed Vec<...> as each
    /// section graduates.
    pub entries: Vec<serde_json::Value>,
}

// -------------------------------------------------------------------------
// Bundle
// -------------------------------------------------------------------------

/// The full data bundle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bundle {
    pub header: BundleHeader,

    // Game ontology -------------------------------------------------------
    pub item_classes: Vec<ItemClass>,
    pub base_items: Vec<BaseType>,
    pub tags: Vec<Tag>,
    pub concepts: Vec<ConceptDefinition>,
    pub mods: Vec<ModDefinition>,

    // Crafting items (graduating to typed in M2.4-M2.6) ------------------
    #[serde(default)]
    pub currencies: BundleSection,
    #[serde(default)]
    pub omens: BundleSection,
    #[serde(default)]
    pub essences: BundleSection,
    #[serde(default)]
    pub bones: BundleSection,
    #[serde(default)]
    pub catalysts: BundleSection,

    // Cross-cutting -------------------------------------------------------
    /// `stat_id → translation template`. Keys mirror RePoE-fork's `stats[].id`.
    /// Populated in M2.3 (pipeline). Keep an empty `IndexMap` for now.
    #[serde(default)]
    pub stat_translations: indexmap::IndexMap<String, String>,
    #[serde(default)]
    pub weights: Vec<WeightObservation>,
    #[serde(default)]
    pub concept_map: ConceptMap,
    #[serde(default)]
    pub synergy_edges: Vec<SynergyEdge>,
    #[serde(default)]
    pub synergy_overrides: Vec<SynergyOverride>,
    /// Pre-computed `base → eligible mod ids` for advisor performance.
    /// Populated in M2.3.
    #[serde(default)]
    pub mods_by_base: indexmap::IndexMap<String, Vec<String>>,
}

impl Bundle {
    /// Construct an empty bundle for the given patch and build identifier.
    /// All sections are zero-sized; useful for tests and pipeline scaffolding.
    pub fn empty(game_patch: PatchVersion, built_by: impl Into<String>) -> Self {
        Self {
            header: BundleHeader {
                schema_version: BUNDLE_SCHEMA_VERSION,
                engine_schema: ENGINE_SCHEMA_VERSION,
                game_patch,
                built_at: now_iso8601(),
                built_by: built_by.into(),
                sources: SourceRevisions::default(),
            },
            item_classes: Vec::new(),
            base_items: Vec::new(),
            tags: Vec::new(),
            concepts: Vec::new(),
            mods: Vec::new(),
            currencies: BundleSection::default(),
            omens: BundleSection::default(),
            essences: BundleSection::default(),
            bones: BundleSection::default(),
            catalysts: BundleSection::default(),
            stat_translations: indexmap::IndexMap::new(),
            weights: Vec::new(),
            concept_map: ConceptMap::default(),
            synergy_edges: Vec::new(),
            synergy_overrides: Vec::new(),
            mods_by_base: indexmap::IndexMap::new(),
        }
    }

    /// Validate the bundle's structural invariants.
    ///
    /// Currently checks:
    /// - Schema versions match
    /// - Engine schema matches
    /// - (More invariants land in M2.3 once we have typed sections — dangling
    ///   references, mods-by-base consistency, weight scope coverage, etc.)
    pub fn validate(&self) -> DataResult<()> {
        self.header.validate()?;
        crate::validation::validate(self)
    }

    /// Patch this bundle declares.
    pub fn game_patch(&self) -> PatchVersion {
        self.header.game_patch
    }

    /// Iterate every mod with the requested `kind` whose
    /// `allowed_item_classes` contains `class`.
    ///
    /// Used by Phase E coverage tests and by the OutcomeDialog when it
    /// renders the per-class breakdown of the desecrated / Vaal-implicit
    /// pools (`docs/80-crafter-helper-v2-plan.md` §5).
    pub fn mods_by_kind_for_class<'a>(
        &'a self,
        class: &'a ItemClassId,
        kind: ModKind,
    ) -> impl Iterator<Item = &'a ModDefinition> + 'a {
        self.mods
            .iter()
            .filter(move |m| m.kind == kind && m.allowed_on(class))
    }

    /// Convenience: count mods of `kind` for `class`.
    pub fn count_mods_by_kind_for_class(&self, class: &ItemClassId, kind: ModKind) -> usize {
        self.mods_by_kind_for_class(class, kind).count()
    }

    /// Extract the bundle's essence catalogue as a typed list of
    /// engine [`poc2_engine::Essence`] presets ready to feed into the
    /// engine's [`poc2_engine::DefaultCurrencyResolver`].
    ///
    /// The bundle's `essences` section is a list of JSON entries with
    /// `{ id, name, corrupt, tooltip, tier_groups: [{ tiers: [{ engine_mod_id, ... }] }] }`.
    /// Each tier group is a per-base-class shape; we collapse them into
    /// a single canonical `(name, quality, target_mod)` per essence by
    /// taking the highest-ilvl tier of the first valid tier group.
    ///
    /// Quality is inferred from the name prefix (`Lesser` / blank =
    /// Greater / `Greater` / `Perfect` / `Corrupted`). Mod target uses
    /// the `engine_mod_id` field embedded in each tier entry.
    pub fn essence_catalogue(&self) -> Vec<poc2_engine::Essence> {
        let mut out = Vec::new();
        for entry in &self.essences.entries {
            let Some(name) = entry.get("name").and_then(|v| v.as_str()) else {
                continue;
            };
            let corrupt = entry
                .get("corrupt")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);
            let quality = quality_from_name(name, corrupt);
            // Find the first tier group with at least one entry, take
            // the highest-ilvl mod id.
            let target_mod = entry
                .get("tier_groups")
                .and_then(|v| v.as_array())
                .and_then(|groups| groups.iter().find_map(extract_target_mod_id));
            let Some(target_mod) = target_mod else {
                continue;
            };
            let id = format!(
                "{}{}",
                quality_prefix(quality),
                name.split_whitespace()
                    .filter(|w| ![
                        "Essence",
                        "of",
                        "the",
                        "Lesser",
                        "Greater",
                        "Perfect",
                        "Corrupted"
                    ]
                    .contains(w))
                    .collect::<String>()
            );
            // Use a Box::leak'd display name since Engine::Essence wants &'static str.
            let display: &'static str = Box::leak(name.to_string().into_boxed_str());
            out.push(poc2_engine::Essence::new(
                id,
                display,
                quality,
                poc2_engine::ids::ModId::from(target_mod),
            ));
        }
        out
    }

    /// Extract the bundle's catalyst catalogue as a typed list of
    /// engine [`poc2_engine::Catalyst`] presets.
    ///
    /// The catalyst's `tag` field comes from the first non-jewellery
    /// tag in the CoE pipe-string (e.g., `["life", "jewellery_attribute"]`
    /// → `"life"`).
    pub fn catalyst_catalogue(&self) -> Vec<poc2_engine::Catalyst> {
        let mut out = Vec::new();
        for entry in &self.catalysts.entries {
            let Some(name) = entry.get("name").and_then(|v| v.as_str()) else {
                continue;
            };
            let tags: Vec<String> = entry
                .get("tags")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|t| t.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            let primary_tag = tags
                .iter()
                .find(|t| !t.contains("jewellery"))
                .cloned()
                .or_else(|| tags.first().cloned());
            let Some(tag) = primary_tag else {
                continue;
            };
            let id_pascal: String = name
                .split_whitespace()
                .map(|w| {
                    let stripped = w.trim_end_matches('\'');
                    let mut chars = stripped.chars();
                    match chars.next() {
                        Some(c) => c.to_ascii_uppercase().to_string() + chars.as_str(),
                        None => String::new(),
                    }
                })
                .collect();
            let id = format!("{id_pascal}Catalyst");
            let display: &'static str = Box::leak(format!("{name} Catalyst").into_boxed_str());
            out.push(poc2_engine::Catalyst::new(id, display, tag));
        }
        out
    }
}

/// Heuristically infer essence quality from its name.
fn quality_from_name(name: &str, corrupt: bool) -> poc2_engine::EssenceQuality {
    if corrupt {
        return poc2_engine::EssenceQuality::Corrupted;
    }
    if name.starts_with("Lesser ") {
        poc2_engine::EssenceQuality::Lesser
    } else if name.starts_with("Greater ") {
        poc2_engine::EssenceQuality::Greater
    } else if name.starts_with("Perfect ") {
        poc2_engine::EssenceQuality::Perfect
    } else {
        // Bare "Essence of X" → Normal tier.
        poc2_engine::EssenceQuality::Normal
    }
}

/// Map quality back to the canonical id prefix used in
/// [`poc2_engine::DefaultCurrencyResolver`].
fn quality_prefix(q: poc2_engine::EssenceQuality) -> &'static str {
    match q {
        poc2_engine::EssenceQuality::Lesser => "LesserEssenceOf",
        poc2_engine::EssenceQuality::Normal => "EssenceOf",
        poc2_engine::EssenceQuality::Greater => "GreaterEssenceOf",
        poc2_engine::EssenceQuality::Perfect => "PerfectEssenceOf",
        poc2_engine::EssenceQuality::Corrupted => "CorruptedEssenceOf",
    }
}

fn extract_target_mod_id(group: &serde_json::Value) -> Option<String> {
    let tiers = group.get("tiers").and_then(|v| v.as_array())?;
    // Pick the highest-ilvl tier — proxy for the most representative mod.
    let best = tiers
        .iter()
        .filter_map(|t| {
            let id = t.get("engine_mod_id").and_then(|v| v.as_str())?;
            let ilvl: u32 = t
                .get("ilvl")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            Some((id.to_string(), ilvl))
        })
        .max_by_key(|(_, ilvl)| *ilvl);
    best.map(|(id, _)| id)
}

/// Best-effort ISO 8601 UTC timestamp without pulling in `chrono`.
///
/// Format: `YYYY-MM-DDTHH:MM:SSZ`. Sub-second precision discarded.
fn now_iso8601() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    iso8601_from_unix(secs)
}

/// Format a positive Unix timestamp as ISO 8601 (Howard Hinnant's algorithm).
///
/// Range: years 1970..=9999. Outside that we don't care.
#[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)]
fn iso8601_from_unix(secs: u64) -> String {
    let days = secs / 86_400;
    let secs_in_day = secs % 86_400;
    let hour = secs_in_day / 3600;
    let minute = (secs_in_day % 3600) / 60;
    let second = secs_in_day % 60;
    let (year, month, day) = ymd_from_days_since_epoch(days);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

/// Compute (Y, M, D) from the count of days since 1970-01-01 (Hinnant).
#[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)]
fn ymd_from_days_since_epoch(days: u64) -> (i64, u64, u64) {
    let z: i64 = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let mut year = (yoe as i64) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    if month <= 2 {
        year += 1;
    }
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_bundle_round_trips_through_json() {
        let b = Bundle::empty(PatchVersion::PATCH_0_4_0, "test@0000000");
        let json = serde_json::to_string(&b).unwrap();
        let back: Bundle = serde_json::from_str(&json).unwrap();
        assert_eq!(back.header.schema_version, BUNDLE_SCHEMA_VERSION);
        assert_eq!(back.header.engine_schema, ENGINE_SCHEMA_VERSION);
        assert_eq!(back.header.game_patch, PatchVersion::PATCH_0_4_0);
    }

    #[test]
    fn empty_bundle_validates() {
        let b = Bundle::empty(PatchVersion::PATCH_0_4_0, "test@0000000");
        b.validate().unwrap();
    }

    #[test]
    fn schema_version_mismatch_is_caught() {
        let mut b = Bundle::empty(PatchVersion::PATCH_0_4_0, "test@0000000");
        b.header.schema_version = 999;
        let err = b.validate().unwrap_err();
        assert!(matches!(err, DataError::SchemaVersionMismatch { .. }));
    }

    #[test]
    fn engine_schema_mismatch_is_caught() {
        let mut b = Bundle::empty(PatchVersion::PATCH_0_4_0, "test@0000000");
        b.header.engine_schema = 999;
        let err = b.validate().unwrap_err();
        assert!(matches!(err, DataError::EngineSchemaMismatch { .. }));
    }

    #[test]
    fn iso8601_well_known_dates() {
        // Unix epoch
        assert_eq!(iso8601_from_unix(0), "1970-01-01T00:00:00Z");
        // 2026-04-26T12:00:00Z (Apr 26 midnight = 1_777_161_600; +43200s = noon)
        assert_eq!(iso8601_from_unix(1_777_204_800), "2026-04-26T12:00:00Z");
        // 2000-02-29 (leap year)
        assert_eq!(iso8601_from_unix(951_782_400), "2000-02-29T00:00:00Z");
        // Boundaries
        assert_eq!(iso8601_from_unix(1_577_836_800), "2020-01-01T00:00:00Z");
        assert_eq!(iso8601_from_unix(2_524_608_000), "2050-01-01T00:00:00Z");
        // Time-of-day exactness
        assert_eq!(iso8601_from_unix(86_399), "1970-01-01T23:59:59Z");
        assert_eq!(iso8601_from_unix(86_400), "1970-01-02T00:00:00Z");
    }
}
