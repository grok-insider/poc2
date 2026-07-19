//! Genesis Tree snapshot → `bundle.genesis` section.
//!
//! Emits three entry types (discriminated by `"type"`):
//!
//! - `"womb"` — one per branch: display name, Wombgift, point cap, node-icon
//!   classes, blurb.
//! - `"node"` — one per allocatable passive: stable id, womb branch, notable
//!   flag, icon class, human-readable description (stat templates + curated
//!   overrides), **computed layout position** (group + orbit math resolved to
//!   a flat `x`/`y`), and connection edges (by node id).
//! - `"preset"` / `"farming"` / `"video"` — curated advisor knowledge
//!   (community node allocations per goal, Hiveblood notes, vetted videos).
//!
//! The engine never simulates Genesis births; this section is pure UI/advisor
//! knowledge served by the WASM `genesisTree` command.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use poc2_data::Bundle;
use serde_json::json;
use tracing::{info, warn};

use crate::error::PipelineResult;
use crate::sources::genesis::{GenesisSnapshot, PresetMeta, RawPlacement, RawTree};

/// Derive the womb branch from a node id (`BrequelTreeCurrency12` →
/// `currency`). Returns `None` for ids outside the five known branches.
pub fn branch_of(node_id: &str) -> Option<&'static str> {
    let rest = node_id.strip_prefix("BrequelTree")?;
    for (prefix, branch) in [
        ("Breachstones", "breachstone"),
        ("Currency", "currency"),
        ("Amulets", "amulet"),
        ("Rings", "ring"),
        ("Belts", "belt"),
    ] {
        if rest.starts_with(prefix) {
            return Some(branch);
        }
    }
    None
}

/// Trim a float to a compact display string (`f64`'s `Display` already
/// drops the trailing `.0` for integral values).
fn fmt_value(v: f64) -> String {
    format!("{v}")
}

/// Render one stat line via the curated template map. `{}` receives the
/// value; `resource_cost_+%` flips negative values to "reduced" phrasing.
fn render_stat(templates: &BTreeMap<String, String>, key: &str, value: f64) -> Option<String> {
    let template = templates.get(key)?;
    if key == "brequel_reward_resource_cost_+%" && value < 0.0 {
        return Some(format!(
            "Birthing consumes {}% reduced Hiveblood",
            fmt_value(-value)
        ));
    }
    Some(template.replace("{}", &fmt_value(value)))
}

/// Strip a tree icon path (`Art/2DArt/SkillIcons/passives/KeepersCurrencyNode.dds`)
/// down to its asset key (`KeepersCurrencyNode`).
fn icon_key(icon: &str) -> String {
    icon.rsplit('/')
        .next()
        .unwrap_or(icon)
        .trim_end_matches(".dds")
        .trim_end_matches(".webp")
        .to_string()
}

/// One resolved preset step: the exact node ids to allocate, plus the
/// connector nodes the shortest path forces, and the cumulative point cost.
pub struct ResolvedStep {
    pub name: String,
    pub why: String,
    pub priority: u32,
    pub fill: bool,
    pub optional: bool,
    pub node_ids: Vec<String>,
    pub connector_ids: Vec<String>,
    pub points_after: u32,
}

/// A preset resolved against the tree graph.
pub struct ResolvedPreset {
    pub steps: Vec<ResolvedStep>,
    /// Points needed for all non-optional, non-fill steps (incl. connectors).
    pub core_points: u32,
    /// Step node names that could not be resolved or connected.
    pub unresolved: Vec<String>,
}

/// Per-branch graph view over the raw tree: adjacency by hash, name index,
/// womb start roots. Used to resolve presets into connected node sets.
pub struct BranchGraphs {
    adj: BTreeMap<u64, Vec<u64>>,
    id_of: BTreeMap<u64, String>,
    branch_of_hash: BTreeMap<u64, &'static str>,
    slot_or_start: BTreeSet<u64>,
    name_idx: BTreeMap<(String, String), Vec<u64>>,
    roots: BTreeMap<&'static str, u64>,
}

impl BranchGraphs {
    pub fn build(tree: &RawTree) -> Self {
        let mut adj: BTreeMap<u64, Vec<u64>> = BTreeMap::new();
        for g in &tree.groups {
            for p in &g.passives {
                for c in &p.connections {
                    adj.entry(p.hash).or_default().push(*c);
                    adj.entry(*c).or_default().push(p.hash);
                }
            }
        }
        let mut id_of = BTreeMap::new();
        let mut branch_of_hash = BTreeMap::new();
        let mut slot_or_start = BTreeSet::new();
        let mut name_idx: BTreeMap<(String, String), Vec<u64>> = BTreeMap::new();
        let mut roots = BTreeMap::new();
        let root_set: BTreeSet<u64> = tree.roots.iter().copied().collect();
        for p in tree.passives.values() {
            let Some(branch) = branch_of(&p.id) else {
                continue;
            };
            id_of.insert(p.hash, p.id.clone());
            branch_of_hash.insert(p.hash, branch);
            if p.id.contains("Slot") || root_set.contains(&p.hash) {
                slot_or_start.insert(p.hash);
            }
            if root_set.contains(&p.hash) {
                roots.insert(branch, p.hash);
            }
            if !p.name.is_empty() {
                name_idx
                    .entry((branch.to_string(), p.name.clone()))
                    .or_default()
                    .push(p.hash);
            }
        }
        Self {
            adj,
            id_of,
            branch_of_hash,
            slot_or_start,
            name_idx,
            roots,
        }
    }

    /// All node ids carrying `name` within `branch`.
    pub fn ids_by_name(&self, branch: &str, name: &str) -> Vec<String> {
        self.name_idx
            .get(&(branch.to_string(), name.to_string()))
            .map(|hs| {
                hs.iter()
                    .filter_map(|h| self.id_of.get(h).cloned())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Shortest path (BFS) from the connected set to the nearest hash in
    /// `targets`, staying inside `branch`. Returns the path of *new* nodes
    /// ending at the reached target.
    fn shortest_attach(
        &self,
        connected: &BTreeSet<u64>,
        targets: &BTreeSet<u64>,
        branch: &str,
    ) -> Option<Vec<u64>> {
        let mut queue: VecDeque<u64> = connected.iter().copied().collect();
        let mut prev: BTreeMap<u64, u64> = BTreeMap::new();
        let mut seen: BTreeSet<u64> = connected.clone();
        while let Some(cur) = queue.pop_front() {
            if targets.contains(&cur) && !connected.contains(&cur) {
                // Walk back to the connected frontier.
                let mut path = vec![cur];
                let mut at = cur;
                while let Some(&p) = prev.get(&at) {
                    if connected.contains(&p) {
                        break;
                    }
                    path.push(p);
                    at = p;
                }
                path.reverse();
                return Some(path);
            }
            for &n in self.adj.get(&cur).map_or(&[][..], |v| v.as_slice()) {
                if seen.contains(&n) {
                    continue;
                }
                if self.branch_of_hash.get(&n) != Some(&branch) {
                    continue;
                }
                seen.insert(n);
                prev.insert(n, cur);
                queue.push_back(n);
            }
        }
        None
    }

    /// Allocatable points in the connected set (excludes the start root and
    /// womb-slot nodes, which cost nothing).
    fn points(&self, connected: &BTreeSet<u64>) -> u32 {
        u32::try_from(
            connected
                .iter()
                .filter(|h| !self.slot_or_start.contains(h))
                .count(),
        )
        .unwrap_or(u32::MAX)
    }

    /// Resolve a preset's steps into connected node id sets (see module doc).
    pub fn resolve_preset(&self, branch: &str, preset: &PresetMeta) -> ResolvedPreset {
        let mut steps: Vec<&crate::sources::genesis::PresetStep> = preset.steps.iter().collect();
        steps.sort_by_key(|s| s.priority);

        let mut connected: BTreeSet<u64> = BTreeSet::new();
        if let Some(&root) = self.roots.get(branch) {
            connected.insert(root);
        }
        let mut out_steps: Vec<ResolvedStep> = Vec::new();
        let mut unresolved: Vec<String> = Vec::new();
        let mut core_points = 0u32;
        let mut fill_indices: Vec<usize> = Vec::new();

        for step in &steps {
            if step.fill {
                // Deferred: resolved against the final core+optional path.
                fill_indices.push(out_steps.len());
                out_steps.push(ResolvedStep {
                    name: step.node.clone(),
                    why: step.why.clone(),
                    priority: step.priority,
                    fill: true,
                    optional: step.optional,
                    node_ids: Vec::new(),
                    connector_ids: Vec::new(),
                    points_after: 0,
                });
                continue;
            }
            let copies: BTreeSet<u64> = self
                .name_idx
                .get(&(branch.to_string(), step.node.clone()))
                .map(|v| v.iter().copied().collect())
                .unwrap_or_default();
            if copies.is_empty() {
                unresolved.push(step.node.clone());
                continue;
            }
            let mut node_ids = Vec::new();
            let mut connector_ids = Vec::new();
            // Copies already on the path (picked up as connectors of an
            // earlier step) are claimed by this step at zero extra cost.
            for h in copies.iter().filter(|h| connected.contains(h)) {
                if let Some(id) = self.id_of.get(h) {
                    node_ids.push(id.clone());
                }
            }
            loop {
                let remaining: BTreeSet<u64> = copies
                    .iter()
                    .copied()
                    .filter(|h| !connected.contains(h))
                    .collect();
                if remaining.is_empty() {
                    break;
                }
                let Some(path) = self.shortest_attach(&connected, &remaining, branch) else {
                    unresolved.push(step.node.clone());
                    break;
                };
                for h in &path {
                    connected.insert(*h);
                    let id = self.id_of.get(h).cloned().unwrap_or_default();
                    if copies.contains(h) {
                        node_ids.push(id);
                    } else {
                        connector_ids.push(id);
                    }
                }
            }
            let points_after = self.points(&connected);
            if !step.optional {
                core_points = points_after;
            }
            out_steps.push(ResolvedStep {
                name: step.node.clone(),
                why: step.why.clone(),
                priority: step.priority,
                fill: false,
                optional: step.optional,
                node_ids,
                connector_ids,
                points_after,
            });
        }

        // Fill steps: claim copies adjacent to the resolved path.
        for idx in fill_indices {
            let name = out_steps[idx].name.clone();
            let copies = self
                .name_idx
                .get(&(branch.to_string(), name))
                .cloned()
                .unwrap_or_default();
            let adjacent: Vec<String> = copies
                .iter()
                .filter(|h| !connected.contains(h))
                .filter(|h| {
                    self.adj
                        .get(h)
                        .is_some_and(|ns| ns.iter().any(|n| connected.contains(n)))
                })
                .filter_map(|h| self.id_of.get(h).cloned())
                .collect();
            // Already-on-path copies count too (e.g. Catalyst Chance en route).
            let on_path: Vec<String> = copies
                .iter()
                .filter(|h| connected.contains(h))
                .filter_map(|h| self.id_of.get(h).cloned())
                .collect();
            out_steps[idx].node_ids = on_path.into_iter().chain(adjacent).collect();
            out_steps[idx].points_after = self.points(&connected);
        }

        ResolvedPreset {
            steps: out_steps,
            core_points,
            unresolved,
        }
    }
}

/// Re-classify the 0.5 "Otherworldly" mods (M14 audit / poe2db research).
///
/// RePoE exports them under `GenesisTree*Crafted` ids as plain explicit
/// mods with zero spawn weights and no allowed classes — invisible to the
/// advisor and unreachable by any modeled mechanic. poe2db places them in
/// the per-class "Otherworldly" sections of Amulets (4), Rings (16) and
/// Belts (16): they are granted ONLY by the Altered Collarbone
/// breach-desecration ("Desecrates a Rare Amulet, Ring or Belt with a
/// chance for otherworldly modifiers") and revealed like desecrated mods.
/// So: kind = Desecrated, flags |= OTHERWORLDLY, and the item class derived
/// from the id (fallback: spawn-weight tags for the two class-less ids).
///
/// Also repairs the one value drift poe2db shows: the Ring Offering-effect
/// prefix is "Dedicated" with (23—30)% in 0.5 (bundle carried the older
/// "Sacrificial" 16—23 export).
fn flag_otherworldly_mods(bundle: &mut Bundle) {
    use poc2_engine::ids::ItemClassId;
    use poc2_engine::mods::{ModFlags, ModKind};

    let mut converted = 0usize;
    for m in &mut bundle.mods {
        let id = m.id.as_str();
        if !(id.starts_with("GenesisTree") && id.ends_with("Crafted")) {
            continue;
        }
        let class = if id.contains("Belt") {
            Some("Belt")
        } else if id.contains("Ring") {
            Some("Ring")
        } else if id.contains("Amulet") {
            Some("Amulet")
        } else {
            // Class-less ids (e.g. GenesisTreeAdditionalMaximumSealsCrafted)
            // carry their class as a spawn-weight tag.
            m.spawn_weights.iter().find_map(|sw| match sw.tag.as_str() {
                "amulet" => Some("Amulet"),
                "ring" => Some("Ring"),
                "belt" => Some("Belt"),
                _ => None,
            })
        };
        let Some(class) = class else { continue };
        m.kind = ModKind::Desecrated;
        m.flags |= ModFlags::OTHERWORLDLY;
        if m.allowed_item_classes.is_empty() {
            m.allowed_item_classes.push(ItemClassId::from(class));
        }
        if id == "GenesisTreeRingOfferingEffectCrafted" {
            m.name = Some("Dedicated".into());
            for stat in &mut m.stats {
                stat.min = 23.0;
                stat.max = 30.0;
            }
        }
        converted += 1;
    }
    info!(
        converted,
        "otherworldly mods reclassified (kind=desecrated, OTHERWORLDLY flag)"
    );
}

/// Populate `bundle.genesis` from the embedded snapshot.
#[allow(clippy::unnecessary_wraps)] // forward-compat with future fallible joins
pub fn normalize_genesis(snapshot: &GenesisSnapshot, bundle: &mut Bundle) -> PipelineResult<()> {
    flag_otherworldly_mods(bundle);
    let tree = &snapshot.tree;
    let meta = &snapshot.meta;

    let overrides: BTreeMap<&str, &str> = meta
        .node_overrides
        .iter()
        .map(|o| (o.id.as_str(), o.description.as_str()))
        .collect();

    // hash → (group x/y, placement) for position + edge resolution.
    let mut placement: BTreeMap<u64, (f64, f64, &RawPlacement)> = BTreeMap::new();
    for g in &tree.groups {
        for p in &g.passives {
            placement.insert(p.hash, (g.x, g.y, p));
        }
    }
    let hash_to_id: BTreeMap<u64, &str> = tree
        .passives
        .values()
        .map(|p| (p.hash, p.id.as_str()))
        .collect();
    let roots: std::collections::BTreeSet<u64> = tree.roots.iter().copied().collect();

    let mut entries: Vec<serde_json::Value> = Vec::new();

    // ---- Womb metadata ---------------------------------------------------
    for (branch, womb) in &meta.wombs {
        entries.push(json!({
            "type": "womb",
            "branch": branch,
            "display_name": womb.display_name,
            "wombgift": womb.wombgift,
            "gift_art": womb.gift_art,
            "points": womb.points,
            "icon_normal": womb.icon_normal,
            "icon_notable": womb.icon_notable,
            "blurb": womb.blurb,
        }));
    }

    // ---- Nodes -------------------------------------------------------------
    let mut node_count = 0usize;
    let mut missing_desc = 0usize;
    for passive in tree.passives.values() {
        let Some(branch) = branch_of(&passive.id) else {
            continue;
        };
        let Some(&(gx, gy, place)) = placement.get(&passive.hash) else {
            continue;
        };

        // Orbit math: angle from 12 o'clock, clockwise.
        let radius = tree.orbit_radii.get(place.radius).copied().unwrap_or(0.0);
        let per_orbit = tree
            .skills_per_orbit
            .get(place.radius)
            .copied()
            .unwrap_or(1)
            .max(1);
        let angle =
            std::f64::consts::TAU * (f64::from(place.position_clockwise) / f64::from(per_orbit));
        let x = gx + radius * angle.sin();
        let y = gy - radius * angle.cos();

        // Description: curated override first, then stat templates.
        let description = if let Some(d) = overrides.get(passive.id.as_str()) {
            (*d).to_string()
        } else {
            let lines: Vec<String> = passive
                .stats
                .iter()
                .filter_map(|(k, v)| render_stat(&meta.stat_templates, k, *v))
                .collect();
            if lines.is_empty() && !passive.stats.is_empty() {
                missing_desc += 1;
            }
            lines.join("\n")
        };

        let connections: Vec<&str> = place
            .connections
            .iter()
            .filter_map(|h| hash_to_id.get(h).copied())
            .collect();

        let is_start = roots.contains(&passive.hash);
        let is_womb_slot = passive.id.contains("Slot");

        entries.push(json!({
            "type": "node",
            "id": passive.id,
            "hash": passive.hash,
            "branch": branch,
            "name": passive.name,
            "notable": passive.is_notable,
            "icon": icon_key(&passive.icon),
            "description": description,
            "x": x,
            "y": y,
            "start": is_start,
            "womb_slot": is_womb_slot,
            "connections": connections,
        }));
        node_count += 1;
    }

    // ---- Presets -----------------------------------------------------------
    // Resolve each preset against the real tree graph: every non-fill step
    // attaches its node copies to the growing connected set via shortest
    // paths from the womb's start node, recording the connector nodes the
    // player must also allocate. Fill steps claim copies adjacent to the
    // resolved path. This guarantees highlighted presets are *connected*
    // and lets the UI show honest point costs.
    let graph = BranchGraphs::build(tree);
    for preset in &meta.presets {
        // Breachstone presets reference Currency-branch nodes (the womb
        // itself has no passives).
        let branch = if preset.womb == "breachstone" {
            "currency"
        } else {
            preset.womb.as_str()
        };
        let resolved = graph.resolve_preset(branch, preset);
        let cap = meta.wombs.get(&preset.womb).map_or(0, |w| w.points);

        let steps: Vec<serde_json::Value> = resolved
            .steps
            .iter()
            .map(|s| {
                json!({
                    "node": s.name,
                    "why": s.why,
                    "priority": s.priority,
                    "fill": s.fill,
                    "optional": s.optional,
                    "node_ids": s.node_ids,
                    "connector_ids": s.connector_ids,
                    "points_after": s.points_after,
                })
            })
            .collect();
        let avoid: Vec<serde_json::Value> = preset
            .avoid
            .iter()
            .map(|a| {
                let ids = graph.ids_by_name(branch, &a.node);
                json!({"node": a.node, "why": a.why, "node_ids": ids})
            })
            .collect();
        if !resolved.unresolved.is_empty() {
            warn!(
                preset = %preset.id,
                unresolved = ?resolved.unresolved,
                "genesis preset references nodes that did not resolve/connect"
            );
        }
        entries.push(json!({
            "type": "preset",
            "id": preset.id,
            "name": preset.name,
            "womb": preset.womb,
            "confidence": preset.confidence,
            "summary": preset.summary,
            "sources": preset.sources,
            "steps": steps,
            "avoid": avoid,
            "gift_advice": preset.gift_advice,
            "core_points": resolved.core_points,
            "points_cap": cap,
        }));
    }

    // ---- Farming notes + videos ---------------------------------------------
    if !meta.farming.notes.is_empty() {
        entries.push(json!({
            "type": "farming",
            "notes": meta.farming.notes,
        }));
    }
    for video in &meta.videos {
        entries.push(json!({
            "type": "video",
            "title": video.title,
            "channel": video.channel,
            "url": video.url,
        }));
    }

    bundle.genesis.section_version = 1;
    bundle.genesis.entries = entries;
    bundle.header.sources.0.push(snapshot.revision.clone());

    info!(
        nodes = node_count,
        wombs = meta.wombs.len(),
        presets = meta.presets.len(),
        missing_descriptions = missing_desc,
        "genesis tree populated"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use poc2_engine::PatchVersion;

    fn build() -> Bundle {
        let snap = crate::sources::genesis::load().expect("genesis snapshot");
        let mut bundle = Bundle::empty(PatchVersion::PATCH_0_5_0, "test");
        normalize_genesis(&snap, &mut bundle).expect("normalize genesis");
        bundle
    }

    #[test]
    fn genesis_section_populates() {
        let bundle = build();
        assert!(bundle.genesis.entries.len() > 250, "wombs + nodes + extras");
        let nodes = bundle
            .genesis
            .entries
            .iter()
            .filter(|e| e["type"] == "node")
            .count();
        assert_eq!(nodes, 248, "all 248 passives must normalize");
        let wombs = bundle
            .genesis
            .entries
            .iter()
            .filter(|e| e["type"] == "womb")
            .count();
        assert_eq!(wombs, 5);
        let presets = bundle
            .genesis
            .entries
            .iter()
            .filter(|e| e["type"] == "preset")
            .count();
        assert!(presets >= 6);
    }

    #[test]
    fn every_stat_bearing_node_has_a_description() {
        let bundle = build();
        let missing: Vec<String> = bundle
            .genesis
            .entries
            .iter()
            .filter(|e| e["type"] == "node")
            .filter(|e| {
                !e["start"].as_bool().unwrap_or(false)
                    && !e["womb_slot"].as_bool().unwrap_or(false)
                    && e["description"].as_str().unwrap_or("").is_empty()
                    && !e["name"].as_str().unwrap_or("").is_empty()
            })
            .map(|e| format!("{} ({})", e["id"], e["name"]))
            .collect();
        assert!(
            missing.is_empty(),
            "nodes with empty descriptions:\n{}",
            missing.join("\n")
        );
    }

    #[test]
    fn node_positions_are_finite_and_spread() {
        let bundle = build();
        let mut xs: Vec<f64> = Vec::new();
        for e in bundle
            .genesis
            .entries
            .iter()
            .filter(|e| e["type"] == "node")
        {
            let x = e["x"].as_f64().expect("x finite");
            let y = e["y"].as_f64().expect("y finite");
            assert!(x.is_finite() && y.is_finite());
            xs.push(x);
        }
        let min = xs.iter().copied().fold(f64::INFINITY, f64::min);
        let max = xs.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        assert!(
            max - min > 1000.0,
            "tree should span a wide layout; got {min}..{max}"
        );
    }

    #[test]
    fn presets_resolve_connected_and_within_budget() {
        let bundle = build();
        for e in bundle
            .genesis
            .entries
            .iter()
            .filter(|e| e["type"] == "preset")
        {
            let id = e["id"].as_str().unwrap();
            let cap = e["points_cap"].as_u64().unwrap();
            let core = e["core_points"].as_u64().unwrap();
            // Breachstone has no own passives; its steps live in Currency
            // and are informational — skip the budget check.
            if e["womb"] == "breachstone" {
                continue;
            }
            assert!(
                core <= cap,
                "preset `{id}`: core points {core} exceed womb cap {cap}"
            );
            assert!(core > 0, "preset `{id}` resolved to an empty path");
            for s in e["steps"].as_array().unwrap() {
                let fill = s["fill"].as_bool().unwrap_or(false);
                let ids = s["node_ids"].as_array().unwrap();
                assert!(
                    !ids.is_empty() || fill,
                    "preset `{id}` step `{}` resolved no nodes",
                    s["node"]
                );
            }
        }
    }

    #[test]
    fn connections_reference_known_nodes() {
        let bundle = build();
        let ids: std::collections::BTreeSet<&str> = bundle
            .genesis
            .entries
            .iter()
            .filter(|e| e["type"] == "node")
            .filter_map(|e| e["id"].as_str())
            .collect();
        for e in bundle
            .genesis
            .entries
            .iter()
            .filter(|e| e["type"] == "node")
        {
            for c in e["connections"].as_array().unwrap() {
                assert!(
                    ids.contains(c.as_str().unwrap()),
                    "dangling connection {c} on {}",
                    e["id"]
                );
            }
        }
    }
}
