//! Genesis Tree view command — serves the bundle's `genesis` section
//! (wombs, positioned nodes, goal presets, farming notes, videos) to the
//! web UI as one typed payload. Pure read; the engine never simulates
//! Genesis births.

use poc2_data::Bundle;
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Serialize)]
pub struct GenesisWomb {
    pub branch: String,
    pub display_name: String,
    pub wombgift: String,
    pub gift_art: String,
    pub points: u32,
    pub icon_normal: String,
    pub icon_notable: String,
    pub blurb: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct GenesisNode {
    pub id: String,
    pub branch: String,
    pub name: String,
    pub notable: bool,
    pub icon: String,
    pub description: String,
    pub x: f64,
    pub y: f64,
    pub start: bool,
    pub womb_slot: bool,
    pub connections: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GenesisPresetStep {
    pub node: String,
    pub why: String,
    pub priority: u32,
    /// Filler step: spend leftover points on any highlighted copy.
    pub fill: bool,
    /// Respec / swap-in choice — not part of the guaranteed budget.
    pub optional: bool,
    /// Resolved node ids to allocate for this step.
    pub node_ids: Vec<String>,
    /// Pathing nodes the shortest route forces along the way.
    pub connector_ids: Vec<String>,
    /// Cumulative allocatable points after this step (incl. connectors).
    pub points_after: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct GenesisPresetAvoid {
    pub node: String,
    pub why: String,
    pub node_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GenesisPreset {
    pub id: String,
    pub name: String,
    pub womb: String,
    pub confidence: String,
    pub summary: String,
    pub sources: Vec<String>,
    pub steps: Vec<GenesisPresetStep>,
    pub avoid: Vec<GenesisPresetAvoid>,
    pub gift_advice: String,
    /// Points required by the non-optional path (incl. connectors).
    pub core_points: u32,
    /// The womb's point cap.
    pub points_cap: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct GenesisVideo {
    pub title: String,
    pub channel: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct GenesisTreeView {
    pub available: bool,
    pub wombs: Vec<GenesisWomb>,
    pub nodes: Vec<GenesisNode>,
    pub presets: Vec<GenesisPreset>,
    pub farming_notes: Vec<String>,
    pub videos: Vec<GenesisVideo>,
}

fn s(v: &Value, key: &str) -> String {
    v.get(key).and_then(Value::as_str).unwrap_or("").to_string()
}

fn b(v: &Value, key: &str) -> bool {
    v.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn f(v: &Value, key: &str) -> f64 {
    v.get(key).and_then(Value::as_f64).unwrap_or(0.0)
}

fn u(v: &Value, key: &str) -> u32 {
    v.get(key).and_then(Value::as_u64).unwrap_or(0) as u32
}

fn str_list(v: &Value, key: &str) -> Vec<String> {
    v.get(key)
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// Assemble the typed Genesis Tree view from the bundle's `genesis` section.
/// Returns `available: false` (everything empty) when the bundle predates
/// 0.5 or the section was never populated.
pub fn genesis_tree(bundle: &Bundle) -> GenesisTreeView {
    let mut view = GenesisTreeView::default();
    for entry in &bundle.genesis.entries {
        match entry.get("type").and_then(Value::as_str) {
            Some("womb") => view.wombs.push(GenesisWomb {
                branch: s(entry, "branch"),
                display_name: s(entry, "display_name"),
                wombgift: s(entry, "wombgift"),
                gift_art: s(entry, "gift_art"),
                points: u(entry, "points"),
                icon_normal: s(entry, "icon_normal"),
                icon_notable: s(entry, "icon_notable"),
                blurb: s(entry, "blurb"),
            }),
            Some("node") => view.nodes.push(GenesisNode {
                id: s(entry, "id"),
                branch: s(entry, "branch"),
                name: s(entry, "name"),
                notable: b(entry, "notable"),
                icon: s(entry, "icon"),
                description: s(entry, "description"),
                x: f(entry, "x"),
                y: f(entry, "y"),
                start: b(entry, "start"),
                womb_slot: b(entry, "womb_slot"),
                connections: str_list(entry, "connections"),
            }),
            Some("preset") => {
                let steps = entry
                    .get("steps")
                    .and_then(Value::as_array)
                    .map(|a| {
                        a.iter()
                            .map(|st| GenesisPresetStep {
                                node: s(st, "node"),
                                why: s(st, "why"),
                                priority: u(st, "priority"),
                                fill: b(st, "fill"),
                                optional: b(st, "optional"),
                                node_ids: str_list(st, "node_ids"),
                                connector_ids: str_list(st, "connector_ids"),
                                points_after: u(st, "points_after"),
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let avoid = entry
                    .get("avoid")
                    .and_then(Value::as_array)
                    .map(|a| {
                        a.iter()
                            .map(|av| GenesisPresetAvoid {
                                node: s(av, "node"),
                                why: s(av, "why"),
                                node_ids: str_list(av, "node_ids"),
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                view.presets.push(GenesisPreset {
                    id: s(entry, "id"),
                    name: s(entry, "name"),
                    womb: s(entry, "womb"),
                    confidence: s(entry, "confidence"),
                    summary: s(entry, "summary"),
                    sources: str_list(entry, "sources"),
                    steps,
                    avoid,
                    gift_advice: s(entry, "gift_advice"),
                    core_points: u(entry, "core_points"),
                    points_cap: u(entry, "points_cap"),
                });
            }
            Some("farming") => view.farming_notes.extend(str_list(entry, "notes")),
            Some("video") => view.videos.push(GenesisVideo {
                title: s(entry, "title"),
                channel: s(entry, "channel"),
                url: s(entry, "url"),
            }),
            _ => {}
        }
    }
    // Stable ordering: wombs in game order, presets by id, nodes by id.
    let womb_order = ["currency", "ring", "amulet", "belt", "breachstone"];
    view.wombs.sort_by_key(|w| {
        womb_order
            .iter()
            .position(|o| *o == w.branch)
            .unwrap_or(usize::MAX)
    });
    view.nodes.sort_by(|a, b| a.id.cmp(&b.id));
    view.available = !view.nodes.is_empty();
    view
}

#[cfg(test)]
mod tests {
    use super::*;
    use poc2_engine::PatchVersion;

    #[test]
    fn empty_bundle_reports_unavailable() {
        let bundle = Bundle::empty(PatchVersion::PATCH_0_4_0, "test");
        let view = genesis_tree(&bundle);
        assert!(!view.available);
        assert!(view.nodes.is_empty() && view.wombs.is_empty());
    }

    #[test]
    fn typed_view_assembles_from_section_entries() {
        let mut bundle = Bundle::empty(PatchVersion::PATCH_0_5_0, "test");
        bundle.genesis.section_version = 1;
        bundle.genesis.entries = vec![
            serde_json::json!({
                "type": "womb", "branch": "currency", "display_name": "Currency Womb",
                "wombgift": "Lavish Wombgift", "gift_art": "BreachFruit2", "points": 15,
                "icon_normal": "KeepersCurrencyNode", "icon_notable": "KeepersCurrencyNotable",
                "blurb": "b"
            }),
            serde_json::json!({
                "type": "node", "id": "BrequelTreeCurrency1", "branch": "currency",
                "name": "Increased Divine Orb Chance", "notable": false,
                "icon": "KeepersCurrencyNode",
                "description": "50% increased chance to Birth Divine Orbs",
                "x": 10.0, "y": -20.0, "start": false, "womb_slot": false,
                "connections": ["BrequelTreeCurrency2"]
            }),
            serde_json::json!({
                "type": "preset", "id": "divine-farm", "name": "Divine farming",
                "womb": "currency", "confidence": "community", "summary": "s",
                "sources": ["url"],
                "steps": [{"node": "Increased Divine Orb Chance", "why": "w", "priority": 1}],
                "avoid": [{"node": "Known Value", "why": "blocks divines"}],
                "gift_advice": "g"
            }),
            serde_json::json!({"type": "farming", "notes": ["note1"]}),
            serde_json::json!({"type": "video", "title": "t", "channel": "c", "url": "u"}),
        ];
        let view = genesis_tree(&bundle);
        assert!(view.available);
        assert_eq!(view.wombs.len(), 1);
        assert_eq!(view.nodes.len(), 1);
        assert_eq!(view.presets.len(), 1);
        assert_eq!(view.presets[0].steps.len(), 1);
        assert_eq!(view.presets[0].avoid.len(), 1);
        assert_eq!(view.farming_notes, vec!["note1"]);
        assert_eq!(view.videos.len(), 1);
    }
}
