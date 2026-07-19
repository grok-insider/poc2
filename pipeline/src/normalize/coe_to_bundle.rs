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
        // Essence tier maps key on the BASES table (`bases`.id_base), whose
        // low ids overlap `bgroups`.id_bgroup (e.g. base 3 = Belt vs bgroup
        // 3 = Boots; base 12 = Dagger vs bgroup 12 = Tablets) — concrete
        // bases must resolve FIRST, base groups only as a fallback for ids
        // absent from the bases table. We emit both id and a friendly label.
        let mut tier_groups: Vec<serde_json::Value> = Vec::new();
        for (base_id, tier_list) in &tiers {
            let label = if let Some(b) = base_by_id.get(base_id.as_str()) {
                b.name_base.as_str()
            } else if let Some(bg) = bgroup_by_id.get(base_id.as_str()) {
                bg.name_bgroup.as_str()
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
            // Resolve the weight's scope target. A concrete base can carry
            // ilvl-stratified weights (`BaseAtIlvl`); a CoE base-GROUP maps to
            // an item class, which has no ilvl axis (single weight).
            let (base, item_class) = if let Some(b) = base_by_id.get(base_id.as_str()) {
                (Some(BaseTypeId::from(b.name_base.as_str())), None)
            } else if let Some(bg) = bgroup_by_id.get(base_id.as_str()) {
                (None, Some(item_class_id_from_bgroup_name(&bg.name_bgroup)))
            } else if let Some(name) = bitem_by_id.get(base_id.as_str()) {
                (Some(BaseTypeId::from(*name)), None)
            } else {
                continue;
            };

            // Positive-weight (ilvl, weight) breakpoints from the tier ladder.
            let mut points: Vec<(u32, f64)> = tier_list
                .iter()
                .filter_map(|t| {
                    let w: u32 = t.weighting.parse().ok()?;
                    let ilvl: u32 = t.ilvl.parse().ok()?;
                    (w > 0).then_some((ilvl, f64::from(w)))
                })
                .collect();
            if points.is_empty() {
                continue;
            }

            let mod_id = ModId::from(engine_mod.id.as_str());
            let note = || Some(format!("from CoE mod {coe_mod_id} via {strategy:?}"));

            if let Some(base) = base {
                // Collapse the tier ladder to genuine breakpoints: sort by ilvl
                // ascending, then drop runs of equal weight.
                points.sort_by_key(|(ilvl, _)| *ilvl);
                let mut ladder: Vec<(u32, f64)> = Vec::new();
                for (ilvl, w) in points {
                    if ladder.last().map(|&(_, lw)| lw) != Some(w) {
                        ladder.push((ilvl, w));
                    }
                }
                if ladder.len() >= 2 {
                    // §5.5: emit one `BaseAtIlvl` observation per breakpoint.
                    // `numeric_weight` picks the highest `min_ilvl <= item.ilvl`,
                    // so high-ilvl items resolve to the top-tier weight (matching
                    // the previous max-tier behaviour) while lower item levels
                    // now resolve to the correct lower-tier weight.
                    for (min_ilvl, w) in ladder {
                        weights.push(WeightObservation {
                            mod_id: mod_id.clone(),
                            scope: WeightScope::BaseAtIlvl {
                                base: base.clone(),
                                min_ilvl,
                            },
                            primary_weight: w,
                            secondary_weight: None,
                            confidence: Confidence::Community,
                            note: note(),
                        });
                    }
                } else {
                    // One effective weight across all tiers → flat `Base` scope.
                    weights.push(WeightObservation {
                        mod_id,
                        scope: WeightScope::Base { base },
                        primary_weight: ladder[0].1,
                        secondary_weight: None,
                        confidence: Confidence::Community,
                        note: note(),
                    });
                }
            } else if let Some(item_class) = item_class {
                // Class scope: keep the highest-ilvl tier weight (unchanged).
                let (_, weight) = points
                    .iter()
                    .copied()
                    .max_by_key(|(ilvl, _)| *ilvl)
                    .expect("points is non-empty");
                weights.push(WeightObservation {
                    mod_id,
                    scope: WeightScope::ItemClass { item_class },
                    primary_weight: weight,
                    secondary_weight: None,
                    confidence: Confidence::Community,
                    note: note(),
                });
            }
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

// ---------------------------------------------------------------------------
// M14.7e: alias suggester
// ---------------------------------------------------------------------------

/// Where in the engine mod the suggestion's similarity score came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuggestionSource {
    /// Score derived from `ModDefinition::name`.
    Name,
    /// Score derived from `ModDefinition::text_template` (placeholders stripped).
    Template,
}

impl SuggestionSource {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::Template => "template",
        }
    }
}

/// Single ranked candidate for an unmatched CoE mod.
#[derive(Debug, Clone)]
pub struct AliasCandidate {
    pub engine_mod_id: String,
    /// Token-Jaccard similarity in `[0.0, 1.0]`. Higher is better.
    pub score: f64,
    pub source: SuggestionSource,
}

/// Top-K alias suggestions for a single unmatched CoE mod display name.
#[derive(Debug, Clone)]
pub struct AliasSuggestion {
    pub coe_name: String,
    /// Sorted high → low score. Empty when no engine mod yielded a
    /// non-zero overlap.
    pub candidates: Vec<AliasCandidate>,
}

/// Tokenise a string into lowercased ASCII alphabetic words of length
/// ≥ 3, dropping placeholder characters and digits. Used as the basic
/// unit for the Jaccard similarity score below.
fn tokens_for_similarity(s: &str) -> Vec<String> {
    s.to_lowercase()
        .split(|c: char| !c.is_ascii_alphabetic())
        .filter(|w| w.len() >= 3)
        .map(str::to_string)
        .collect()
}

/// Token-Jaccard score: `|A ∩ B| / |A ∪ B|`. Returns `0.0` when
/// either side is empty.
fn token_jaccard(left: &[String], right: &[String]) -> f64 {
    if left.is_empty() || right.is_empty() {
        return 0.0;
    }
    let set_l: std::collections::HashSet<&String> = left.iter().collect();
    let set_r: std::collections::HashSet<&String> = right.iter().collect();
    let inter = set_l.intersection(&set_r).count();
    let union = set_l.union(&set_r).count();
    if union == 0 {
        0.0
    } else {
        // usize→f64 precision loss is irrelevant for token-set
        // cardinalities (always small, well under 2^52).
        #[allow(clippy::cast_precision_loss)]
        let score = inter as f64 / union as f64;
        score
    }
}

/// Score one engine mod against the lowercased CoE display name and
/// return the best `(score, source)` pair across both `name` and
/// `text_template`. A score of `0.0` means no overlap; the suggester
/// drops zero-score candidates.
fn best_score_for_mod(coe_tokens: &[String], em: &ModDefinition) -> (f64, SuggestionSource) {
    let mut best = (0.0_f64, SuggestionSource::Name);
    if let Some(name) = em.name.as_deref() {
        let s = token_jaccard(coe_tokens, &tokens_for_similarity(name));
        if s > best.0 {
            best = (s, SuggestionSource::Name);
        }
    }
    if let Some(template) = em.text_template.as_deref() {
        // Strip `{0}`-style placeholders before tokenising.
        let cleaned: String = template
            .chars()
            .filter(|c| !matches!(*c, '{' | '}'))
            .collect();
        let s = token_jaccard(coe_tokens, &tokens_for_similarity(&cleaned));
        if s > best.0 {
            best = (s, SuggestionSource::Template);
        }
    }
    best
}

/// For every CoE mod that the four-tier resolver could not match,
/// score every engine mod by token-Jaccard similarity and return the
/// top `top_k` candidates per name. `top_k` of `0` is treated as `1`.
///
/// Suggestions are sorted high → low score, ties broken by engine mod
/// id ascending so the output is deterministic.
#[must_use]
pub fn suggest_aliases_for_unmatched(
    snapshot: &CoeSnapshot,
    bundle: &Bundle,
    top_k: usize,
) -> Vec<AliasSuggestion> {
    let alias_table = load_coe_aliases();
    let alias_idx = alias_table.build_index();
    let essence_xref = build_essence_xref(snapshot);
    let top_k = top_k.max(1);

    let mut out = Vec::new();
    let mut seen_names: std::collections::HashSet<String> = std::collections::HashSet::new();
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
        if resolve_engine_mod(coe_mod, &bundle.mods, &alias_idx, &essence_xref).is_some() {
            continue;
        }
        // De-duplicate by display name so each unique unmatched name
        // is suggested for only once even when CoE has multiple ids
        // sharing it.
        if !seen_names.insert(coe_mod.name_modifier.to_lowercase()) {
            continue;
        }

        let coe_tokens = tokens_for_similarity(&coe_mod.name_modifier);
        if coe_tokens.is_empty() {
            continue;
        }
        let mut scored: Vec<AliasCandidate> = bundle
            .mods
            .iter()
            .filter_map(|em| {
                let (score, source) = best_score_for_mod(&coe_tokens, em);
                if score <= 0.0 {
                    return None;
                }
                Some(AliasCandidate {
                    engine_mod_id: em.id.as_str().to_string(),
                    score,
                    source,
                })
            })
            .collect();
        scored.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.engine_mod_id.cmp(&b.engine_mod_id))
        });
        scored.truncate(top_k);

        out.push(AliasSuggestion {
            coe_name: coe_mod.name_modifier.clone(),
            candidates: scored,
        });
    }
    // Stable order: sort by coe_name to make the rendered output
    // diff-friendly.
    out.sort_by(|a, b| a.coe_name.cmp(&b.coe_name));
    out
}

/// Render `suggestions` as a TOML fragment that an operator can paste
/// into `pipeline/data/coe_aliases.toml`. For each unmatched name we
/// emit a commented header listing the top candidates, then a single
/// `[[alias]]` block pre-filled with the highest-scoring candidate. A
/// reviewer is expected to delete obviously-wrong blocks before
/// committing.
#[must_use]
pub fn render_alias_suggestions_toml(suggestions: &[AliasSuggestion]) -> String {
    use std::fmt::Write as _;

    let mut out = String::new();
    out.push_str("# === auto-suggested CoE→engine aliases ====================================\n");
    out.push_str("# Generated by `cargo run -p poc2-pipeline -- coe-aliases-suggest`.\n");
    out.push_str("# Review each block, delete wrong ones, then merge into coe_aliases.toml.\n");
    out.push_str(
        "# ===========================================================================\n\n",
    );

    for suggestion in suggestions {
        let _ = writeln!(out, "# UNMATCHED: {}", suggestion.coe_name);
        if suggestion.candidates.is_empty() {
            out.push_str("# (no engine mod scored above zero — manual lookup needed)\n\n");
            continue;
        }
        out.push_str("# top suggestions:\n");
        for (idx, cand) in suggestion.candidates.iter().enumerate() {
            let _ = writeln!(
                out,
                "#   {}. {} (score={:.2}, via={})",
                idx + 1,
                cand.engine_mod_id,
                cand.score,
                cand.source.as_str()
            );
        }
        let top = &suggestion.candidates[0];
        out.push_str("[[alias]]\n");
        let _ = writeln!(
            out,
            "coe_name = \"{}\"",
            escape_toml_string(&suggestion.coe_name)
        );
        let _ = writeln!(
            out,
            "engine_mod_id = \"{}\"",
            escape_toml_string(&top.engine_mod_id)
        );
        let _ = writeln!(
            out,
            "note = \"auto-suggested (score={:.2}, via {}); review before commit\"\n",
            top.score,
            top.source.as_str()
        );
    }
    out
}

/// Minimal TOML basic-string escape: backslash, quote, and ASCII
/// control characters. Sufficient for CoE display strings (no embedded
/// quotes expected, but defensive escaping keeps the output valid).
fn escape_toml_string(input: &str) -> String {
    use std::fmt::Write as _;

    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if (ch as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04X}", ch as u32);
            }
            ch => out.push(ch),
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
            tier: None,
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

    // ----------------------------------------------------------------
    // M14.7e — alias suggester
    // ----------------------------------------------------------------

    #[test]
    fn token_jaccard_basic_overlap() {
        let life_a = vec!["maximum".to_string(), "life".to_string()];
        let life_b = vec!["maximum".to_string(), "life".to_string()];
        assert!((token_jaccard(&life_a, &life_b) - 1.0).abs() < 1e-9);
        let mana = vec!["maximum".to_string(), "mana".to_string()];
        // |{maximum}| / |{maximum, life, mana}| = 1/3
        let partial = token_jaccard(&life_a, &mana);
        assert!((partial - 1.0 / 3.0).abs() < 1e-9, "got {partial}");
        let empty: Vec<String> = vec![];
        assert!(token_jaccard(&life_a, &empty).abs() < 1e-9);
    }

    #[test]
    fn tokens_for_similarity_drops_short_and_digits() {
        let toks = tokens_for_similarity("+# to maximum Life {0}");
        assert_eq!(toks, vec!["maximum".to_string(), "life".to_string()]);
    }

    #[test]
    fn best_score_for_mod_prefers_higher_overlap_template_over_name() {
        let coe_tokens = tokens_for_similarity("+# to maximum life");
        // Name has zero overlap, template fully covers the CoE tokens.
        let em = mk_engine_mod("MaxLife1", Some("Vitality"), Some("+{0} to Maximum Life"));
        let (score, source) = best_score_for_mod(&coe_tokens, &em);
        assert!(score > 0.5, "expected high overlap, got {score}");
        assert_eq!(source, SuggestionSource::Template);
    }

    fn mk_snapshot_with_one_unmatched() -> CoeSnapshot {
        // Snapshot with a single CoE mod that the resolver can't match
        // against the engine mods we'll supply in the bundle below.
        // CoeData has no Default derive (it's deserialise-driven), so
        // construct each section explicitly with empty `seq`s.
        use crate::sources::coe::{CoeData, Section};
        let coe_data = CoeData {
            bitems: Section { seq: vec![] },
            bases: Section { seq: vec![] },
            bgroups: Section { seq: vec![] },
            modifiers: Section {
                seq: vec![mk_coe_mod("9001", "+# to maximum Life")],
            },
            mgroups: Section { seq: vec![] },
            mtypes: Section { seq: vec![] },
            catalysts: Section { seq: vec![] },
            essences: Section { seq: vec![] },
            basemods: std::collections::BTreeMap::new(),
            modbases: std::collections::BTreeMap::new(),
            tiers: {
                let mut m = std::collections::BTreeMap::new();
                m.insert("9001".to_string(), std::collections::BTreeMap::new());
                m
            },
        };
        CoeSnapshot {
            data: coe_data,
            revisions: poc2_data::SourceRevisions::default(),
        }
    }

    fn mk_tier(ilvl: &str, weighting: &str) -> crate::sources::coe::CoeTierEntry {
        crate::sources::coe::CoeTierEntry {
            ilvl: ilvl.into(),
            weighting: weighting.into(),
            nvalues: None,
            tord: 0,
            alias: None,
        }
    }

    /// Build a snapshot with one CoE mod resolvable to engine mod `MaxLife`
    /// (via name-substring) carrying the given tier ladder on base "Vaal Regalia".
    fn snapshot_with_tiers(tiers: Vec<crate::sources::coe::CoeTierEntry>) -> CoeSnapshot {
        use crate::sources::coe::{CoeBase, CoeData, Section};
        let mut by_base = std::collections::BTreeMap::new();
        by_base.insert("b1".to_string(), tiers);
        let mut tiers_by_mod = std::collections::BTreeMap::new();
        tiers_by_mod.insert("9001".to_string(), by_base);
        let coe_data = CoeData {
            bitems: Section { seq: vec![] },
            bases: Section {
                seq: vec![CoeBase {
                    id_bgroup: "bg1".into(),
                    id_base: "b1".into(),
                    name_base: "Vaal Regalia".into(),
                    is_jewellery: None,
                    base_type: None,
                    is_legacy: None,
                    is_martial: None,
                }],
            },
            bgroups: Section { seq: vec![] },
            modifiers: Section {
                seq: vec![mk_coe_mod("9001", "+# to maximum Life")],
            },
            mgroups: Section { seq: vec![] },
            mtypes: Section { seq: vec![] },
            catalysts: Section { seq: vec![] },
            essences: Section { seq: vec![] },
            basemods: std::collections::BTreeMap::new(),
            modbases: std::collections::BTreeMap::new(),
            tiers: tiers_by_mod,
        };
        CoeSnapshot {
            data: coe_data,
            revisions: poc2_data::SourceRevisions::default(),
        }
    }

    #[test]
    fn populate_weights_emits_base_at_ilvl_for_multi_tier_base() {
        // §5.5: a base whose tier ladder has distinct weights at distinct ilvls
        // yields one `BaseAtIlvl` observation per breakpoint, so the engine can
        // resolve the right weight per item level.
        use poc2_data::weights::WeightScope;
        let mut bundle = empty_bundle_for_test();
        bundle
            .mods
            .push(mk_engine_mod("MaxLife", Some("maximum Life"), None));
        let snapshot = snapshot_with_tiers(vec![
            mk_tier("1", "100"),
            mk_tier("50", "250"),
            mk_tier("80", "1000"),
        ]);

        let report = populate_weights(&snapshot, &mut bundle, &AHashMap::new(), &AHashMap::new());
        assert_eq!(
            report.total_matched(),
            1,
            "CoE mod must resolve via name-substring"
        );

        let mut got: Vec<(u32, f64)> = bundle
            .weights
            .iter()
            .filter(|o| o.mod_id.as_str() == "MaxLife")
            .filter_map(|o| match &o.scope {
                WeightScope::BaseAtIlvl { base, min_ilvl } if base.as_str() == "Vaal Regalia" => {
                    Some((*min_ilvl, o.primary_weight))
                }
                _ => None,
            })
            .collect();
        got.sort_by_key(|(i, _)| *i);
        assert_eq!(
            got,
            vec![(1, 100.0), (50, 250.0), (80, 1000.0)],
            "expected one BaseAtIlvl breakpoint per CoE tier"
        );
        // No flat Base observation should be emitted for this base when the
        // BaseAtIlvl ladder is present.
        assert!(
            !bundle
                .weights
                .iter()
                .any(|o| matches!(&o.scope, WeightScope::Base { base } if base.as_str() == "Vaal Regalia")),
            "ilvl-stratified bases must not also emit a flat Base observation"
        );
    }

    #[test]
    fn populate_weights_collapses_single_weight_ladder_to_base_scope() {
        // When every tier shares one weight there is no real breakpoint, so a
        // single flat `Base` observation is emitted (no spurious ladder).
        use poc2_data::weights::WeightScope;
        let mut bundle = empty_bundle_for_test();
        bundle
            .mods
            .push(mk_engine_mod("MaxLife", Some("maximum Life"), None));
        let snapshot = snapshot_with_tiers(vec![
            mk_tier("1", "500"),
            mk_tier("50", "500"),
            mk_tier("80", "500"),
        ]);

        populate_weights(&snapshot, &mut bundle, &AHashMap::new(), &AHashMap::new());
        let base_scoped: Vec<f64> = bundle
            .weights
            .iter()
            .filter(|o| o.mod_id.as_str() == "MaxLife")
            .filter_map(|o| match &o.scope {
                WeightScope::Base { base } if base.as_str() == "Vaal Regalia" => {
                    Some(o.primary_weight)
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            base_scoped,
            vec![500.0],
            "uniform tiers → one flat Base weight"
        );
        assert!(
            !bundle
                .weights
                .iter()
                .any(|o| matches!(&o.scope, WeightScope::BaseAtIlvl { .. })),
            "uniform-weight ladder must not emit BaseAtIlvl breakpoints"
        );
    }

    fn empty_bundle_for_test() -> Bundle {
        Bundle::empty(poc2_engine::patch::PatchVersion::PATCH_0_4_0, "test")
    }

    /// Snapshot exercising the essence tier-label join: id "3" is BOTH a
    /// concrete base (Belt) and a base group (Boots) — mirroring the live
    /// CoE id overlap — plus one id only in bgroups and one in neither.
    fn snapshot_with_overlapping_essence_ids() -> CoeSnapshot {
        use crate::sources::coe::{CoeData, CoeEssence, Section};
        let coe_data = CoeData {
            bitems: Section { seq: vec![] },
            bases: Section {
                seq: vec![CoeBase {
                    id_bgroup: "1".into(),
                    id_base: "3".into(),
                    name_base: "Belt".into(),
                    is_jewellery: None,
                    base_type: None,
                    is_legacy: None,
                    is_martial: None,
                }],
            },
            bgroups: Section {
                seq: vec![
                    CoeBaseGroup {
                        id_bgroup: "3".into(),
                        name_bgroup: "Boots".into(),
                        max_affix: "6".into(),
                        is_rare: "1".into(),
                        is_craftable: "1".into(),
                        max_sockets: "0".into(),
                    },
                    CoeBaseGroup {
                        id_bgroup: "77".into(),
                        name_bgroup: "Offhands".into(),
                        max_affix: "6".into(),
                        is_rare: "1".into(),
                        is_craftable: "1".into(),
                        max_sockets: "0".into(),
                    },
                ],
            },
            modifiers: Section { seq: vec![] },
            mgroups: Section { seq: vec![] },
            mtypes: Section { seq: vec![] },
            catalysts: Section { seq: vec![] },
            essences: Section {
                seq: vec![CoeEssence {
                    id_essence: "1".into(),
                    name_essence: "Essence of Insanity".into(),
                    tooltip: "[]".into(),
                    tiers: r#"{"3":[[{"mod":"6","id":"EssenceInsanityBelt1","ilvl":"1"}]],"77":[[{"mod":"7","id":"EssenceInsanityOffhand1","ilvl":"1"}]],"999":[[{"mod":"8","id":"EssenceInsanityUnknown1","ilvl":"1"}]]}"#.into(),
                    corrupt: "0".into(),
                }],
            },
            basemods: std::collections::BTreeMap::new(),
            modbases: std::collections::BTreeMap::new(),
            tiers: std::collections::BTreeMap::new(),
        };
        CoeSnapshot {
            data: coe_data,
            revisions: poc2_data::SourceRevisions::default(),
        }
    }

    #[test]
    fn essence_tier_labels_resolve_bases_before_bgroups() {
        // Root cause of the 0.5 essence-scope bug: CoE essence tiers key on
        // the bases table, whose ids overlap bgroup ids. Bases must win;
        // bgroups remain a fallback for ids absent from bases.
        let snapshot = snapshot_with_overlapping_essence_ids();
        let mut bundle = empty_bundle_for_test();
        normalize_coe(&snapshot, &mut bundle).unwrap();
        assert_eq!(bundle.essences.entries.len(), 1);
        let groups = bundle.essences.entries[0]["tier_groups"]
            .as_array()
            .unwrap();
        let label_for = |id: &str| {
            groups
                .iter()
                .find(|g| g["base_group_id"] == id)
                .map_or_else(
                    || panic!("no tier group for id {id}"),
                    |g| g["label"].as_str().unwrap().to_string(),
                )
        };
        // id 3 = base Belt AND bgroup Boots → the concrete base wins.
        assert_eq!(label_for("3"), "Belt");
        // id 77 exists only in bgroups → group fallback still resolves.
        assert_eq!(label_for("77"), "Offhands");
        // id 999 exists in neither table → raw id passes through.
        assert_eq!(label_for("999"), "999");
    }

    #[test]
    fn suggest_picks_top_candidate_by_token_overlap() {
        let snapshot = mk_snapshot_with_one_unmatched();
        let mut bundle = empty_bundle_for_test();
        // Both engine mods are deliberately constructed so that the
        // four-tier resolver fails:
        //   - name has no substring overlap with "+# to maximum life"
        //   - template includes at least one extra word that is NOT in
        //     the needle, so `templates_compatible` (which requires
        //     subset) returns false
        // …but token-Jaccard still picks the more relevant candidate.
        bundle.mods.push(mk_engine_mod(
            "VitalityRoll",
            Some("VitalityRoll"),
            // tokens: {bonus, maximum, life, increase}; needle tokens
            // {maximum, life} ⇒ intersection = 2, union = 4 ⇒ J=0.5
            Some("+{0} bonus Maximum Life increase"),
        ));
        bundle.mods.push(mk_engine_mod(
            "ManaRoll",
            Some("ManaRoll"),
            // tokens: {bonus, maximum, mana, increase}; needle tokens
            // {maximum, life} ⇒ intersection = 1, union = 5 ⇒ J=0.2
            Some("+{0} bonus Maximum Mana increase"),
        ));

        let suggestions = suggest_aliases_for_unmatched(&snapshot, &bundle, 2);
        assert_eq!(suggestions.len(), 1, "expected one unmatched suggestion");
        let s = &suggestions[0];
        assert_eq!(s.coe_name, "+# to maximum Life");
        assert!(!s.candidates.is_empty(), "expected at least one candidate");
        assert_eq!(
            s.candidates[0].engine_mod_id, "VitalityRoll",
            "highest token-overlap should win"
        );
        assert!(
            s.candidates[0].score > s.candidates[1].score,
            "first candidate should outscore second"
        );
    }

    #[test]
    fn suggest_skips_already_matched_via_resolver() {
        let snapshot = mk_snapshot_with_one_unmatched();
        let mut bundle = empty_bundle_for_test();
        // This engine mod's name substring-matches the CoE name, so the
        // four-tier resolver succeeds and the suggester should skip it.
        bundle
            .mods
            .push(mk_engine_mod("MaxLife", Some("+# to maximum Life"), None));
        let suggestions = suggest_aliases_for_unmatched(&snapshot, &bundle, 3);
        assert!(
            suggestions.is_empty(),
            "resolver-matched names should be skipped by suggester"
        );
    }

    #[test]
    fn suggest_de_duplicates_by_lowercased_name() {
        let mut snapshot = mk_snapshot_with_one_unmatched();
        // Add a second CoE mod with the same display name (different case)
        // — only one suggestion should come out.
        snapshot
            .data
            .modifiers
            .seq
            .push(mk_coe_mod("9002", "+# TO MAXIMUM LIFE"));
        snapshot
            .data
            .tiers
            .insert("9002".to_string(), std::collections::BTreeMap::new());
        let mut bundle = empty_bundle_for_test();
        // Engine mod constructed so resolver misses (template has an
        // extra word) but suggester still scores it.
        bundle.mods.push(mk_engine_mod(
            "VitalityRoll",
            Some("VitalityRoll"),
            Some("+{0} bonus Maximum Life increase"),
        ));
        let suggestions = suggest_aliases_for_unmatched(&snapshot, &bundle, 1);
        assert_eq!(suggestions.len(), 1, "expected de-duplication by name");
    }

    #[test]
    fn render_emits_alias_block_with_top_candidate() {
        let suggestions = vec![AliasSuggestion {
            coe_name: "+# to maximum Life".into(),
            candidates: vec![
                AliasCandidate {
                    engine_mod_id: "MaximumLife4".into(),
                    score: 0.83,
                    source: SuggestionSource::Template,
                },
                AliasCandidate {
                    engine_mod_id: "VitalityLite".into(),
                    score: 0.40,
                    source: SuggestionSource::Name,
                },
            ],
        }];
        let toml_out = render_alias_suggestions_toml(&suggestions);
        assert!(toml_out.contains("[[alias]]"));
        assert!(toml_out.contains("coe_name = \"+# to maximum Life\""));
        assert!(toml_out.contains("engine_mod_id = \"MaximumLife4\""));
        assert!(toml_out.contains("via template"));
        assert!(toml_out.contains("UNMATCHED: +# to maximum Life"));
        assert!(
            toml_out.contains("VitalityLite"),
            "alternate candidate should appear in the comment block"
        );
    }

    #[test]
    fn render_handles_no_candidates_gracefully() {
        let suggestions = vec![AliasSuggestion {
            coe_name: "Some Wholly Unique CoE Mod".into(),
            candidates: vec![],
        }];
        let toml_out = render_alias_suggestions_toml(&suggestions);
        assert!(toml_out.contains("UNMATCHED: Some Wholly Unique CoE Mod"));
        assert!(toml_out.contains("manual lookup needed"));
        assert!(
            !toml_out.contains("[[alias]]"),
            "no [[alias]] block should be emitted when there are no candidates"
        );
    }

    #[test]
    fn escape_toml_string_handles_quotes_and_backslashes() {
        assert_eq!(escape_toml_string(r#"a"b\c"#), r#"a\"b\\c"#);
        assert_eq!(escape_toml_string("plain"), "plain");
    }
}
