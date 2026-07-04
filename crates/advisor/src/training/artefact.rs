//! M16.6 + M16.4 — bridge between the offline `train-advisor` binary
//! output and the live planner's [`TrainedModelCache`].
//!
//! The training binary writes one `*.json` file containing a
//! `Vec<TrainedModelArtefact>`; the desktop loader rehydrates each
//! artefact into [`TrainedModel`]s and inserts them into the
//! cache. Both ends share this module so the on-disk schema is
//! single-sourced.
//!
//! ## On-disk layout
//!
//! ```text
//! ~/.config/poc2/cache/trained_models/
//!   ├─ poc2-trained-models-0.4.0.json   (one file per training run)
//!   └─ poc2-trained-models-0.4.0-aux.json
//! ```
//!
//! Each file deserializes into `Vec<TrainedModelArtefact>`. The
//! [`load_cache_from_dir`] helper scans a directory, loads every
//! recognised file, and merges all artefacts into a single
//! [`TrainedModelCache`]. Files that fail to parse are logged and
//! skipped — the cache remains usable when one artefact is corrupt.

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::hybrid::{TrainedModel, TrainedModelCache};

/// One serialised training artefact: the path-length and cost models
/// for a single goal × item-class, plus diagnostic metrics. Mirrors
/// the binary's local struct so callers can deserialise without
/// duplicating the field list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainedModelArtefact {
    /// Stable identifier for the goal that produced this artefact.
    pub goal_id: String,
    /// Human-readable name (mirrors the corpus' `display_name`).
    pub display_name: String,
    /// Item-class this artefact targets.
    pub item_class: String,
    /// Q-table solved against the path-length reward.
    pub model_path_length: TrainedModel,
    /// Q-table solved against the cost reward.
    pub model_cost: TrainedModel,
    /// Diagnostic metrics from the training run.
    pub metrics: TrainingArtefactMetrics,
}

/// Per-artefact training diagnostics. Surfaced for the UI's "trained
/// model status" panel and for sim-to-real-gap analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingArtefactMetrics {
    pub states_visited: usize,
    pub transitions_learned: usize,
    pub value_iteration_iters_path: u32,
    pub value_iteration_iters_cost: u32,
    pub initial_state_v_path: f64,
    pub initial_state_v_cost: f64,
}

/// Outcome of loading a single artefact file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtefactLoadOutcome {
    /// Artefacts inserted into the cache successfully.
    Loaded(usize),
    /// File didn't parse as `Vec<TrainedModelArtefact>`. Cache left
    /// unchanged. The error message is logged but not returned to the
    /// caller; the cache remains usable.
    Skipped(String),
}

/// Parse a raw artefact JSON payload (`Vec<TrainedModelArtefact>`) and
/// merge its models into `cache`. Filesystem-free — the WASM engine
/// feeds it a fetched static asset, the native loader a file's text.
/// Returns `(inserted, version_skipped)` or a parse error.
pub fn load_artefacts_str(
    raw: &str,
    cache: &mut TrainedModelCache,
) -> Result<(usize, usize), String> {
    let artefacts: Vec<TrainedModelArtefact> = serde_json::from_str(raw)
        .map_err(|e| format!("parse as Vec<TrainedModelArtefact> failed: {e}"))?;
    let mut inserted = 0;
    let mut version_skipped = 0;
    for artefact in artefacts {
        // Version guard: a trained model is only valid for the bundle +
        // engine + artefact (featurization) schema it was trained against.
        // A stale model (e.g. a 0.4-era schema-v2 artefact next to a
        // v3/0.5 bundle, or a presence-bitmap v1 artefact under the
        // count-aware v2 featurization) keys on a goal_hash /
        // featurization that may collide or mis-estimate, so it must be
        // refused — the advisor falls back to heuristic planning, which is
        // always correct. Retraining against the new bundle regenerates a
        // matching artefact.
        let m = &artefact.model_path_length;
        if m.bundle_schema_version != poc2_data::BUNDLE_SCHEMA_VERSION
            || m.engine_schema_version != poc2_engine::ENGINE_SCHEMA_VERSION
            || m.artefact_schema_version != super::hybrid::TRAINED_ARTEFACT_SCHEMA_VERSION
        {
            version_skipped += 1;
            continue;
        }
        // The cache keys on (goal_hash, item_class); the path-length model
        // is the canonical entry and the cost model rides along as the
        // cost-priority side of the risk slider's Q-blend (docs/81 §6.3).
        // A cost model with a mismatched schema is dropped rather than
        // rejected wholesale (the path model alone is still valid).
        let cost = Some(artefact.model_cost).filter(|c| {
            c.bundle_schema_version == poc2_data::BUNDLE_SCHEMA_VERSION
                && c.engine_schema_version == poc2_engine::ENGINE_SCHEMA_VERSION
                && c.artefact_schema_version == super::hybrid::TRAINED_ARTEFACT_SCHEMA_VERSION
        });
        cache.insert_pair(artefact.model_path_length, cost);
        inserted += 1;
    }
    if version_skipped > 0 {
        tracing::warn!(
            version_skipped,
            expected_bundle = poc2_data::BUNDLE_SCHEMA_VERSION,
            expected_engine = poc2_engine::ENGINE_SCHEMA_VERSION,
            expected_artefact = super::hybrid::TRAINED_ARTEFACT_SCHEMA_VERSION,
            "skipped trained-model artefacts trained against a different schema; retrain the corpus"
        );
    }
    Ok((inserted, version_skipped))
}

/// Load one artefact file at `path` and merge its models into `cache`.
/// Returns the number of `(goal × class)` entries inserted, or a
/// human-readable error reason. Either path-length or cost models are
/// inserted depending on whether the cache already has a hit for the
/// same key.
pub fn load_artefact_file(path: &Path, cache: &mut TrainedModelCache) -> ArtefactLoadOutcome {
    let raw = match fs::read_to_string(path) {
        Ok(r) => r,
        Err(e) => {
            return ArtefactLoadOutcome::Skipped(format!("read {} failed: {e}", path.display()))
        }
    };
    match load_artefacts_str(&raw, cache) {
        Ok((inserted, _version_skipped)) => ArtefactLoadOutcome::Loaded(inserted),
        Err(reason) => ArtefactLoadOutcome::Skipped(format!("{}: {reason}", path.display())),
    }
}

/// Scan `dir` for artefact files and merge every one into a single
/// [`TrainedModelCache`].
///
/// Recognised filenames:
/// - `*.json` files anywhere directly under `dir`
///
/// Subdirectories are ignored. Files that fail to deserialise are
/// counted in the returned `(loaded, skipped)` tuple and logged via
/// `tracing::warn!`. Missing/empty directories yield an empty cache
/// without error.
pub fn load_cache_from_dir(dir: &Path) -> (TrainedModelCache, usize, usize) {
    let mut cache = TrainedModelCache::new();
    let mut loaded = 0;
    let mut skipped = 0;
    let Ok(entries) = fs::read_dir(dir) else {
        return (cache, 0, 0);
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        match load_artefact_file(&path, &mut cache) {
            ArtefactLoadOutcome::Loaded(n) => {
                loaded += n;
                tracing::info!(path = %path.display(), inserted = n, "trained-model artefact loaded");
            }
            ArtefactLoadOutcome::Skipped(reason) => {
                skipped += 1;
                tracing::warn!(path = %path.display(), reason, "trained-model artefact skipped");
            }
        }
    }
    (cache, loaded, skipped)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::AdvisorAction;
    use crate::training::hybrid::{QEntry, RewardKind, TrainedModel};
    use poc2_engine::ids::{CurrencyId, ItemClassId};

    fn mk_model(goal_hash: u64, class: &str) -> TrainedModel {
        // Round-trip through serde so the private `q_index` field doesn't
        // need to be nameable here; loaders always deserialize anyway.
        let json = serde_json::json!({
            "goal_hash": goal_hash,
            "item_class": class,
            "artefact_schema_version":
                crate::training::hybrid::TRAINED_ARTEFACT_SCHEMA_VERSION,
            "bundle_schema_version": poc2_data::BUNDLE_SCHEMA_VERSION,
            "engine_schema_version": poc2_engine::ENGINE_SCHEMA_VERSION,
            "q_table": [QEntry {
                state: 0,
                action: AdvisorAction::ApplyCurrency {
                    currency: CurrencyId::from("ChaosOrb"),
                    omens: vec![],
                },
                q: -1.5,
            }],
            "value_path_length": [(0u64, -1.5f64)],
            "value_cost": [],
            "reward_kind": RewardKind::PathLength,
        });
        serde_json::from_value(json).unwrap()
    }

    fn mk_artefact(goal_hash: u64, class: &str) -> TrainedModelArtefact {
        TrainedModelArtefact {
            goal_id: format!("goal-{goal_hash}"),
            display_name: "test goal".into(),
            item_class: class.into(),
            model_path_length: mk_model(goal_hash, class),
            model_cost: mk_model(goal_hash, class),
            metrics: TrainingArtefactMetrics {
                states_visited: 1,
                transitions_learned: 1,
                value_iteration_iters_path: 1,
                value_iteration_iters_cost: 1,
                initial_state_v_path: -1.5,
                initial_state_v_cost: -2.0,
            },
        }
    }

    #[test]
    fn load_artefacts_str_inserts_and_reports_version_skips() {
        // The filesystem-free entry point the WASM engine uses: current-
        // schema models insert, stale-schema models count as skipped.
        let mut stale = mk_artefact(7, "BodyArmour");
        stale.model_path_length.bundle_schema_version =
            poc2_data::BUNDLE_SCHEMA_VERSION.wrapping_sub(1);
        let artefacts = vec![mk_artefact(42, "BodyArmour"), stale];
        let raw = serde_json::to_string(&artefacts).unwrap();

        let mut cache = TrainedModelCache::new();
        let (inserted, version_skipped) = load_artefacts_str(&raw, &mut cache).unwrap();
        assert_eq!(inserted, 1);
        assert_eq!(version_skipped, 1);
        assert!(cache.lookup(42, &ItemClassId::from("BodyArmour")).is_some());

        // Garbage payload → parse error, cache untouched.
        assert!(load_artefacts_str("not json", &mut cache).is_err());
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn load_cache_from_dir_returns_empty_when_dir_missing() {
        let nonexistent = std::path::Path::new("/tmp/poc2-no-such-dir-for-test");
        let (cache, loaded, skipped) = load_cache_from_dir(nonexistent);
        assert!(cache.is_empty());
        assert_eq!(loaded, 0);
        assert_eq!(skipped, 0);
    }

    #[test]
    fn load_artefact_file_inserts_path_length_model() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("models.json");
        let artefacts = vec![mk_artefact(42, "BodyArmour"), mk_artefact(99, "Helmet")];
        std::fs::write(&path, serde_json::to_string(&artefacts).unwrap()).unwrap();

        let mut cache = TrainedModelCache::new();
        let outcome = load_artefact_file(&path, &mut cache);
        assert_eq!(outcome, ArtefactLoadOutcome::Loaded(2));
        assert_eq!(cache.len(), 2);
        assert!(cache.lookup(42, &ItemClassId::from("BodyArmour")).is_some());
        assert!(cache.lookup(99, &ItemClassId::from("Helmet")).is_some());
    }

    #[test]
    fn load_artefact_file_skips_schema_mismatched_models() {
        // A model trained against a stale schema must be refused so the
        // advisor falls back to heuristic planning instead of consuming
        // mis-keyed Q-values (the 0.4-model-vs-0.5-bundle case).
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("stale.json");
        let mut stale = mk_artefact(7, "BodyArmour");
        stale.model_path_length.bundle_schema_version =
            poc2_data::BUNDLE_SCHEMA_VERSION.wrapping_sub(1);
        std::fs::write(&path, serde_json::to_string(&vec![stale]).unwrap()).unwrap();

        let mut cache = TrainedModelCache::new();
        let outcome = load_artefact_file(&path, &mut cache);
        // The file parses (Loaded), but the mismatched model is not inserted.
        assert_eq!(outcome, ArtefactLoadOutcome::Loaded(0));
        assert!(cache.is_empty());
    }

    #[test]
    fn load_artefact_file_skips_engine_schema_mismatch() {
        // The guard rejects a model whose ENGINE schema differs even when the
        // bundle schema matches (the bundle-schema rejection is covered by
        // load_artefact_file_skips_schema_mismatched_models; this covers the
        // other half of the `||` guard).
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("engine_stale.json");
        let mut stale = mk_artefact(8, "BodyArmour");
        stale.model_path_length.engine_schema_version =
            poc2_engine::ENGINE_SCHEMA_VERSION.wrapping_add(1);
        std::fs::write(&path, serde_json::to_string(&vec![stale]).unwrap()).unwrap();

        let mut cache = TrainedModelCache::new();
        let outcome = load_artefact_file(&path, &mut cache);
        assert_eq!(outcome, ArtefactLoadOutcome::Loaded(0));
        assert!(cache.is_empty());
    }

    #[test]
    fn load_artefacts_str_skips_artefact_schema_mismatch() {
        // v1 artefacts (presence-only bitmap era) must be refused under the
        // count-aware v2 featurization — including artefacts written before
        // the field existed (serde default = 1).
        let mut stale = mk_artefact(11, "BodyArmour");
        stale.model_path_length.artefact_schema_version = 1;
        let raw = serde_json::to_string(&vec![stale]).unwrap();
        let mut cache = TrainedModelCache::new();
        let (inserted, version_skipped) = load_artefacts_str(&raw, &mut cache).unwrap();
        assert_eq!(inserted, 0);
        assert_eq!(version_skipped, 1);

        // Field absent entirely → defaults to v1 → refused.
        let mut v = serde_json::to_value(vec![mk_artefact(12, "BodyArmour")]).unwrap();
        v[0]["model_path_length"]
            .as_object_mut()
            .unwrap()
            .remove("artefact_schema_version");
        let raw = serde_json::to_string(&v).unwrap();
        let (inserted, version_skipped) = load_artefacts_str(&raw, &mut cache).unwrap();
        assert_eq!(inserted, 0);
        assert_eq!(version_skipped, 1);
    }

    #[test]
    fn load_artefacts_str_inserts_cost_twin() {
        let raw = serde_json::to_string(&vec![mk_artefact(21, "BodyArmour")]).unwrap();
        let mut cache = TrainedModelCache::new();
        let (inserted, _) = load_artefacts_str(&raw, &mut cache).unwrap();
        assert_eq!(inserted, 1);
        let (path, cost) = cache
            .lookup_pair(21, &poc2_engine::ids::ItemClassId::from("BodyArmour"))
            .expect("pair");
        assert_eq!(path.goal_hash, 21);
        assert!(cost.is_some(), "cost twin should be cached");
    }

    #[test]
    fn load_artefact_file_skips_unparseable_input() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("garbage.json");
        std::fs::write(&path, "not valid json").unwrap();
        let mut cache = TrainedModelCache::new();
        let outcome = load_artefact_file(&path, &mut cache);
        assert!(matches!(outcome, ArtefactLoadOutcome::Skipped(_)));
        assert!(cache.is_empty());
    }

    #[test]
    fn load_cache_from_dir_aggregates_multiple_files_and_skips_non_json() {
        let tmp = tempfile::tempdir().unwrap();
        // First artefact file.
        let p1 = tmp.path().join("a.json");
        std::fs::write(
            &p1,
            serde_json::to_string(&vec![mk_artefact(1, "BodyArmour")]).unwrap(),
        )
        .unwrap();
        // Second artefact file.
        let p2 = tmp.path().join("b.json");
        std::fs::write(
            &p2,
            serde_json::to_string(&vec![mk_artefact(2, "Helmet")]).unwrap(),
        )
        .unwrap();
        // Non-json file — must be ignored.
        std::fs::write(tmp.path().join("readme.txt"), "ignore me").unwrap();
        // Subdirectory — must be ignored.
        std::fs::create_dir(tmp.path().join("nested")).unwrap();

        let (cache, loaded, skipped) = load_cache_from_dir(tmp.path());
        assert_eq!(loaded, 2);
        assert_eq!(skipped, 0);
        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn load_cache_from_dir_counts_skipped_files() {
        let tmp = tempfile::tempdir().unwrap();
        // One good file.
        std::fs::write(
            tmp.path().join("good.json"),
            serde_json::to_string(&vec![mk_artefact(1, "BodyArmour")]).unwrap(),
        )
        .unwrap();
        // Two corrupt JSON files.
        std::fs::write(tmp.path().join("bad1.json"), "{ broken").unwrap();
        std::fs::write(tmp.path().join("bad2.json"), "[1, 2, 3]").unwrap();

        let (cache, loaded, skipped) = load_cache_from_dir(tmp.path());
        assert_eq!(loaded, 1);
        assert_eq!(skipped, 2);
        assert_eq!(cache.len(), 1);
    }
}
