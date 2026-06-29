//! Genesis Tree source (PoE2 0.5 "Return of the Ancients").
//!
//! Two embedded artifacts:
//!
//! - `pipeline/data/brequel_tree.json` — committed snapshot of RePoE-fork's
//!   `passive_skill_trees/BrequelTree.json` (the full "Brequel" Genesis Tree:
//!   248 passives, group positions, orbit layout, connection edges). Refresh
//!   from <https://repoe-fork.github.io/poe2/passive_skill_trees/BrequelTree.json>
//!   when a patch changes the tree.
//! - `pipeline/data/genesis_meta.toml` — curated stat-key templates, display-
//!   node description overrides, womb metadata, community goal presets and
//!   farming notes. Hand-maintained from poe2db / poe2wiki / Fextralife /
//!   creator videos; every soft number is a community estimate.
//!
//! Embedded (not network-fetched) so the bundle build stays deterministic
//! and offline-friendly; the live URL above is the manual refresh path.

use std::collections::BTreeMap;

use poc2_data::SourceRevision;
use serde::Deserialize;

const BREQUEL_TREE_JSON: &str = include_str!("../../data/brequel_tree.json");
const GENESIS_META_TOML: &str = include_str!("../../data/genesis_meta.toml");

// -------------------------------------------------------------------------
// Raw BrequelTree.json shapes (deserialize-only)
// -------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct RawTree {
    pub groups: Vec<RawGroup>,
    pub orbit_radii: Vec<f64>,
    pub passives: BTreeMap<String, RawPassive>,
    pub roots: Vec<u64>,
    pub skills_per_orbit: Vec<u32>,
    #[serde(default)]
    pub title: String,
}

#[derive(Debug, Deserialize)]
pub struct RawGroup {
    pub passives: Vec<RawPlacement>,
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Deserialize)]
pub struct RawPlacement {
    pub hash: u64,
    /// Orbit index into `orbit_radii` / `skills_per_orbit`.
    pub radius: usize,
    pub position_clockwise: u32,
    #[serde(default)]
    pub connections: Vec<u64>,
}

#[derive(Debug, Deserialize)]
pub struct RawPassive {
    pub hash: u64,
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub icon: String,
    #[serde(default)]
    pub is_notable: bool,
    #[serde(default)]
    pub stats: BTreeMap<String, f64>,
}

// -------------------------------------------------------------------------
// Curated genesis_meta.toml shapes
// -------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct GenesisMeta {
    pub schema_version: u32,
    pub wombs: BTreeMap<String, WombMeta>,
    #[serde(default)]
    pub stat_templates: BTreeMap<String, String>,
    #[serde(default)]
    pub node_overrides: Vec<NodeOverride>,
    #[serde(default)]
    pub presets: Vec<PresetMeta>,
    #[serde(default)]
    pub farming: FarmingMeta,
    #[serde(default)]
    pub videos: Vec<VideoMeta>,
}

#[derive(Debug, Deserialize)]
pub struct WombMeta {
    pub display_name: String,
    pub wombgift: String,
    pub gift_art: String,
    pub points: u32,
    pub icon_normal: String,
    pub icon_notable: String,
    pub blurb: String,
}

#[derive(Debug, Deserialize)]
pub struct NodeOverride {
    pub id: String,
    pub description: String,
}

#[derive(Debug, Deserialize)]
pub struct PresetMeta {
    pub id: String,
    pub name: String,
    pub womb: String,
    pub confidence: String,
    pub summary: String,
    #[serde(default)]
    pub sources: Vec<String>,
    #[serde(default)]
    pub steps: Vec<PresetStep>,
    #[serde(default)]
    pub avoid: Vec<PresetAvoid>,
    #[serde(default)]
    pub gift_advice: String,
}

#[derive(Debug, Deserialize)]
pub struct PresetStep {
    pub node: String,
    pub why: String,
    pub priority: u32,
    /// Filler step: claims every copy adjacent to the resolved core path
    /// (no connectors computed; spend leftover points here).
    #[serde(default)]
    pub fill: bool,
    /// Nice-to-have: resolved + highlighted, but not counted against the
    /// womb's guaranteed point budget (respec / swap-in choice).
    #[serde(default)]
    pub optional: bool,
}

#[derive(Debug, Deserialize)]
pub struct PresetAvoid {
    pub node: String,
    pub why: String,
}

#[derive(Debug, Default, Deserialize)]
pub struct FarmingMeta {
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct VideoMeta {
    pub title: String,
    pub channel: String,
    pub url: String,
}

/// Parsed Genesis snapshot (tree + curated meta).
#[derive(Debug)]
pub struct GenesisSnapshot {
    pub tree: RawTree,
    pub meta: GenesisMeta,
    pub revision: SourceRevision,
}

impl GenesisSnapshot {
    pub fn count_summary(&self) -> String {
        format!(
            "genesis: {} passives, {} groups, {} wombs, {} presets, {} stat templates",
            self.tree.passives.len(),
            self.tree.groups.len(),
            self.meta.wombs.len(),
            self.meta.presets.len(),
            self.meta.stat_templates.len(),
        )
    }
}

/// Load the embedded Genesis Tree snapshot + curated metadata.
pub fn load() -> Result<GenesisSnapshot, String> {
    let tree: RawTree =
        serde_json::from_str(BREQUEL_TREE_JSON).map_err(|e| format!("brequel_tree.json: {e}"))?;
    let meta: GenesisMeta =
        toml::from_str(GENESIS_META_TOML).map_err(|e| format!("genesis_meta.toml: {e}"))?;
    Ok(GenesisSnapshot {
        tree,
        meta,
        revision: SourceRevision {
            name: "genesis.embedded".into(),
            revision: format!(
                "brequel_tree={} genesis_meta={}",
                BREQUEL_TREE_JSON.len(),
                GENESIS_META_TOML.len()
            ),
            url: Some(
                "https://repoe-fork.github.io/poe2/passive_skill_trees/BrequelTree.json".into(),
            ),
            fetched_at: String::new(),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn genesis_snapshot_parses() {
        let snap = load().expect("genesis fixtures parse");
        assert_eq!(snap.tree.passives.len(), 248, "expected 248 tree passives");
        assert_eq!(snap.tree.roots.len(), 5, "expected 5 womb roots");
        assert_eq!(snap.meta.wombs.len(), 5, "expected 5 womb metas");
        assert!(
            snap.meta.presets.len() >= 6,
            "expected at least 6 goal presets"
        );
    }

    #[test]
    fn every_display_stat_key_has_template_or_override() {
        let snap = load().expect("parse");
        let override_ids: std::collections::BTreeSet<&str> = snap
            .meta
            .node_overrides
            .iter()
            .map(|o| o.id.as_str())
            .collect();
        let mut missing = Vec::new();
        for p in snap.tree.passives.values() {
            if p.stats.is_empty() {
                continue;
            }
            let covered = override_ids.contains(p.id.as_str())
                || p.stats
                    .keys()
                    .all(|k| snap.meta.stat_templates.contains_key(k));
            if !covered {
                missing.push(format!("{} ({}): {:?}", p.id, p.name, p.stats.keys()));
            }
        }
        assert!(
            missing.is_empty(),
            "nodes lacking template/override coverage:\n{}",
            missing.join("\n")
        );
    }

    #[test]
    fn preset_nodes_resolve_to_real_tree_nodes() {
        let snap = load().expect("parse");
        // Build (womb, name) index. Branch derivation mirrors the normalizer.
        let mut names: std::collections::BTreeSet<(String, String)> =
            std::collections::BTreeSet::new();
        for p in snap.tree.passives.values() {
            if let Some(branch) = crate::normalize::genesis_to_bundle::branch_of(&p.id) {
                names.insert((branch.to_string(), p.name.clone()));
            }
        }
        let mut missing = Vec::new();
        for preset in &snap.meta.presets {
            for step in &preset.steps {
                // Breachstone presets reference Currency-branch nodes.
                let womb = if preset.womb == "breachstone" {
                    "currency"
                } else {
                    preset.womb.as_str()
                };
                if !names.contains(&(womb.to_string(), step.node.clone())) {
                    missing.push(format!("{}: {}", preset.id, step.node));
                }
            }
            for avoid in &preset.avoid {
                let womb = if preset.womb == "breachstone" {
                    "currency"
                } else {
                    preset.womb.as_str()
                };
                if !names.contains(&(womb.to_string(), avoid.node.clone())) {
                    missing.push(format!("{} (avoid): {}", preset.id, avoid.node));
                }
            }
        }
        assert!(
            missing.is_empty(),
            "preset steps referencing unknown nodes:\n{}",
            missing.join("\n")
        );
    }
}
