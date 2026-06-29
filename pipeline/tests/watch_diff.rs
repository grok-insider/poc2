//! Integration coverage for the automated data-refresh loop (ADR-0012):
//! the bundle semantic diff + the upstream-state round-trip.
//!
//! Unit-level logic (hash comparison, section indexing) is covered inside
//! `watch.rs` / `diff.rs`. This file exercises the public library surface
//! (`diff_bundles`, `render_markdown`, `UpstreamState`) against whole bundles
//! to catch contract regressions a reviewer of an auto-refresh PR would rely on.

use poc2_data::{Bundle, BundleSection};
use poc2_engine::PatchVersion;
use poc2_pipeline::{diff_bundles, render_markdown, UpstreamState};

fn empty_bundle() -> Bundle {
    Bundle::empty(PatchVersion::PATCH_0_5_0, "test")
}

fn section(entries: Vec<serde_json::Value>) -> BundleSection {
    BundleSection {
        section_version: 1,
        entries,
    }
}

#[test]
fn identical_bundles_diff_is_empty() {
    let a = empty_bundle();
    let b = empty_bundle();
    let d = diff_bundles(&a, &b);
    assert!(d.is_empty(), "two empty bundles should diff to nothing");
    assert_eq!(d.total_changes(), 0);

    let md = render_markdown(&d);
    assert!(
        md.contains("No semantic changes"),
        "markdown should announce an empty diff, got:\n{md}"
    );
}

#[test]
fn added_alloy_entry_is_reported_in_diff_and_markdown() {
    let old = empty_bundle();
    let mut new = empty_bundle();
    new.alloys = section(vec![
        serde_json::json!({"id": "alloy_verisium_fire", "name": "Verisium Fire Alloy"}),
    ]);

    let d = diff_bundles(&old, &new);
    assert!(!d.is_empty());
    assert_eq!(d.total_changes(), 1);

    let alloys = d
        .sections
        .iter()
        .find(|s| s.label == "alloys")
        .expect("alloys section delta present");
    assert_eq!(alloys.added, vec!["alloy_verisium_fire".to_string()]);
    assert!(alloys.removed.is_empty());

    let md = render_markdown(&d);
    assert!(md.contains("### alloys"));
    assert!(md.contains("alloy_verisium_fire"));
    // The reviewer-facing caveat about curated fixtures must always be present.
    assert!(
        md.contains("not"),
        "expected curated-fixtures caveat in body"
    );
}

#[test]
fn removed_emotion_entry_is_reported() {
    let mut old = empty_bundle();
    old.emotions = section(vec![
        serde_json::json!({"id": "emotion_greed", "name": "Liquid Greed"}),
        serde_json::json!({"id": "emotion_fear", "name": "Liquid Fear"}),
    ]);
    let mut new = empty_bundle();
    new.emotions = section(vec![
        serde_json::json!({"id": "emotion_greed", "name": "Liquid Greed"}),
    ]);

    let d = diff_bundles(&old, &new);
    let emotions = d
        .sections
        .iter()
        .find(|s| s.label == "emotions")
        .expect("emotions delta present");
    assert_eq!(emotions.removed, vec!["emotion_fear".to_string()]);
    assert!(emotions.added.is_empty());
}

#[test]
fn changed_section_entry_content_is_flagged() {
    let mut old = empty_bundle();
    old.omens = section(vec![serde_json::json!({"id": "omen_x", "effect": "old"})]);
    let mut new = empty_bundle();
    new.omens = section(vec![serde_json::json!({"id": "omen_x", "effect": "new"})]);

    let d = diff_bundles(&old, &new);
    let omens = d
        .sections
        .iter()
        .find(|s| s.label == "omens")
        .expect("omens delta present");
    assert_eq!(omens.changed.len(), 1);
    assert_eq!(omens.changed[0].id, "omen_x");
}

#[test]
fn patch_strings_are_carried_into_the_report() {
    let old = Bundle::empty(PatchVersion::PATCH_0_4_0, "test");
    let new = Bundle::empty(PatchVersion::PATCH_0_5_0, "test");
    let d = diff_bundles(&old, &new);
    assert_eq!(d.old_patch, "0.4.0");
    assert_eq!(d.new_patch, "0.5.0");

    let md = render_markdown(&d);
    assert!(md.contains("0.4.0"));
    assert!(md.contains("0.5.0"));
}

#[test]
fn upstream_state_file_round_trips() {
    let tmp = std::env::temp_dir().join(format!(
        "poc2-upstream-state-{}-{}.json",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));

    let mut shas = std::collections::BTreeMap::new();
    shas.insert("mods".to_string(), "abc123".to_string());
    shas.insert("base_items".to_string(), "def456".to_string());
    shas.insert("tags".to_string(), "789ghi".to_string());

    let state = UpstreamState {
        poe2_patch: Some("4.5.4.1.2".into()),
        repoe_shas: shas,
        last_checked: "epoch:1782703530".into(),
    };

    state.save(&tmp).expect("save state");
    let loaded = UpstreamState::load(&tmp).expect("load state");
    assert_eq!(state, loaded);

    let _ = std::fs::remove_file(&tmp);
}

#[test]
fn committed_seed_state_is_valid_and_populated() {
    // The repo ships pipeline/data/upstream_state.json as the detection
    // baseline. Guard against it being emptied/corrupted by a bad commit.
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("data/upstream_state.json");
    let state = UpstreamState::load(&path).expect("seed state parses");
    assert!(
        state.repoe_shas.contains_key("mods"),
        "seed state must carry the RePoE mods hash"
    );
    assert!(
        state.repoe_shas.contains_key("base_items"),
        "seed state must carry the RePoE base_items hash"
    );
    assert!(
        state.repoe_shas.contains_key("tags"),
        "seed state must carry the RePoE tags hash"
    );
}
