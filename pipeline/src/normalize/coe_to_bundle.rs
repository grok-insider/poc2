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
//!    [`WeightObservation`]s. `mod_id` resolution joins on
//!    case-insensitive substring match between
//!    [`CoeModifier::name_modifier`] and our existing
//!    [`ModDefinition::name`] (when present) or `text_template`.
//!    Unmatched mods are logged and dropped.

use ahash::AHashMap;
use poc2_data::weights::{Confidence, WeightObservation, WeightScope};
use poc2_data::Bundle;
use poc2_engine::{BaseTypeId, ItemClassId, ModId};
use serde_json::json;
use tracing::{debug, info, warn};

use crate::error::PipelineResult;
use crate::sources::coe::{
    parse_essence_tiers, parse_essence_tooltip, split_pipes, CoeBase, CoeBaseGroup, CoeSnapshot,
};

/// Apply CoE data on top of an already-built bundle (the RePoE-fork bundle
/// is the base; CoE supplies essences, catalysts, and weights).
#[allow(clippy::unnecessary_wraps)] // forward-compat with future fallible joins
pub fn normalize_coe(snapshot: &CoeSnapshot, bundle: &mut Bundle) -> PipelineResult<()> {
    info!("normalizing CoE snapshot…");

    // Index lookups.
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
    let mut weights: Vec<WeightObservation> = Vec::new();
    let mut joined = 0_usize;
    let mut missing = 0_usize;
    for (coe_mod_id, by_base) in &snapshot.data.tiers {
        // Find the CoE modifier metadata by mod_id.
        let coe_mod = snapshot
            .data
            .modifiers
            .seq
            .iter()
            .find(|m| &m.id_modifier == coe_mod_id);
        let Some(coe_mod) = coe_mod else {
            missing += 1;
            continue;
        };
        let needle = coe_mod.name_modifier.to_lowercase();

        // Look up the engine mod by case-insensitive substring match
        // against either explicit name or text_template content.
        let engine_mod = bundle.mods.iter().find(|m| {
            if let Some(name) = &m.name {
                if name.to_lowercase().contains(&needle) || needle.contains(&name.to_lowercase()) {
                    return true;
                }
            }
            if let Some(template) = &m.text_template {
                if templates_compatible(template, &needle) {
                    return true;
                }
            }
            false
        });
        let Some(engine_mod) = engine_mod else {
            debug!(coe_mod = %coe_mod.name_modifier, "no engine mod for CoE weight");
            missing += 1;
            continue;
        };
        joined += 1;
        for (base_id, tier_list) in by_base {
            // Determine the scope: prefer Base if the CoE id maps to a
            // concrete base, else item-class via base group.
            let (scope, _label) = if let Some(b) = base_by_id.get(base_id.as_str()) {
                (
                    WeightScope::Base {
                        base: BaseTypeId::from(b.name_base.as_str()),
                    },
                    b.name_base.clone(),
                )
            } else if let Some(bg) = bgroup_by_id.get(base_id.as_str()) {
                let class = item_class_id_from_bgroup_name(&bg.name_bgroup);
                (
                    WeightScope::ItemClass { item_class: class },
                    bg.name_bgroup.clone(),
                )
            } else if let Some(name) = bitem_by_id.get(base_id.as_str()) {
                (
                    WeightScope::Base {
                        base: BaseTypeId::from(*name),
                    },
                    (*name).to_string(),
                )
            } else {
                continue;
            };

            // Take the highest-ilvl tier's weighting as the canonical
            // value. Tier entries with weighting=0 are excluded.
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
                note: Some(format!("from CoE mod {coe_mod_id}")),
            });
        }
    }
    bundle.weights.extend(weights);
    info!(
        joined,
        missing,
        total = bundle.weights.len(),
        "weights populated (CoE→engine join)"
    );
    if missing > joined {
        warn!(
            "more CoE mods unmatched than matched ({missing} missing vs {joined} joined). \
             Refine the name-substring join or add explicit aliases in M5.x."
        );
    }

    bundle.header.sources.0.extend(snapshot.revisions.0.clone());
    Ok(())
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
}
