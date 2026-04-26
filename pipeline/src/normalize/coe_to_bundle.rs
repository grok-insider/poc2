//! Lower a [`CoeSnapshot`] into a [`Bundle`] — populates `essences`,
//! `catalysts`, and `weights` sections.
//!
//! ## Strategy
//!
//! 1. **Essences**: every CoE essence becomes a typed JSON entry under
//!    `bundle.essences` (the section is currently `Vec<serde_json::Value>`
//!    until M2.6 promotes it to a typed schema). Each entry includes
//!    name, tooltip lines, tier-to-base-id mapping, and the corruption
//!    flag.
//! 2. **Catalysts**: each CoE catalyst entry becomes a typed JSON
//!    entry under `bundle.catalysts` with name + tag list.
//! 3. **Weights**: per (mod_id, base_id) tier entries are folded into
//!    [`WeightObservation`]s. `mod_id` resolution uses a four-tier
//!    join (per A.3 of the v1 execution plan) — see [`MapStrategy`]
//!    below. Unmatched mods are logged and dropped.

use ahash::AHashMap;
use poc2_data::weights::{Confidence, WeightObservation, WeightScope};
use poc2_data::Bundle;
use poc2_engine::{BaseTypeId, ItemClassId, ModDefinition, ModId};
use serde::Deserialize;
use serde_json::json;
use tracing::{debug, info, warn};

use crate::error::PipelineResult;
use crate::sources::coe::{
    parse_essence_tiers, parse_essence_tooltip, split_pipes, CoeBase, CoeBaseGroup, CoeModifier,
    CoeSnapshot,
};

// ---------------------------------------------------------------------------
// CoE → engine mod-id alias table (A.3)
// ---------------------------------------------------------------------------

/// Hand-curated CoE display name → engine mod-id alias.
#[derive(Debug, Clone, Deserialize)]
pub struct CoeAlias {
    pub coe_name: String,
    pub engine_mod_id: String,
    #[serde(default)]
    pub note: Option<String>,
}

/// Wrapper around the alias TOML file shape (`[[alias]]` array).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct CoeAliasTable {
    #[serde(default, rename = "alias")]
    pub aliases: Vec<CoeAlias>,
}

impl CoeAliasTable {
    /// Build a case-insensitive `coe_name -> engine_mod_id` lookup.
    #[must_use]
    pub fn build_index(&self) -> AHashMap<String, String> {
        self.aliases
            .iter()
            .map(|a| (a.coe_name.to_lowercase(), a.engine_mod_id.clone()))
            .collect()
    }
}

/// Embedded alias table from `pipeline/data/coe_aliases.toml`.
const COE_ALIASES_TOML: &str = include_str!("../../data/coe_aliases.toml");

/// Load the embedded alias table. Falls back to an empty table on
/// parse failure (with a warning) so a malformed alias file never
/// blocks bundle builds.
#[must_use]
pub fn load_coe_aliases() -> CoeAliasTable {
    match toml::from_str::<CoeAliasTable>(COE_ALIASES_TOML) {
        Ok(t) => t,
        Err(e) => {
            warn!(error = %e, "embedded coe_aliases.toml failed to parse — running with empty alias table");
            CoeAliasTable::default()
        }
    }
}

/// How a CoE→engine mod-id mapping was resolved. Surfaced via
/// [`JoinReport`] for the diagnose subcommand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapStrategy {
    /// Tier 1: explicit alias from `coe_aliases.toml`.
    Alias,
    /// Tier 2: harvested from CoE's essence cross-reference (essence
    /// tier entries carry both the CoE id_modifier and the engine ModId).
    EssenceXref,
    /// Tier 3: case-insensitive substring match between
    /// [`CoeModifier::name_modifier`] and the engine mod's name.
    NameSubstring,
    /// Tier 4: template-token fuzzy match against the engine mod's
    /// `text_template`.
    TemplateTokens,
}

/// Per-tier counts of how many CoE mods were resolved which way.
#[derive(Debug, Clone, Copy, Default)]
pub struct JoinReport {
    pub via_alias: usize,
    pub via_essence_xref: usize,
    pub via_name_substring: usize,
    pub via_template_tokens: usize,
    pub unmatched: usize,
}

impl JoinReport {
    #[must_use]
    pub fn total_matched(&self) -> usize {
        self.via_alias + self.via_essence_xref + self.via_name_substring + self.via_template_tokens
    }
    #[must_use]
    pub fn total_seen(&self) -> usize {
        self.total_matched() + self.unmatched
    }
    #[must_use]
    pub fn match_rate(&self) -> f64 {
        let total = self.total_seen();
        if total == 0 {
            0.0
        } else {
            #[allow(clippy::cast_precision_loss)] // counts < 2^52 in practice
            let r = self.total_matched() as f64 / total as f64;
            r
        }
    }
}

/// Apply CoE data on top of an already-built bundle (the RePoE-fork bundle
/// is the base; CoE supplies essences, catalysts, and weights).
#[allow(clippy::unnecessary_wraps)] // forward-compat with future fallible joins
pub fn normalize_coe(snapshot: &CoeSnapshot, bundle: &mut Bundle) -> PipelineResult<()> {
    info!("normalizing CoE snapshot…");

    // Index lookups (used by essences + catalysts; weights handles its
    // own indexing inside [`populate_weights`]).
    let bgroup_by_id: AHashMap<&str, &CoeBaseGroup> = snapshot
        .data
        .bgroups
        .seq
        .iter()
        .map(|b| (b.id_bgroup.as_str(), b))
        .collect();
    let base_by_id: AHashMap<&str, &CoeBase> = snapshot
        .data
        .bases
        .seq
        .iter()
        .map(|b| (b.id_base.as_str(), b))
        .collect();

    // ---- Essences -------------------------------------------------------
    let mut essence_entries: Vec<serde_json::Value> = Vec::new();
    for ess in &snapshot.data.essences.seq {
        let tooltip = parse_essence_tooltip(&ess.tooltip);
        let tiers = parse_essence_tiers(&ess.tiers);
        // Resolve base groups for the tier base ids: each base id might be
        // a base group (`bgroups`) or a concrete base (`bases`). We emit
        // both id and a friendly label.
        let mut tier_groups: Vec<serde_json::Value> = Vec::new();
        for (base_id, tier_list) in &tiers {
            let label = if let Some(bg) = bgroup_by_id.get(base_id.as_str()) {
                bg.name_bgroup.as_str()
            } else if let Some(b) = base_by_id.get(base_id.as_str()) {
                b.name_base.as_str()
            } else {
                base_id.as_str()
            };
            let mods: Vec<serde_json::Value> = tier_list
                .iter()
                .flat_map(|inner| inner.iter())
                .map(|m| json!({"mod_id": &m.r#mod, "engine_mod_id": &m.id, "ilvl": &m.ilvl}))
                .collect();
            tier_groups.push(json!({
                "base_group_id": base_id,
                "label": label,
                "tiers": mods,
            }));
        }
        essence_entries.push(json!({
            "id": ess.id_essence,
            "name": ess.name_essence,
            "corrupt": ess.corrupt == "1",
            "tooltip": tooltip,
            "tier_groups": tier_groups,
        }));
    }
    bundle.essences.section_version = 1;
    bundle.essences.entries = essence_entries;
    info!(count = bundle.essences.entries.len(), "essences populated");

    // ---- Catalysts ------------------------------------------------------
    let mut catalyst_entries: Vec<serde_json::Value> = Vec::new();
    for cat in &snapshot.data.catalysts.seq {
        let tags = split_pipes(&cat.tags);
        catalyst_entries.push(json!({
            "id": cat.id_catalyst,
            "name": cat.name_catalyst,
            "tags": tags,
        }));
    }
    bundle.catalysts.section_version = 1;
    bundle.catalysts.entries = catalyst_entries;
    info!(
        count = bundle.catalysts.entries.len(),
        "catalysts populated"
    );

    // ---- Weights --------------------------------------------------------
    let alias_table = load_coe_aliases();
    let alias_idx = alias_table.build_index();
    let essence_xref = build_essence_xref(snapshot);
    let report = populate_weights(snapshot, bundle, &alias_idx, &essence_xref);
    info!(
        via_alias = report.via_alias,
        via_essence_xref = report.via_essence_xref,
        via_name_substring = report.via_name_substring,
        via_template_tokens = report.via_template_tokens,
        unmatched = report.unmatched,
        match_rate = format!("{:.1}%", report.match_rate() * 100.0),
        total_weights = bundle.weights.len(),
        "weights populated (CoE→engine join, multi-tier)"
    );
    if report.unmatched > report.total_matched() {
        warn!(
            "more CoE mods unmatched ({}) than matched ({}). Run \
             `poc2-pipeline diagnose-coe <bundle>` and add aliases to \
             pipeline/data/coe_aliases.toml.",
            report.unmatched,
            report.total_matched()
        );
    }

    bundle.header.sources.0.extend(snapshot.revisions.0.clone());
    Ok(())
}

/// Build the CoE id_modifier → engine ModId index by harvesting essence
/// tier cross-references. Each essence's `tiers` payload references both
/// the CoE numeric mod id (`mod`) and the engine ModId string (`id`).
fn build_essence_xref(snapshot: &CoeSnapshot) -> AHashMap<String, String> {
    let mut xref: AHashMap<String, String> = AHashMap::new();
    for ess in &snapshot.data.essences.seq {
        let tiers = parse_essence_tiers(&ess.tiers);
        for tier_list in tiers.values() {
            for inner in tier_list {
                for entry in inner {
                    if entry.id.is_empty() || entry.r#mod.is_empty() {
                        continue;
                    }
                    xref.entry(entry.r#mod.clone())
                        .or_insert_with(|| entry.id.clone());
                }
            }
        }
    }
    xref
}

/// Resolve `coe_mod` against the engine mod set using a four-tier join
/// (explicit alias > essence cross-reference > name substring > template
/// tokens). Returns the matched engine mod plus the strategy that
/// produced the match.
fn resolve_engine_mod<'a>(
    coe_mod: &CoeModifier,
    bundle_mods: &'a [ModDefinition],
    alias_idx: &AHashMap<String, String>,
    essence_xref: &AHashMap<String, String>,
) -> Option<(&'a ModDefinition, MapStrategy)> {
    let needle = coe_mod.name_modifier.to_lowercase();

    // Tier 1: explicit alias
    if let Some(target) = alias_idx.get(&needle) {
        if let Some(em) = bundle_mods.iter().find(|m| m.id.as_str() == target) {
            return Some((em, MapStrategy::Alias));
        }
    }
    // Tier 2: essence cross-reference (CoE id_modifier → engine ModId)
    if let Some(target) = essence_xref.get(&coe_mod.id_modifier) {
        if let Some(em) = bundle_mods.iter().find(|m| m.id.as_str() == target) {
            return Some((em, MapStrategy::EssenceXref));
        }
    }
    // Tier 3: case-insensitive substring on engine name
    if let Some(em) = bundle_mods.iter().find(|m| {
        m.name
            .as_deref()
            .map(str::to_lowercase)
            .is_some_and(|n| n.contains(&needle) || needle.contains(&n))
    }) {
        return Some((em, MapStrategy::NameSubstring));
    }
    // Tier 4: template-token fuzzy match
    if let Some(em) = bundle_mods.iter().find(|m| {
        m.text_template
            .as_deref()
            .is_some_and(|t| templates_compatible(t, &needle))
    }) {
        return Some((em, MapStrategy::TemplateTokens));
    }
    None
}

/// Populate `bundle.weights` from the CoE snapshot, returning a
/// [`JoinReport`] summarising how each CoE mod was resolved.
pub fn populate_weights(
    snapshot: &CoeSnapshot,
    bundle: &mut Bundle,
    alias_idx: &AHashMap<String, String>,
    essence_xref: &AHashMap<String, String>,
) -> JoinReport {
    let bgroup_by_id: AHashMap<&str, &CoeBaseGroup> = snapshot
        .data
        .bgroups
        .seq
        .iter()
        .map(|b| (b.id_bgroup.as_str(), b))
        .collect();
    let base_by_id: AHashMap<&str, &CoeBase> = snapshot
        .data
        .bases
        .seq
        .iter()
        .map(|b| (b.id_base.as_str(), b))
        .collect();
    let bitem_by_id: AHashMap<&str, &str> = snapshot
        .data
        .bitems
        .seq
        .iter()
        .map(|b| (b.id_bitem.as_str(), b.name_bitem.as_str()))
        .collect();

    let bundle_mods = std::mem::take(&mut bundle.mods);
    let mut weights: Vec<WeightObservation> = Vec::new();
    let mut report = JoinReport::default();

    for (coe_mod_id, by_base) in &snapshot.data.tiers {
        let Some(coe_mod) = snapshot
            .data
            .modifiers
            .seq
            .iter()
            .find(|m| &m.id_modifier == coe_mod_id)
        else {
            report.unmatched += 1;
            continue;
        };
        let Some((engine_mod, strategy)) =
            resolve_engine_mod(coe_mod, &bundle_mods, alias_idx, essence_xref)
        else {
            debug!(coe_mod = %coe_mod.name_modifier, "no engine mod for CoE weight");
            report.unmatched += 1;
            continue;
        };
        match strategy {
            MapStrategy::Alias => report.via_alias += 1,
            MapStrategy::EssenceXref => report.via_essence_xref += 1,
            MapStrategy::NameSubstring => report.via_name_substring += 1,
            MapStrategy::TemplateTokens => report.via_template_tokens += 1,
        }
        for (base_id, tier_list) in by_base {
            let scope = if let Some(b) = base_by_id.get(base_id.as_str()) {
                WeightScope::Base {
                    base: BaseTypeId::from(b.name_base.as_str()),
                }
            } else if let Some(bg) = bgroup_by_id.get(base_id.as_str()) {
                WeightScope::ItemClass {
                    item_class: item_class_id_from_bgroup_name(&bg.name_bgroup),
                }
            } else if let Some(name) = bitem_by_id.get(base_id.as_str()) {
                WeightScope::Base {
                    base: BaseTypeId::from(*name),
                }
            } else {
                continue;
            };
            let max_tier = tier_list
                .iter()
                .filter_map(|t| {
                    let w: u32 = t.weighting.parse().ok()?;
                    let ilvl: u32 = t.ilvl.parse().ok()?;
                    Some((w, ilvl))
                })
                .filter(|(w, _)| *w > 0)
                .max_by_key(|(_, ilvl)| *ilvl);
            let Some((weight, _ilvl)) = max_tier else {
                continue;
            };
            weights.push(WeightObservation {
                mod_id: ModId::from(engine_mod.id.as_str()),
                scope,
                primary_weight: f64::from(weight),
                secondary_weight: None,
                confidence: Confidence::Community,
                note: Some(format!("from CoE mod {coe_mod_id} via {strategy:?}")),
            });
        }
    }
    bundle.mods = bundle_mods;
    bundle.weights.extend(weights);
    report
}

/// Diagnose helper: return the unmatched CoE modifier display names so
/// the diagnose subcommand can group them by frequency for users
/// authoring new aliases.
#[must_use]
pub fn unmatched_coe_mods(snapshot: &CoeSnapshot, bundle: &Bundle) -> Vec<String> {
    let alias_table = load_coe_aliases();
    let alias_idx = alias_table.build_index();
    let essence_xref = build_essence_xref(snapshot);
    let mut out = Vec::new();
    for coe_mod_id in snapshot.data.tiers.keys() {
        let Some(coe_mod) = snapshot
            .data
            .modifiers
            .seq
            .iter()
            .find(|m| &m.id_modifier == coe_mod_id)
        else {
            continue;
        };
        if resolve_engine_mod(coe_mod, &bundle.mods, &alias_idx, &essence_xref).is_none() {
            out.push(coe_mod.name_modifier.clone());
        }
    }
    out
}

/// Convert a CoE base-group name like `"Body Armours"` to our
/// internal `ItemClassId` (`"BodyArmour"`).
fn item_class_id_from_bgroup_name(name: &str) -> ItemClassId {
    let normalized: String = name
        .split_whitespace()
        .map(|w| {
            let stripped = w.strip_suffix('s').unwrap_or(w);
            let mut chars = stripped.chars();
            match chars.next() {
                Some(c) => c.to_ascii_uppercase().to_string() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect();
    ItemClassId::from(normalized.as_str())
}

/// Approximate match: every literal whitespace-separated token in the
/// template (skipping placeholders and digits) must appear in the needle.
fn templates_compatible(template: &str, needle_lower: &str) -> bool {
    let template_lower = template.to_lowercase();
    let stripped: String = template_lower
        .chars()
        .filter(|c| !matches!(*c, '{' | '}'))
        .collect();
    for word in stripped.split_whitespace() {
        let word: String = word.chars().filter(char::is_ascii_alphabetic).collect();
        if word.is_empty() || word.len() < 3 {
            continue;
        }
        if !needle_lower.contains(&word) {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use poc2_engine::item::AffixType;
    use poc2_engine::mods::{ModDomain, ModFlags, ModGroup, ModKind};
    use poc2_engine::patch::PatchRange;
    use poc2_engine::ModGroupId;
    use smallvec::smallvec;

    #[test]
    fn item_class_id_strips_trailing_s_per_word() {
        assert_eq!(
            item_class_id_from_bgroup_name("Body Armours").as_str(),
            "BodyArmour"
        );
        assert_eq!(item_class_id_from_bgroup_name("Helmets").as_str(), "Helmet");
        assert_eq!(
            item_class_id_from_bgroup_name("One Hand Maces").as_str(),
            "OneHandMace"
        );
    }

    #[test]
    fn templates_compatible_fuzzy_match() {
        assert!(templates_compatible(
            "+{0} to Maximum Energy Shield",
            "+# to maximum energy shield"
        ));
        assert!(templates_compatible(
            "+{0}% to Cold Resistance",
            "+# to cold resistance"
        ));
        assert!(!templates_compatible(
            "+{0} to Maximum Mana",
            "+# to maximum life"
        ));
    }

    // ----------------------------------------------------------------
    // A.3 — alias table + multi-tier join
    // ----------------------------------------------------------------

    #[test]
    fn embedded_aliases_parse() {
        let t = load_coe_aliases();
        assert!(!t.aliases.is_empty(), "expected non-empty embedded aliases");
        let idx = t.build_index();
        assert!(
            idx.contains_key("+#% to fire resistance"),
            "embedded aliases should cover Fire Resistance"
        );
    }

    fn mk_engine_mod(id: &str, name: Option<&str>, template: Option<&str>) -> ModDefinition {
        ModDefinition {
            id: ModId::from(id),
            name: name.map(str::to_string),
            mod_group: ModGroup(ModGroupId::from(id)),
            affix_type: AffixType::Suffix,
            kind: ModKind::Explicit,
            domain: ModDomain::Item,
            tags: smallvec![],
            concept_set: smallvec![],
            spawn_weights: smallvec![],
            stats: smallvec![],
            required_level: 1,
            allowed_item_classes: smallvec![],
            patch_range: PatchRange::ALL,
            flags: ModFlags::empty(),
            text_template: template.map(str::to_string),
        }
    }

    fn mk_coe_mod(id: &str, name: &str) -> CoeModifier {
        CoeModifier {
            id_modifier: id.into(),
            modgroups: None,
            affix: "suffix".into(),
            name_modifier: name.into(),
            mtypes: None,
            hybrid: "0".into(),
        }
    }

    #[test]
    fn resolve_via_alias_takes_precedence() {
        let engine = vec![
            mk_engine_mod("FireResistance", Some("FireResistance"), None),
            // Decoy with a name that also substring-matches; the alias
            // must win regardless.
            mk_engine_mod("FireResistanceLegacy", Some("Fire Res Legacy"), None),
        ];
        let mut alias = AHashMap::new();
        alias.insert(
            "+#% to fire resistance".into(),
            "FireResistance".to_string(),
        );
        let xref = AHashMap::new();
        let coe = mk_coe_mod("99", "+#% to Fire Resistance");
        let (em, strategy) = resolve_engine_mod(&coe, &engine, &alias, &xref).unwrap();
        assert_eq!(em.id.as_str(), "FireResistance");
        assert_eq!(strategy, MapStrategy::Alias);
    }

    #[test]
    fn resolve_via_essence_xref() {
        let engine = vec![mk_engine_mod(
            "LocalAddedPhysicalDamage5",
            Some("Local Added Physical Damage 5"),
            None,
        )];
        let alias = AHashMap::new();
        let mut xref = AHashMap::new();
        xref.insert("5118".into(), "LocalAddedPhysicalDamage5".into());
        let coe = mk_coe_mod("5118", "Adds Physical Damage to Attacks");
        let (em, strategy) = resolve_engine_mod(&coe, &engine, &alias, &xref).unwrap();
        assert_eq!(em.id.as_str(), "LocalAddedPhysicalDamage5");
        assert_eq!(strategy, MapStrategy::EssenceXref);
    }

    #[test]
    fn resolve_via_name_substring_when_no_alias_or_xref() {
        let engine = vec![mk_engine_mod(
            "ColdResistance3",
            Some("Cold Resistance"),
            None,
        )];
        let alias = AHashMap::new();
        let xref = AHashMap::new();
        let coe = mk_coe_mod("42", "to Cold Resistance");
        let (em, strategy) = resolve_engine_mod(&coe, &engine, &alias, &xref).unwrap();
        assert_eq!(em.id.as_str(), "ColdResistance3");
        assert_eq!(strategy, MapStrategy::NameSubstring);
    }

    #[test]
    fn resolve_via_template_tokens_last_resort() {
        let engine = vec![mk_engine_mod(
            "MaximumLife4",
            None,
            Some("+{0} to Maximum Life"),
        )];
        let alias = AHashMap::new();
        let xref = AHashMap::new();
        let coe = mk_coe_mod("13", "+# to maximum life");
        let (em, strategy) = resolve_engine_mod(&coe, &engine, &alias, &xref).unwrap();
        assert_eq!(em.id.as_str(), "MaximumLife4");
        assert_eq!(strategy, MapStrategy::TemplateTokens);
    }

    #[test]
    fn resolve_returns_none_when_no_match() {
        let engine = vec![mk_engine_mod(
            "Strength",
            Some("Strength"),
            Some("+{0} to Strength"),
        )];
        let alias = AHashMap::new();
        let xref = AHashMap::new();
        let coe = mk_coe_mod("7", "Some Brand-New Mod That Doesn't Exist");
        assert!(resolve_engine_mod(&coe, &engine, &alias, &xref).is_none());
    }

    #[test]
    fn join_report_match_rate() {
        let r = JoinReport {
            via_alias: 30,
            via_essence_xref: 40,
            via_name_substring: 15,
            via_template_tokens: 5,
            unmatched: 10,
        };
        assert_eq!(r.total_matched(), 90);
        assert_eq!(r.total_seen(), 100);
        assert!((r.match_rate() - 0.9).abs() < 1e-9);
    }
}
