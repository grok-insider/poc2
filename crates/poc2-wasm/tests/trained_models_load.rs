//! Engine boundary test for trained-model loading (M16.4 wiring):
//! `loadTrainedModels` merges current-schema artefacts, refuses stale
//! ones, and the diagnostic count reflects what the planner consults.

use poc2_advisor::action::AdvisorAction;
use poc2_advisor::training::{
    QEntry, RewardKind, TrainedModel, TrainedModelArtefact, TrainingArtefactMetrics,
};
use poc2_engine::ids::{CurrencyId, ItemClassId};
use poc2_engine::patch::PatchVersion;
use poc2_wasm::Engine;

fn engine_with_empty_bundle() -> Engine {
    let bundle = poc2_data::Bundle::empty(PatchVersion::PATCH_0_5_0, "trained-models-test");
    let bytes = serde_json::to_vec(&bundle).expect("serialize bundle");
    Engine::new(&bytes).expect("engine boots from an empty bundle")
}

fn mk_model(goal_hash: u64, class: &str, bundle_schema: u32) -> TrainedModel {
    TrainedModel {
        goal_hash,
        item_class: ItemClassId::from(class),
        bundle_schema_version: bundle_schema,
        engine_schema_version: poc2_engine::ENGINE_SCHEMA_VERSION,
        q_table: vec![QEntry {
            state: 0,
            action: AdvisorAction::ApplyCurrency {
                currency: CurrencyId::from("ChaosOrb"),
                omens: vec![],
            },
            q: -1.5,
        }],
        value_path_length: vec![(0, -1.5)],
        value_cost: vec![],
        reward_kind: RewardKind::PathLength,
    }
}

fn mk_artefact(goal_hash: u64, class: &str, bundle_schema: u32) -> TrainedModelArtefact {
    TrainedModelArtefact {
        goal_id: format!("goal-{goal_hash}"),
        display_name: "test goal".into(),
        item_class: class.into(),
        model_path_length: mk_model(goal_hash, class, bundle_schema),
        model_cost: mk_model(goal_hash, class, bundle_schema),
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
fn load_trained_models_merges_and_counts() {
    let mut engine = engine_with_empty_bundle();
    assert_eq!(engine.trained_model_count(), 0);

    let batch1 = serde_json::to_string(&vec![
        mk_artefact(42, "BodyArmour", poc2_data::BUNDLE_SCHEMA_VERSION),
        mk_artefact(7, "Helmet", poc2_data::BUNDLE_SCHEMA_VERSION),
    ])
    .unwrap();
    let view: serde_json::Value =
        serde_json::from_str(&engine.load_trained_models(&batch1).unwrap()).unwrap();
    assert_eq!(view["loaded"], 2);
    assert_eq!(view["version_skipped"], 0);
    assert_eq!(engine.trained_model_count(), 2);

    // A second file merges (the loader is additive across artefact files).
    let batch2 = serde_json::to_string(&vec![mk_artefact(
        99,
        "Ring",
        poc2_data::BUNDLE_SCHEMA_VERSION,
    )])
    .unwrap();
    engine.load_trained_models(&batch2).unwrap();
    assert_eq!(engine.trained_model_count(), 3);
}

#[test]
fn stale_schema_artefacts_leave_planning_heuristic() {
    let mut engine = engine_with_empty_bundle();
    let stale = serde_json::to_string(&vec![mk_artefact(
        1,
        "BodyArmour",
        poc2_data::BUNDLE_SCHEMA_VERSION.wrapping_sub(1),
    )])
    .unwrap();
    let view: serde_json::Value =
        serde_json::from_str(&engine.load_trained_models(&stale).unwrap()).unwrap();
    assert_eq!(view["loaded"], 0);
    assert_eq!(view["version_skipped"], 1);
    assert_eq!(engine.trained_model_count(), 0);
}

// NOTE: the garbage-payload → JsError path can't run natively
// (wasm-bindgen imported functions panic off-wasm); the underlying
// parse-error behaviour is covered by
// `poc2_advisor::training::artefact` unit tests.
