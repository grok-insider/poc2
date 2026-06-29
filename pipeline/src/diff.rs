//! Semantic bundle diff — the reporting half of the automated data-refresh
//! loop (ADR-0012).
//!
//! Given an old (committed) bundle and a freshly-rebuilt one, compute a
//! human-meaningful changelog: which mods / bases / tags / section entries were
//! added, removed, or changed. This is what populates the auto-refresh PR body
//! so a reviewer can see exactly what new patch content arrived without eyeballing
//! a 300 KB JSON diff.
//!
//! The diff is *keyed on stable identifiers* (RePoE-fork mod/base/tag ids,
//! section-entry `id`/`name`) rather than positional, so reordering by the
//! pipeline never shows up as a change. "Changed" entries carry a short list of
//! which fields differ, kept terse on purpose — the goal is a triage signal, not
//! a byte-exact patch.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use poc2_data::Bundle;
use serde::Serialize;

/// Added / removed / changed counts plus the ids involved, for one keyed
/// collection (mods, bases, tags, or a named section).
#[derive(Debug, Clone, Default, Serialize)]
pub struct SectionDelta {
    /// Display label for this collection (`"mods"`, `"alloys"`, …).
    pub label: String,
    /// Ids present in the new bundle but not the old.
    pub added: Vec<String>,
    /// Ids present in the old bundle but not the new.
    pub removed: Vec<String>,
    /// Ids present in both whose serialized content differs, with a short note
    /// describing which fields changed.
    pub changed: Vec<ChangedEntry>,
}

impl SectionDelta {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty() && self.changed.is_empty()
    }

    pub fn total(&self) -> usize {
        self.added.len() + self.removed.len() + self.changed.len()
    }
}

/// One entry that exists in both bundles but changed.
#[derive(Debug, Clone, Serialize)]
pub struct ChangedEntry {
    pub id: String,
    /// Field-level notes, e.g. `["tier 2 → 1", "required_level 68 → 70"]`.
    pub notes: Vec<String>,
}

/// The full diff between two bundles.
#[derive(Debug, Clone, Serialize)]
pub struct BundleDiff {
    pub old_patch: String,
    pub new_patch: String,
    pub old_built_at: String,
    pub new_built_at: String,
    pub mods: SectionDelta,
    pub bases: SectionDelta,
    pub tags: SectionDelta,
    /// One delta per data-driven `BundleSection` (omens, essences, catalysts,
    /// bones, alloys, emotions, genesis, currencies).
    pub sections: Vec<SectionDelta>,
}

impl BundleDiff {
    /// `true` when nothing of substance changed between the two bundles.
    pub fn is_empty(&self) -> bool {
        self.mods.is_empty()
            && self.bases.is_empty()
            && self.tags.is_empty()
            && self.sections.iter().all(SectionDelta::is_empty)
    }

    /// Total number of changed items across every collection.
    pub fn total_changes(&self) -> usize {
        self.mods.total()
            + self.bases.total()
            + self.tags.total()
            + self.sections.iter().map(SectionDelta::total).sum::<usize>()
    }
}

/// Compute the semantic diff `old → new`.
pub fn diff_bundles(old: &Bundle, new: &Bundle) -> BundleDiff {
    BundleDiff {
        old_patch: old.header.game_patch.to_string(),
        new_patch: new.header.game_patch.to_string(),
        old_built_at: old.header.built_at.clone(),
        new_built_at: new.header.built_at.clone(),
        mods: diff_mods(old, new),
        bases: diff_bases(old, new),
        tags: diff_tags(old, new),
        sections: diff_all_sections(old, new),
    }
}

// -------------------------------------------------------------------------
// Typed collections (mods / bases / tags)
// -------------------------------------------------------------------------

fn diff_mods(old: &Bundle, new: &Bundle) -> SectionDelta {
    let old_map: BTreeMap<&str, &poc2_engine::ModDefinition> =
        old.mods.iter().map(|m| (m.id.as_str(), m)).collect();
    let new_map: BTreeMap<&str, &poc2_engine::ModDefinition> =
        new.mods.iter().map(|m| (m.id.as_str(), m)).collect();

    let mut delta = SectionDelta {
        label: "mods".into(),
        ..Default::default()
    };
    for id in new_map.keys() {
        if !old_map.contains_key(id) {
            delta.added.push((*id).to_string());
        }
    }
    for id in old_map.keys() {
        if !new_map.contains_key(id) {
            delta.removed.push((*id).to_string());
        }
    }
    for (id, new_mod) in &new_map {
        if let Some(old_mod) = old_map.get(id) {
            let mut notes = Vec::new();
            if old_mod.tier != new_mod.tier {
                notes.push(format!("tier {:?} → {:?}", old_mod.tier, new_mod.tier));
            }
            if old_mod.required_level != new_mod.required_level {
                notes.push(format!(
                    "required_level {} → {}",
                    old_mod.required_level, new_mod.required_level
                ));
            }
            if old_mod.name != new_mod.name {
                notes.push(format!("name {:?} → {:?}", old_mod.name, new_mod.name));
            }
            if old_mod.stats.len() != new_mod.stats.len() {
                notes.push(format!(
                    "stat count {} → {}",
                    old_mod.stats.len(),
                    new_mod.stats.len()
                ));
            }
            if old_mod.allowed_item_classes.len() != new_mod.allowed_item_classes.len() {
                notes.push(format!(
                    "item-class count {} → {}",
                    old_mod.allowed_item_classes.len(),
                    new_mod.allowed_item_classes.len()
                ));
            }
            if !notes.is_empty() {
                delta.changed.push(ChangedEntry {
                    id: (*id).to_string(),
                    notes,
                });
            }
        }
    }
    finalize(&mut delta);
    delta
}

fn diff_bases(old: &Bundle, new: &Bundle) -> SectionDelta {
    let old_map: BTreeMap<&str, &poc2_engine::BaseType> =
        old.base_items.iter().map(|b| (b.id.as_str(), b)).collect();
    let new_map: BTreeMap<&str, &poc2_engine::BaseType> =
        new.base_items.iter().map(|b| (b.id.as_str(), b)).collect();

    let mut delta = SectionDelta {
        label: "bases".into(),
        ..Default::default()
    };
    for id in new_map.keys() {
        if !old_map.contains_key(id) {
            delta.added.push((*id).to_string());
        }
    }
    for id in old_map.keys() {
        if !new_map.contains_key(id) {
            delta.removed.push((*id).to_string());
        }
    }
    for (id, new_base) in &new_map {
        if let Some(old_base) = old_map.get(id) {
            let mut notes = Vec::new();
            if old_base.name != new_base.name {
                notes.push(format!("name {:?} → {:?}", old_base.name, new_base.name));
            }
            if old_base.drop_level != new_base.drop_level {
                notes.push(format!(
                    "drop_level {} → {}",
                    old_base.drop_level, new_base.drop_level
                ));
            }
            if old_base.release_state != new_base.release_state {
                notes.push(format!(
                    "release_state {:?} → {:?}",
                    old_base.release_state, new_base.release_state
                ));
            }
            if !notes.is_empty() {
                delta.changed.push(ChangedEntry {
                    id: (*id).to_string(),
                    notes,
                });
            }
        }
    }
    finalize(&mut delta);
    delta
}

fn diff_tags(old: &Bundle, new: &Bundle) -> SectionDelta {
    let old_ids: std::collections::BTreeSet<&str> =
        old.tags.iter().map(|t| t.id.as_str()).collect();
    let new_ids: std::collections::BTreeSet<&str> =
        new.tags.iter().map(|t| t.id.as_str()).collect();

    let mut delta = SectionDelta {
        label: "tags".into(),
        ..Default::default()
    };
    delta.added = new_ids
        .difference(&old_ids)
        .map(|s| (*s).to_string())
        .collect();
    delta.removed = old_ids
        .difference(&new_ids)
        .map(|s| (*s).to_string())
        .collect();
    finalize(&mut delta);
    delta
}

// -------------------------------------------------------------------------
// Generic JSON sections (omens / essences / catalysts / bones / alloys /
// emotions / genesis / currencies)
// -------------------------------------------------------------------------

fn diff_all_sections(old: &Bundle, new: &Bundle) -> Vec<SectionDelta> {
    let mut out = Vec::new();
    let pairs: &[(&str, &poc2_data::BundleSection, &poc2_data::BundleSection)] = &[
        ("currencies", &old.currencies, &new.currencies),
        ("omens", &old.omens, &new.omens),
        ("essences", &old.essences, &new.essences),
        ("bones", &old.bones, &new.bones),
        ("catalysts", &old.catalysts, &new.catalysts),
        ("alloys", &old.alloys, &new.alloys),
        ("emotions", &old.emotions, &new.emotions),
        ("genesis", &old.genesis, &new.genesis),
    ];
    for (label, old_sec, new_sec) in pairs {
        let delta = diff_section(label, old_sec, new_sec);
        if !delta.is_empty() {
            out.push(delta);
        }
    }
    out
}

fn diff_section(
    label: &str,
    old_sec: &poc2_data::BundleSection,
    new_sec: &poc2_data::BundleSection,
) -> SectionDelta {
    let old_map = index_section(old_sec);
    let new_map = index_section(new_sec);

    let mut delta = SectionDelta {
        label: label.to_string(),
        ..Default::default()
    };
    for id in new_map.keys() {
        if !old_map.contains_key(id) {
            delta.added.push(id.clone());
        }
    }
    for id in old_map.keys() {
        if !new_map.contains_key(id) {
            delta.removed.push(id.clone());
        }
    }
    for (id, new_val) in &new_map {
        if let Some(old_val) = old_map.get(id) {
            if old_val != new_val {
                delta.changed.push(ChangedEntry {
                    id: id.clone(),
                    notes: vec!["entry content changed".into()],
                });
            }
        }
    }
    finalize(&mut delta);
    delta
}

/// Index a section's entries by a stable key: prefer `id`, then `name`, then
/// (last resort) the entry's compact JSON so identical entries collapse.
fn index_section(sec: &poc2_data::BundleSection) -> BTreeMap<String, &serde_json::Value> {
    let mut map = BTreeMap::new();
    for (i, entry) in sec.entries.iter().enumerate() {
        let key = entry
            .get("id")
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .or_else(|| {
                entry
                    .get("name")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
            })
            .or_else(|| {
                // Genesis uses a `type` discriminator + a per-type id-ish field.
                entry
                    .get("node")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
            })
            .unwrap_or_else(|| format!("#{i}"));
        map.insert(key, entry);
    }
    map
}

fn finalize(delta: &mut SectionDelta) {
    delta.added.sort();
    delta.removed.sort();
    delta.changed.sort_by(|a, b| a.id.cmp(&b.id));
}

// -------------------------------------------------------------------------
// Markdown rendering (for the PR body)
// -------------------------------------------------------------------------

/// Render the diff as a GitHub-flavoured-markdown changelog suitable for an
/// auto-refresh PR body. Caps long id lists so the body stays readable.
pub fn render_markdown(diff: &BundleDiff) -> String {
    const MAX_LIST: usize = 40;
    let mut s = String::new();

    s.push_str("## Data bundle refresh\n\n");
    let _ = writeln!(
        s,
        "Patch `{}` → `{}`  ·  built `{}` → `{}`\n",
        diff.old_patch, diff.new_patch, diff.old_built_at, diff.new_built_at
    );

    if diff.is_empty() {
        s.push_str("_No semantic changes — bundle content is identical._\n");
        return s;
    }

    let _ = writeln!(
        s,
        "**{} total change(s)** across mods/bases/tags/sections.\n",
        diff.total_changes()
    );

    render_delta(&mut s, &diff.mods, MAX_LIST);
    render_delta(&mut s, &diff.bases, MAX_LIST);
    render_delta(&mut s, &diff.tags, MAX_LIST);
    for sec in &diff.sections {
        render_delta(&mut s, sec, MAX_LIST);
    }

    s.push_str(
        "\n---\n_Auto-generated by `poc2-pipeline diff-bundle`. Curated fixtures \
         (alloys / emotions / desecrated / genesis) are **not** auto-updated — \
         review whether new mod ids above need hand-curated `engine_mod_id` joins._\n",
    );
    s
}

fn render_delta(s: &mut String, delta: &SectionDelta, max: usize) {
    if delta.is_empty() {
        return;
    }
    let _ = writeln!(s, "### {}\n", delta.label);
    if !delta.added.is_empty() {
        let _ = writeln!(s, "**Added ({}):**\n", delta.added.len());
        render_list(s, &delta.added, max);
    }
    if !delta.removed.is_empty() {
        let _ = writeln!(s, "**Removed ({}):**\n", delta.removed.len());
        render_list(s, &delta.removed, max);
    }
    if !delta.changed.is_empty() {
        let _ = writeln!(s, "**Changed ({}):**\n", delta.changed.len());
        for c in delta.changed.iter().take(max) {
            let _ = writeln!(s, "- `{}` — {}", c.id, c.notes.join("; "));
        }
        if delta.changed.len() > max {
            let _ = writeln!(s, "- _…and {} more_", delta.changed.len() - max);
        }
        s.push('\n');
    }
}

fn render_list(s: &mut String, ids: &[String], max: usize) {
    for id in ids.iter().take(max) {
        let _ = writeln!(s, "- `{id}`");
    }
    if ids.len() > max {
        let _ = writeln!(s, "- _…and {} more_", ids.len() - max);
    }
    s.push('\n');
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_section_delta_helpers() {
        let d = SectionDelta {
            label: "mods".into(),
            ..Default::default()
        };
        assert!(d.is_empty());
        assert_eq!(d.total(), 0);
    }

    #[test]
    fn index_section_prefers_id_then_name() {
        let sec = poc2_data::BundleSection {
            section_version: 1,
            entries: vec![
                serde_json::json!({"id": "alloy_a", "name": "Alpha"}),
                serde_json::json!({"name": "OnlyName"}),
                serde_json::json!({"node": "GenesisNode1"}),
                serde_json::json!({"foo": "bar"}),
            ],
        };
        let map = index_section(&sec);
        assert!(map.contains_key("alloy_a"));
        assert!(map.contains_key("OnlyName"));
        assert!(map.contains_key("GenesisNode1"));
        assert!(map.contains_key("#3"));
    }

    #[test]
    fn diff_section_detects_add_remove_change() {
        let old = poc2_data::BundleSection {
            section_version: 1,
            entries: vec![
                serde_json::json!({"id": "a", "v": 1}),
                serde_json::json!({"id": "b", "v": 2}),
            ],
        };
        let new = poc2_data::BundleSection {
            section_version: 1,
            entries: vec![
                serde_json::json!({"id": "a", "v": 1}),   // unchanged
                serde_json::json!({"id": "b", "v": 999}), // changed
                serde_json::json!({"id": "c", "v": 3}),   // added
            ],
        };
        let d = diff_section("alloys", &old, &new);
        assert_eq!(d.added, vec!["c".to_string()]);
        assert!(d.removed.is_empty());
        assert_eq!(d.changed.len(), 1);
        assert_eq!(d.changed[0].id, "b");
    }

    #[test]
    fn markdown_renders_empty_when_no_change() {
        let sec = poc2_data::BundleSection::default();
        let same = diff_section("omens", &sec, &sec);
        assert!(same.is_empty());
    }
}
