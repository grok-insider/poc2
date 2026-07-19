//! M16.6 — train-advisor binary smoke test.
//!
//! Drives the binary through one tiny goal at low samples to verify the
//! end-to-end pipeline (corpus parse → training → JSON output)
//! compiles, links, and produces a syntactically-valid artefact.
//!
//! Production-scale training (100k samples × 50 goals) is operator-driven
//! — this test only validates the plumbing.
//!
//! Reference: `docs/81-engine-training-and-rule-encoding-plan.md` §6.6
//! Tier 3.6.

use std::path::PathBuf;
use std::process::Command;

fn target_binary() -> PathBuf {
    // CARGO_MANIFEST_DIR is `pipeline/`. The binary lives at
    // `target/debug/train-advisor` relative to the workspace root.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("target")
        .join("debug")
        .join("train-advisor")
}

fn ensure_binary_built() {
    if !target_binary().exists() {
        // First test invocation builds the binary in-place.
        let status = Command::new(env!("CARGO"))
            .args(["build", "-p", "poc2-pipeline", "--bin", "train-advisor"])
            .status()
            .expect("cargo build train-advisor");
        assert!(status.success(), "binary build failed");
    }
}

#[test]
fn train_advisor_runs_on_minimal_corpus_and_produces_json() {
    ensure_binary_built();
    let tmp_corpus = std::env::temp_dir().join("poc2_train_advisor_smoke_corpus.toml");
    let tmp_out = std::env::temp_dir().join("poc2_train_advisor_smoke_out.json");

    // Tiny corpus: one goal, minimal target.
    std::fs::write(
        &tmp_corpus,
        r#"
[[goal]]
id = "smoke-goal-helmet"
display_name = "Smoke Goal Helmet"
item_class = "Helmet"
ilvl = 82
budget_div = 50.0

[[goal.target.prefixes]]
concept = "Life"
count = 1
allow_hybrid = true
"#,
    )
    .unwrap();

    let output = Command::new(target_binary())
        .arg("--corpus")
        .arg(&tmp_corpus)
        .arg("--out")
        .arg(&tmp_out)
        .arg("--samples")
        .arg("50")
        .arg("--max-states")
        .arg("100")
        .output()
        .expect("run train-advisor");
    assert!(
        output.status.success(),
        "train-advisor exited non-zero: stdout={:?}, stderr={:?}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(tmp_out.exists(), "output file should be written");

    let content = std::fs::read_to_string(&tmp_out).unwrap();
    assert!(
        content.starts_with('['),
        "JSON output should start with array bracket; got: {}",
        &content[..content.len().min(80)]
    );
    assert!(
        content.contains("smoke-goal-helmet"),
        "output should mention the goal id"
    );
    assert!(
        content.contains("model_path_length"),
        "output should include path-length model"
    );
    assert!(
        content.contains("model_cost"),
        "output should include cost model"
    );
    assert!(
        content.contains("metrics"),
        "output should include training metrics"
    );

    // Round-trip the artefact through the desktop loader's deserializer
    // to guard against schema drift between the binary's output and the
    // `TrainedModelArtefact` type the loader expects.
    let dir = std::env::temp_dir().join("poc2_train_advisor_smoke_dir");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let dest = dir.join("models.json");
    std::fs::copy(&tmp_out, &dest).unwrap();
    let (cache, loaded, skipped) = poc2_advisor::training::load_cache_from_dir(&dir);
    assert_eq!(skipped, 0, "loader should accept binary output");
    assert_eq!(
        loaded, 1,
        "loader should ingest the single artefact written by the binary"
    );
    assert_eq!(cache.len(), 1, "cache should hold one (goal × class) entry");

    std::fs::remove_dir_all(&dir).ok();
    std::fs::remove_file(&tmp_corpus).ok();
    std::fs::remove_file(&tmp_out).ok();
}

/// Bundle-mode integration smoke: build a tiny synthetic bundle on
/// disk, run `train-advisor --bundle <path>`, and assert that
/// `V_path(s0)` is no longer pinned to the value-iteration floor —
/// i.e., the new bundle wiring + bitmap-full terminal predicate
/// actually produce a learning signal instead of degenerate Q values.
#[test]
fn train_advisor_with_bundle_produces_non_degenerate_v_path() {
    use poc2_engine::base::{BaseType, InventorySize, ReleaseState};
    use poc2_engine::ids::{BaseTypeId, ConceptId, ItemClassId, ModGroupId, ModId, StatId, TagId};
    use poc2_engine::item::AffixType;
    use poc2_engine::item_class::AttributePool;
    use poc2_engine::mods::{
        ModDefinition, ModDomain, ModFlags, ModGroup, ModKind, ModStat, SpawnWeight,
    };
    use poc2_engine::patch::{PatchRange, PatchVersion};
    use poc2_engine::ENGINE_SCHEMA_VERSION;
    use smallvec::smallvec;

    ensure_binary_built();

    // ---- 1. Build a synthetic bundle that can satisfy a `Life` goal ----
    let class = ItemClassId::from("BodyArmour");
    let base = BaseType {
        id: BaseTypeId::from("HeavyPlate"),
        name: "Heavy Plate".into(),
        item_class: class.clone(),
        attribute_pool: AttributePool::Str,
        drop_level: 70,
        tags: smallvec![TagId::from("body_armour"), TagId::from("str_armour")],
        implicits: smallvec![],
        inventory: InventorySize {
            width: 2,
            height: 3,
        },
        release_state: ReleaseState::Released,
        patch_range: PatchRange::ALL,
    };
    let life_mod = ModDefinition {
        id: ModId::from("LifeModSmoke"),
        name: Some("of Life".into()),
        mod_group: ModGroup(ModGroupId::from("LifeGroup")),
        affix_type: AffixType::Prefix,
        kind: ModKind::Explicit,
        domain: ModDomain::Item,
        tags: smallvec![],
        concept_set: smallvec![ConceptId::from("Life")],
        spawn_weights: smallvec![SpawnWeight {
            tag: TagId::from("body_armour"),
            weight: 1000,
        }],
        stats: smallvec![ModStat {
            stat_id: StatId::from("base_maximum_life"),
            min: 50.0,
            max: 80.0,
        }],
        // Perfect orbs (`MIN_LEVEL_PERFECT_ALL = 70`) only roll mods
        // with `required_level >= 70`. The training corpus uses
        // Perfect orbs by default, so the synthetic mod must clear
        // that bar to be eligible.
        required_level: 75,
        tier: None,
        allowed_item_classes: smallvec![class.clone()],
        patch_range: PatchRange::ALL,
        flags: ModFlags::empty(),
        text_template: Some("+{0} to maximum Life".into()),
    };

    let mut bundle = poc2_data::Bundle::empty(PatchVersion::PATCH_0_4_0, "smoke-test");
    bundle.header.schema_version = poc2_data::BUNDLE_SCHEMA_VERSION;
    bundle.header.engine_schema = ENGINE_SCHEMA_VERSION;
    bundle.base_items.push(base);
    bundle.mods.push(life_mod);

    let bundle_path = std::env::temp_dir().join("poc2_train_advisor_smoke_bundle.json");
    poc2_data::io::write_bundle(&bundle, &bundle_path, false).unwrap();

    // ---- 2. Tiny corpus referencing the synthetic Life mod -------------
    let tmp_corpus = std::env::temp_dir().join("poc2_train_advisor_bundle_corpus.toml");
    let tmp_out = std::env::temp_dir().join("poc2_train_advisor_bundle_out.json");
    std::fs::write(
        &tmp_corpus,
        r#"
[[goal]]
id = "smoke-life-bundle"
display_name = "Smoke Life Bundle"
item_class = "BodyArmour"
ilvl = 82
budget_div = 50.0

[[goal.target.prefixes]]
concept = "Life"
count = 1
allow_hybrid = true
"#,
    )
    .unwrap();

    // ---- 3. Run the binary with --bundle -------------------------------
    let output = std::process::Command::new(target_binary())
        .args([
            "--corpus".as_ref(),
            tmp_corpus.as_os_str(),
            "--bundle".as_ref(),
            bundle_path.as_os_str(),
            "--out".as_ref(),
            tmp_out.as_os_str(),
            "--samples".as_ref(),
            "200".as_ref(),
            "--max-states".as_ref(),
            "200".as_ref(),
        ])
        .output()
        .expect("run train-advisor");
    assert!(
        output.status.success(),
        "train-advisor --bundle exited non-zero: stdout={:?}, stderr={:?}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let content = std::fs::read_to_string(&tmp_out).unwrap();

    // ---- 4. Parse the artefact and assert a non-degenerate signal ------
    let artefacts: Vec<poc2_advisor::training::TrainedModelArtefact> =
        serde_json::from_str(&content).expect("artefact must deserialize");
    assert_eq!(artefacts.len(), 1);
    let v_path = artefacts[0].metrics.initial_state_v_path;
    // Value-iteration's default initial-value floor is around -1000 (the
    // V(s) initial estimate for unreachable states). With the new
    // bundle wiring + bitmap-full terminal predicate we should land
    // strictly above that floor — even a few hundred samples on a
    // single-mod bundle should push V_path well into the > -100 range.
    assert!(
        v_path > -999.0,
        "V_path should escape the value-iteration floor with --bundle, got {v_path}"
    );

    std::fs::remove_file(&tmp_corpus).ok();
    std::fs::remove_file(&tmp_out).ok();
    std::fs::remove_file(&bundle_path).ok();
}

/// Schema-mismatch path: a v1-stamped bundle on disk should make the
/// binary fail-fast with a rebuild-instruction error.
#[test]
fn train_advisor_rejects_v1_bundle_with_rebuild_instructions() {
    use poc2_engine::patch::PatchVersion;

    ensure_binary_built();

    let mut bundle = poc2_data::Bundle::empty(PatchVersion::PATCH_0_4_0, "v1-test");
    bundle.header.schema_version = 1;
    let bundle_path = std::env::temp_dir().join("poc2_train_advisor_v1_bundle.json");
    let serialized = serde_json::to_string(&bundle).unwrap();
    std::fs::write(&bundle_path, serialized).unwrap();

    let tmp_corpus = std::env::temp_dir().join("poc2_train_advisor_v1_corpus.toml");
    let tmp_out = std::env::temp_dir().join("poc2_train_advisor_v1_out.json");
    std::fs::write(
        &tmp_corpus,
        r#"
[[goal]]
id = "smoke-v1"
display_name = "Smoke V1"
item_class = "BodyArmour"
ilvl = 82
budget_div = 50.0
[[goal.target.prefixes]]
concept = "Life"
count = 1
"#,
    )
    .unwrap();

    let output = std::process::Command::new(target_binary())
        .args([
            "--corpus".as_ref(),
            tmp_corpus.as_os_str(),
            "--bundle".as_ref(),
            bundle_path.as_os_str(),
            "--out".as_ref(),
            tmp_out.as_os_str(),
            "--samples".as_ref(),
            "10".as_ref(),
        ])
        .output()
        .expect("run train-advisor");
    assert!(
        !output.status.success(),
        "v1 bundle should fail-fast, but exit was 0"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("schema_version=v1"),
        "stderr should mention the actual schema version: {stderr}"
    );
    assert!(
        stderr.contains("Rebuild via"),
        "stderr should include rebuild instructions: {stderr}"
    );

    std::fs::remove_file(&tmp_corpus).ok();
    std::fs::remove_file(&bundle_path).ok();
    std::fs::remove_file(&tmp_out).ok();
}
