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
    /// Verisium Alloys (PoE2 0.5 "Return of the Ancients"). Data-driven like
    /// essences/catalysts: each entry binds an alloy currency id + display
    /// name to the crafted `engine_mod_id` it grants. `#[serde(default)]` so
    /// pre-0.5 bundles (which carry no alloys) round-trip unchanged.
    #[serde(default)]
    pub alloys: BundleSection,
    /// Distilled Emotions (PoE2 0.5) — Liquid / Potent / Ancient Emotions
    /// that replace a random mod on a Rare jewel with a guaranteed crafted
    /// modifier, keyed by jewel base ("Ruby" / "Time-Lost Sapphire" / …).
    /// Same degrade-gracefully contract as `alloys`.
    #[serde(default)]
    pub emotions: BundleSection,
    /// Genesis Tree (PoE2 0.5 "Return of the Ancients") — the Breach-attached
    /// "Brequel" crafting tree. UI-only advisor knowledge: entries are typed
    /// by a `"type"` discriminator — `"womb"` (the five branches + Wombgift
    /// metadata), `"node"` (one allocatable passive with computed layout
    /// position, edges, and human-readable description), and `"preset"`
    /// (curated per-goal node allocations with source citations). The engine
    /// never simulates births; the WASM `genesisTree` command serves this
    /// straight to the web UI. `#[serde(default)]` keeps older bundles valid.
    #[serde(default)]
    pub genesis: BundleSection,

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
            alloys: BundleSection::default(),
            emotions: BundleSection::default(),
            genesis: BundleSection::default(),
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
        // Mod-id → allowed classes index, used to filter aggregate-label
        // expansions ("Jewellery", "One-Handed Weapons") down to classes the
        // target mod can actually carry.
        let allowed_by_mod: std::collections::HashMap<&str, &[poc2_engine::ids::ItemClassId]> =
            self.mods
                .iter()
                .map(|m| (m.id.as_str(), m.allowed_item_classes.as_slice()))
                .collect();

        let mut out = Vec::new();
        for entry in &self.essences.entries {
            let Some(name) = entry.get("name").and_then(|v| v.as_str()) else {
                continue;
            };
            // The CoE source table mixes Verisium Alloys (and other
            // remove-add materials) into the essence rows. Those ship via
            // [`Self::alloy_catalogue`] with alloy apply semantics —
            // ingesting them here would fabricate pseudo-essences with
            // garbage ids (e.g. "Swift Alloy" → Normal-quality essence)
            // that the resolver can never resolve but that essence-based
            // reachability checks (corpus audit) wrongly count.
            if !name.contains("Essence") {
                continue;
            }
            let corrupt = entry
                .get("corrupt")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);
            let quality = quality_from_name(name, corrupt);
            // One EssenceTarget per (class, attribute-pool, mod): each CoE
            // tier-group label names a class scope (possibly an aggregate or
            // an attribute split); the group's highest-ilvl tier is the
            // granted mod for that scope.
            let mut targets: Vec<poc2_engine::EssenceTarget> = Vec::new();
            if let Some(groups) = entry.get("tier_groups").and_then(|v| v.as_array()) {
                for group in groups {
                    let Some(label) = group.get("label").and_then(|v| v.as_str()) else {
                        continue;
                    };
                    let Some(mod_id) = extract_target_mod_id(group) else {
                        continue;
                    };
                    let (classes, pool) = essence_label_scope(label);
                    if classes.is_empty() {
                        tracing::warn!(
                            essence = name,
                            label,
                            "unknown essence tier-group label — scope skipped"
                        );
                        continue;
                    }
                    let allowed = allowed_by_mod.get(mod_id.as_str()).copied();
                    for class in classes {
                        let class = poc2_engine::ids::ItemClassId::from(class);
                        // Aggregate labels over-approximate; intersect with
                        // the mod's own allowed classes when known.
                        if let Some(allowed) = allowed {
                            if !allowed.is_empty() && !allowed.contains(&class) {
                                continue;
                            }
                        }
                        let dup = targets.iter().any(|t| {
                            t.class == class
                                && t.attribute_pool == pool
                                && t.mod_id.as_str() == mod_id
                        });
                        if !dup {
                            targets.push(poc2_engine::EssenceTarget {
                                class,
                                attribute_pool: pool,
                                mod_id: poc2_engine::ids::ModId::from(mod_id.as_str()),
                            });
                        }
                    }
                }
            }
            if targets.is_empty() {
                continue;
            }
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
            out.push(poc2_engine::Essence::with_class_targets(
                id, display, quality, targets,
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

    /// Extract the bundle's Verisium Alloy catalogue as a typed list of
    /// engine [`poc2_engine::Alloy`]s ready to seed a
    /// [`poc2_engine::DefaultCurrencyResolver`] via `with_alloys`.
    ///
    /// Two entry shapes are accepted:
    /// - **v2 (class-targeted, production):** `{ id, name, targets: [{ class,
    ///   engine_mod_id }] }` — real alloys grant a *different* crafted mod per
    ///   item class (poe2db per-alloy tables).
    /// - **v1 (legacy single-target):** `{ id, name, engine_mod_id }`.
    ///
    /// Malformed entries are skipped (a partial bundle degrades to "this
    /// alloy can't be simulated yet" rather than crashing). The engine still
    /// enforces the 0.5+ patch gate at apply time, so seeding the catalogue
    /// on a pre-0.5 bundle is harmless.
    pub fn alloy_catalogue(&self) -> Vec<poc2_engine::Alloy> {
        let mut out = Vec::new();
        for entry in &self.alloys.entries {
            let Some(id) = entry.get("id").and_then(|v| v.as_str()) else {
                continue;
            };
            let name = entry.get("name").and_then(|v| v.as_str()).unwrap_or(id);
            if let Some(targets) = entry.get("targets").and_then(|v| v.as_array()) {
                let class_targets: Vec<(poc2_engine::ids::ItemClassId, poc2_engine::ids::ModId)> =
                    targets
                        .iter()
                        .filter_map(|t| {
                            let class = t.get("class").and_then(|v| v.as_str())?;
                            let m = t.get("engine_mod_id").and_then(|v| v.as_str())?;
                            Some((
                                poc2_engine::ids::ItemClassId::from(class),
                                poc2_engine::ids::ModId::from(m),
                            ))
                        })
                        .collect();
                if class_targets.is_empty() {
                    continue;
                }
                out.push(poc2_engine::Alloy::with_class_targets(
                    id,
                    name.to_string(),
                    class_targets,
                ));
            } else if let Some(target_mod) = entry.get("engine_mod_id").and_then(|v| v.as_str()) {
                out.push(poc2_engine::Alloy::new(
                    id,
                    name.to_string(),
                    poc2_engine::ids::ModId::from(target_mod),
                ));
            }
        }
        out
    }

    /// Extract the bundle's Distilled Emotion catalogue (Liquid / Potent /
    /// Ancient Emotions, 0.5) as base-targeted engine
    /// [`poc2_engine::Alloy`]s — emotions reuse the alloy remove-then-add
    /// crafted-mod mechanic, keyed by jewel base name instead of class.
    ///
    /// Targets whose `engine_mod_id` is `null` (mod not exported upstream
    /// yet) are skipped; an emotion with zero bound targets is omitted.
    pub fn emotion_catalogue(&self) -> Vec<poc2_engine::Alloy> {
        let mut out = Vec::new();
        for entry in &self.emotions.entries {
            let Some(id) = entry.get("id").and_then(|v| v.as_str()) else {
                continue;
            };
            let name = entry.get("name").and_then(|v| v.as_str()).unwrap_or(id);
            let Some(targets) = entry.get("targets").and_then(|v| v.as_array()) else {
                continue;
            };
            let base_targets: Vec<(String, poc2_engine::ids::ModId)> = targets
                .iter()
                .filter_map(|t| {
                    let base = t.get("base").and_then(|v| v.as_str())?;
                    let m = t.get("engine_mod_id").and_then(|v| v.as_str())?;
                    Some((base.to_string(), poc2_engine::ids::ModId::from(m)))
                })
                .collect();
            if base_targets.is_empty() {
                continue;
            }
            out.push(poc2_engine::Alloy::with_base_targets(
                id,
                name.to_string(),
                base_targets,
            ));
        }
        out
    }
}

/// The six Vaal-corrupted essences (poe2db
/// `Metadata/Items/Currency/CurrencyCorruptedEssence*`; remove-a-random-mod
/// then add on RARE items). CoE exports them with `corrupt == "0"`, so
/// classification must key on this name set — the flag alone under-reports.
const CORRUPTED_ESSENCE_NAMES: [&str; 6] = [
    "Essence of the Abyss",
    "Essence of Delirium",
    "Essence of Horror",
    "Essence of Hysteria",
    "Essence of Insanity",
    "Essence of the Breach",
];

/// Heuristically infer essence quality from its name. The CoE `corrupt`
/// flag is kept as an additional signal, but the corrupted set is matched
/// by name (see [`CORRUPTED_ESSENCE_NAMES`]).
fn quality_from_name(name: &str, corrupt: bool) -> poc2_engine::EssenceQuality {
    if corrupt || CORRUPTED_ESSENCE_NAMES.contains(&name) {
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

/// Map a CoE essence tier-group label to engine item classes + an optional
/// attribute-pool refinement.
///
/// Label shapes observed in the live data: exact classes (`"Bow"`,
/// `"Warstaff"`), pluralised classes (`"Helmets"`, `"Jewels"`), attribute
/// splits (`"Body Armour (STR/INT)"`), element-flavoured weapon families
/// that share one class (`"Fire Wand"`, `"Ice Staff"`), and aggregates
/// (`"Jewellery"`, `"Offhands"`, `"One-Handed Weapons"`). Unknown labels
/// return an empty class list (skipped by the caller).
fn essence_label_scope(label: &str) -> (Vec<&'static str>, Option<poc2_engine::AttributePool>) {
    use poc2_engine::AttributePool as P;
    // CoE files dex/int "shields" under the Shield umbrella, but PoE2
    // models them as separate item classes: Bucklers (DEX) and Foci (INT).
    match label {
        "Shield (DEX)" => return (vec!["Buckler"], None),
        "Shield (INT)" => return (vec!["Focus"], None),
        _ => {}
    }
    let (core, pool) = match label.rfind(" (") {
        Some(i) if label.ends_with(')') => {
            let pool = match &label[i + 2..label.len() - 1] {
                "STR" => Some(P::Str),
                "DEX" => Some(P::Dex),
                "INT" => Some(P::Int),
                "STR/DEX" => Some(P::StrDex),
                "STR/INT" => Some(P::StrInt),
                "DEX/INT" => Some(P::DexInt),
                _ => None,
            };
            // Unrecognized parenthetical → treat the whole label as core.
            if pool.is_some() {
                (&label[..i], pool)
            } else {
                (label, None)
            }
        }
        _ => (label, None),
    };
    // Element-flavoured wand/staff families collapse onto their class.
    let core = core
        .strip_prefix("Fire ")
        .or_else(|| core.strip_prefix("Ice "))
        .or_else(|| core.strip_prefix("Lightning "))
        .or_else(|| core.strip_prefix("Chaos "))
        .or_else(|| core.strip_prefix("Physical "))
        .filter(|rest| matches!(*rest, "Wand" | "Staff"))
        .unwrap_or(core);
    // "Time-Lost Ruby" etc. collapse onto the plain jewel base name.
    let core = core.strip_prefix("Time-Lost ").unwrap_or(core);
    let classes: Vec<&'static str> = match core {
        // "Grasping Mail" (0.5 Abyssal body armour) is its own CoE base
        // row but shares the BodyArmour class.
        "Body Armour" | "Body Armours" | "Grasping Mail" => vec!["BodyArmour"],
        "Helmet" | "Helmets" => vec!["Helmet"],
        "Boots" => vec!["Boots"],
        "Gloves" => vec!["Gloves"],
        "Bow" | "Bows" => vec!["Bow"],
        "Crossbow" | "Crossbows" => vec!["Crossbow"],
        "Two Hand Sword" => vec!["TwoHandSword"],
        "Two Hand Axe" => vec!["TwoHandAxe"],
        "Two Hand Mace" => vec!["TwoHandMace"],
        "Warstaff" | "Quarterstaff" => vec!["Warstaff"],
        "Staff" | "Staves" => vec!["Staff"],
        "Wand" | "Wands" => vec!["Wand"],
        "Sceptre" | "Sceptres" => vec!["Sceptre"],
        "Spear" | "Spears" => vec!["Spear"],
        "Flail" | "Flails" => vec!["Flail"],
        "One Hand Axe" => vec!["OneHandAxe"],
        "One Hand Mace" => vec!["OneHandMace"],
        "One Hand Sword" => vec!["OneHandSword"],
        "Claw" | "Claws" => vec!["Claw"],
        "Dagger" | "Daggers" => vec!["Dagger"],
        "Talisman" | "Talismans" => vec!["Talisman"],
        "Focus" | "Foci" => vec!["Focus"],
        // Concrete jewel bases (Ruby/Emerald/Sapphire, plus Time-Lost
        // variants via the prefix strip above) share the Jewel class.
        "Jewel" | "Jewels" | "Ruby" | "Emerald" | "Sapphire" => vec!["Jewel"],
        "Quiver" | "Quivers" => vec!["Quiver"],
        "Ring" | "Rings" => vec!["Ring"],
        "Amulet" | "Amulets" => vec!["Amulet"],
        "Belt" | "Belts" => vec!["Belt"],
        "Shield" | "Shields" => vec!["Shield"],
        "Buckler" | "Bucklers" => vec!["Buckler"],
        "Life Flask" | "Life Flasks" => vec!["LifeFlask"],
        "Mana Flask" | "Mana Flasks" => vec!["ManaFlask"],
        "Jewellery" => vec!["Ring", "Amulet", "Belt"],
        "Offhands" => vec!["Shield", "Buckler", "Focus"],
        "One-Handed Weapons" => vec![
            "OneHandSword",
            "OneHandAxe",
            "OneHandMace",
            "Claw",
            "Dagger",
            "Wand",
            "Sceptre",
            "Spear",
            "Flail",
        ],
        // Mirrors CoE bgroup 7 membership (Talisman is filed under
        // Two-Handed there); over-approximation is safe — the catalogue
        // intersects with each mod's allowed classes.
        "Two-Handed Weapons" => vec![
            "TwoHandSword",
            "TwoHandAxe",
            "TwoHandMace",
            "Warstaff",
            "Staff",
            "Bow",
            "Crossbow",
            "Talisman",
        ],
        "Flasks" => vec!["LifeFlask", "ManaFlask"],
        // No craftable mod pools for these classes yet (bundle ships no
        // flask/charm/tablet/waystone domains) — map them anyway; the engine
        // simply never builds items of these classes today.
        "Charm" | "Charms" => vec!["UtilityFlask"],
        "Tablets" => vec!["TowerAugmentation"],
        // Waystone tier rows ("Low Tier (1-5)" / … / "Uber Tier"): the
        // "(1-5)" parenthetical is not an attribute pool, so the full label
        // reaches this match unchanged.
        "Low Tier (1-5)" | "Mid Tier (6-10)" | "Top Tier (11-15)" | "Uber Tier" | "Waystones" => {
            vec!["Waystone"]
        }
        // "Precursor Tablet" plus its flavoured rows (Breach/Ritual/…).
        c if c.ends_with("Precursor Tablet") => vec!["TowerAugmentation"],
        _ => vec![],
    };
    (classes, pool)
}

/// Known limitation: "Essence of the Abyss" tiers carry TWO engine mods per
/// class (`EssenceAbyssPrefix` + `EssenceAbyssSuffix` — together "Mark of
/// the Abyssal Lord"), but this picks a single mod per group, dropping one
/// half of the pair. Lifting this needs `EssenceTarget` to grant a mod SET.
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
    fn legacy_bundle_is_rejected_with_rebuild_guidance() {
        // M14.7a / P6 — a bundle built against the previous schema version
        // must error with a message pointing at the rebuild command. The
        // desktop loader matches on this error to surface a structured
        // "rebuild bundle" hint to the user. Parameterized on the current
        // schema so it survives future version bumps.
        let prev = crate::BUNDLE_SCHEMA_VERSION - 1;
        let mut b = Bundle::empty(PatchVersion::PATCH_0_4_0, "test@legacy");
        b.header.schema_version = prev;
        let err = b.validate().unwrap_err();
        let msg = err.to_string();
        assert!(
            matches!(
                err,
                DataError::SchemaVersionMismatch { bundle, expected }
                    if bundle == prev && expected == crate::BUNDLE_SCHEMA_VERSION
            ),
            "expected SchemaVersionMismatch{{{prev}, {}}}; got {err:?}",
            crate::BUNDLE_SCHEMA_VERSION
        );
        assert!(
            msg.contains(&format!("v{prev}"))
                && msg.contains(&format!("v{}", crate::BUNDLE_SCHEMA_VERSION)),
            "error message should mention both versions; got {msg}"
        );
        assert!(
            msg.contains("poc2-pipeline") && msg.contains("build"),
            "error message should reference the rebuild command; got {msg}"
        );
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

    // -----------------------------------------------------------------
    // 0.5 — essence quality + tier-group label scope
    // -----------------------------------------------------------------

    #[test]
    fn corrupted_essences_classify_by_name_despite_clear_flag() {
        // CoE exports the six corrupted essences with corrupt == "0"; the
        // name set must force Corrupted regardless.
        for name in [
            "Essence of the Abyss",
            "Essence of Delirium",
            "Essence of Horror",
            "Essence of Hysteria",
            "Essence of Insanity",
            "Essence of the Breach",
        ] {
            assert_eq!(
                quality_from_name(name, false),
                poc2_engine::EssenceQuality::Corrupted,
                "{name} must classify as Corrupted with flag unset"
            );
        }
        // The flag stays an additional signal for any future export fix.
        assert_eq!(
            quality_from_name("Essence of the Abyss", true),
            poc2_engine::EssenceQuality::Corrupted
        );
    }

    #[test]
    fn quality_from_name_prefix_ladder_unchanged() {
        use poc2_engine::EssenceQuality as Q;
        assert_eq!(
            quality_from_name("Lesser Essence of Flames", false),
            Q::Lesser
        );
        assert_eq!(quality_from_name("Essence of Flames", false), Q::Normal);
        assert_eq!(
            quality_from_name("Greater Essence of Flames", false),
            Q::Greater
        );
        assert_eq!(
            quality_from_name("Perfect Essence of Flames", false),
            Q::Perfect
        );
        assert_eq!(quality_from_name("Essence of Flames", true), Q::Corrupted);
    }

    #[test]
    fn essence_label_scope_covers_all_live_coe_base_labels() {
        // Full set of bases[].name_base labels that flow through essence
        // tier groups in the live CoE snapshot (poec_clean.json) now that
        // the bases-before-bgroups join is in place. Every label must map
        // to at least one engine class.
        let live_labels = [
            "Amulet",
            "Belt",
            "Body Armour (DEX)",
            "Body Armour (DEX/INT)",
            "Body Armour (INT)",
            "Body Armour (STR)",
            "Body Armour (STR/DEX)",
            "Body Armour (STR/INT)",
            "Boots (DEX)",
            "Boots (DEX/INT)",
            "Boots (INT)",
            "Boots (STR)",
            "Boots (STR/DEX)",
            "Boots (STR/INT)",
            "Bow",
            "Chaos Staff",
            "Chaos Wand",
            "Crossbow",
            "Dagger",
            "Fire Staff",
            "Fire Wand",
            "Flail",
            "Focus",
            "Gloves (DEX)",
            "Gloves (DEX/INT)",
            "Gloves (INT)",
            "Gloves (STR)",
            "Gloves (STR/DEX)",
            "Gloves (STR/INT)",
            "Helmet (DEX)",
            "Helmet (DEX/INT)",
            "Helmet (INT)",
            "Helmet (STR)",
            "Helmet (STR/DEX)",
            "Helmet (STR/INT)",
            "Ice Staff",
            "Ice Wand",
            "Lightning Staff",
            "Lightning Wand",
            "One Hand Axe",
            "One Hand Mace",
            "One Hand Sword",
            "Physical Staff",
            "Physical Wand",
            "Quiver",
            "Ring",
            "Sceptre",
            "Shield (DEX)",
            "Shield (STR)",
            "Shield (STR/DEX)",
            "Shield (STR/INT)",
            "Spear",
            "Staff",
            "Talisman",
            "Two Hand Axe",
            "Two Hand Mace",
            "Two Hand Sword",
            "Wand",
            "Warstaff",
        ];
        for label in live_labels {
            let (classes, _) = essence_label_scope(label);
            assert!(
                !classes.is_empty(),
                "label {label:?} resolved to no classes"
            );
        }
    }

    #[test]
    fn essence_label_scope_attribute_and_niche_bases() {
        use poc2_engine::AttributePool as P;
        // Attribute-split bases carry the pool refinement.
        assert_eq!(
            essence_label_scope("Shield (STR)"),
            (vec!["Shield"], Some(P::Str))
        );
        assert_eq!(
            essence_label_scope("Body Armour (DEX/INT)"),
            (vec!["BodyArmour"], Some(P::DexInt))
        );
        // Element-flavoured weapon rows collapse onto their class.
        assert_eq!(essence_label_scope("Chaos Wand"), (vec!["Wand"], None));
        assert_eq!(essence_label_scope("Ice Staff"), (vec!["Staff"], None));
        // Remaining bases-table rows (not in essence tiers today, but the
        // join can surface them): jewels, charms, tablets, flasks, waystone
        // tiers, Grasping Mail.
        assert_eq!(essence_label_scope("Ruby"), (vec!["Jewel"], None));
        assert_eq!(
            essence_label_scope("Time-Lost Emerald"),
            (vec!["Jewel"], None)
        );
        assert_eq!(essence_label_scope("Charm"), (vec!["UtilityFlask"], None));
        assert_eq!(
            essence_label_scope("Grasping Mail"),
            (vec!["BodyArmour"], None)
        );
        assert_eq!(essence_label_scope("Life Flask"), (vec!["LifeFlask"], None));
        assert_eq!(essence_label_scope("Mana Flask"), (vec!["ManaFlask"], None));
        assert_eq!(
            essence_label_scope("Precursor Tablet"),
            (vec!["TowerAugmentation"], None)
        );
        assert_eq!(
            essence_label_scope("Delirium Precursor Tablet"),
            (vec!["TowerAugmentation"], None)
        );
        assert_eq!(
            essence_label_scope("Low Tier (1-5)"),
            (vec!["Waystone"], None)
        );
        assert_eq!(essence_label_scope("Uber Tier"), (vec!["Waystone"], None));
        // bgroup-fallback aggregates resolve too.
        assert_eq!(essence_label_scope("Waystones"), (vec!["Waystone"], None));
        assert_eq!(
            essence_label_scope("Flasks"),
            (vec!["LifeFlask", "ManaFlask"], None)
        );
        assert!(essence_label_scope("Two-Handed Weapons")
            .0
            .contains(&"TwoHandSword"));
        // Genuinely unknown labels stay empty (logged + skipped by caller).
        assert_eq!(essence_label_scope("Mystery Box"), (vec![], None));
    }

    #[test]
    fn essence_catalogue_classifies_corrupted_by_name() {
        let mut b = Bundle::empty(PatchVersion::PATCH_0_4_0, "test@0000000");
        b.essences.entries.push(serde_json::json!({
            "id": "999",
            "name": "Essence of Insanity",
            "corrupt": false,
            "tooltip": [],
            "tier_groups": [{
                "base_group_id": "3",
                "label": "Belt",
                "tiers": [{"mod_id": "6", "engine_mod_id": "EssenceInsanityBelt1", "ilvl": "1"}],
            }],
        }));
        let catalogue = b.essence_catalogue();
        assert_eq!(catalogue.len(), 1);
        let e = &catalogue[0];
        assert_eq!(e.quality, poc2_engine::EssenceQuality::Corrupted);
        assert_eq!(e.id.as_str(), "CorruptedEssenceOfInsanity");
        assert_eq!(e.class_targets.len(), 1);
        assert_eq!(e.class_targets[0].class.as_str(), "Belt");
        assert_eq!(e.class_targets[0].mod_id.as_str(), "EssenceInsanityBelt1");
    }
}
