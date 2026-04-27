//! M16.4 — Hybrid planner glue: trained-model cache + Q-driven scoring.
//!
//! Provides the data structures and scoring helpers that let the
//! advisor's planner consult a pre-trained Q-table when one is available,
//! falling back to v2 beam-search scoring otherwise.
//!
//! ## Architecture
//!
//! The trained-model artefact ships with the bundle as a
//! `bincode`-encoded blob containing `Vec<(GoalHash, ItemClassId,
//! TrainedModel)>`. The desktop loader rehydrates it into a
//! [`TrainedModelCache`]; the planner consults it via
//! [`TrainedModelCache::lookup`].
//!
//! When a lookup hits, the planner uses [`score_with_trained_policy`]
//! to assign Q-values to candidate first-actions. Q-values dominate the
//! score; the v2 concept-occupancy adjustment + cost band become
//! tiebreakers.
//!
//! When a lookup misses, the planner runs unchanged.
//!
//! ## Sim-to-real gap detection
//!
//! Trained models can drift from engine reality when:
//! - The bundle's mods or weights changed since training.
//! - A new currency was added to the resolver.
//! - The featurization missed a state signal that materially affects
//!   transitions.
//!
//! [`sim_to_real_gap`] compares the trained model's expected
//! accumulated reward at depth-N against the live MC depth-N rollout
//! result. When the divergence exceeds a configurable threshold
//! (default 50% on absolute value), the planner downgrades the
//! trained-policy weight to 0 for the affected state and falls back to
//! beam search. Logged for offline analysis.
//!
//! ## Goal hashing
//!
//! Two goals with the same target prefixes/suffixes and abandon
//! criteria hash identically. [`goal_hash`] is the canonical-form
//! hash used by [`TrainedModelCache::lookup`].
//!
//! Reference: `docs/81-engine-training-and-rule-encoding-plan.md` §6.4
//! Tier 3.4.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use ahash::AHashMap;
use poc2_engine::ids::ItemClassId;
use serde::{Deserialize, Serialize};

use crate::action::AdvisorAction;
use crate::featurize::FeatureVec;
use crate::goal::Goal;
use crate::training::value_iteration::ValueIterationResult;

/// Stable canonical hash of a [`Goal`] keyed on the externally-meaningful
/// fields. Used as the lookup key into [`TrainedModelCache`].
///
/// Two goals with the same `target.prefixes`, `target.suffixes`,
/// `abandon_criteria`, and `budget` produce identical hashes. Field
/// ordering inside `Vec<TargetSpec>` matters — callers should
/// canonicalize their target specs before constructing goals if they
/// want round-trip stability across UI sessions.
#[must_use]
pub fn goal_hash(goal: &Goal) -> u64 {
    let mut hasher = DefaultHasher::new();
    // Canonical-form hashing: hash each field in fixed order. The Goal
    // type derives Hash through `Target` which derives Hash through
    // `TargetSpec`, but we don't rely on that here — we hash field-by-
    // field so future struct extensions don't accidentally change the
    // hash unless they're meaningful.
    for spec in &goal.target.prefixes {
        spec_hash(spec, &mut hasher);
    }
    "::PRE_SUF_DIVIDER::".hash(&mut hasher);
    for spec in &goal.target.suffixes {
        spec_hash(spec, &mut hasher);
    }
    "::SUF_BUDGET_DIVIDER::".hash(&mut hasher);
    // Hash the budget's serialized form to avoid depending on internal
    // representation of `DivEquiv`. This is rounding-safe because the
    // serialization is deterministic for f64.
    let budget_str = format!("{:?}", goal.budget);
    budget_str.hash(&mut hasher);
    "::BUDGET_ABANDON_DIVIDER::".hash(&mut hasher);
    for predicate in &goal.abandon_criteria {
        let pred_str = format!("{predicate:?}");
        pred_str.hash(&mut hasher);
    }
    hasher.finish()
}

fn spec_hash(spec: &poc2_strategies::TargetSpec, hasher: &mut DefaultHasher) {
    // Canonical-form: concept first (or "_none"), then concept_any in
    // sort order, then affix, count, min_tier, allow_hybrid.
    let concept_str = spec
        .concept
        .as_ref()
        .map_or_else(|| "_none".to_string(), |c| c.as_str().to_string());
    concept_str.hash(hasher);
    let mut any: Vec<&str> = spec
        .concept_any
        .iter()
        .map(poc2_engine::ConceptId::as_str)
        .collect();
    any.sort_unstable();
    for c in any {
        c.hash(hasher);
    }
    format!("{:?}", spec.affix).hash(hasher);
    spec.count.hash(hasher);
    spec.min_tier.hash(hasher);
    spec.allow_hybrid.hash(hasher);
}

/// One trained-model artefact: per-(goal-hash × item-class) Q-table
/// covering the goal's reachable feature-vector graph.
///
/// Carries metadata so loaders can reject mismatched bundles without
/// running degraded inference.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainedModel {
    /// `goal_hash` this model was trained against.
    pub goal_hash: u64,
    /// Item class the model targets. The advisor may have trained models
    /// for multiple classes per goal; lookup keys on both.
    pub item_class: ItemClassId,
    /// Bundle schema version the model was trained against. Loaders
    /// refuse mismatched versions and trigger retraining.
    pub bundle_schema_version: u32,
    /// Engine schema version (mirrors [`poc2_engine::ENGINE_SCHEMA_VERSION`]).
    pub engine_schema_version: u32,
    /// `Q(s, a)` keyed by packed `FeatureVec` (`u64`) and the original
    /// action. Packed because `FeatureVec` doesn't impl `Serialize`
    /// itself; the `pack`/`unpack` round-trip is the canonical
    /// serialization path.
    pub q_table: Vec<QEntry>,
    /// Path-length value function `V(s)` for each reachable state.
    /// Useful for sim-to-real-gap detection (compare model V vs MC
    /// rollout depth-N reward).
    pub value_path_length: Vec<(u64, f64)>,
    /// Cost-reward value function `V(s)` for each reachable state.
    /// Used by the cost-priority side of the user's risk slider.
    pub value_cost: Vec<(u64, f64)>,
    /// Reward function the Q-table was solved for. The hybrid scorer
    /// uses this to know which side of the risk slider this model
    /// covers.
    pub reward_kind: RewardKind,
}

/// Single Q-table entry: (state, action, value).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QEntry {
    /// Packed `FeatureVec`; unpack via TODO-bit-decomposition (the
    /// hybrid scorer keys on the packed value directly so this is
    /// fine for v3).
    pub state: u64,
    /// Serialized action (uses `serde_json` for human-readable
    /// debuggability of trained models).
    pub action: AdvisorAction,
    /// `Q(s, a)` value.
    pub q: f64,
}

/// Reward function the Q-table was solved against.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RewardKind {
    /// `R(s, a) = -1` per non-terminal step.
    PathLength,
    /// `R(s, a) = -cost(a)` per non-terminal step.
    Cost,
}

/// Build a [`TrainedModel`] from a [`ValueIterationResult`].
///
/// Used by the offline training binary (M16.6) to package the solver
/// output into the bundle-shippable format.
#[must_use]
pub fn trained_model_from(
    goal_hash: u64,
    item_class: ItemClassId,
    bundle_schema_version: u32,
    engine_schema_version: u32,
    reward_kind: RewardKind,
    result_path_length: &ValueIterationResult,
    result_cost: Option<&ValueIterationResult>,
) -> TrainedModel {
    let q_table: Vec<QEntry> = result_path_length
        .q
        .iter()
        .map(|((state, action), q)| QEntry {
            state: state.pack(),
            action: action.clone(),
            q: *q,
        })
        .collect();
    let value_path_length: Vec<(u64, f64)> = result_path_length
        .value
        .iter()
        .map(|(s, v)| (s.pack(), *v))
        .collect();
    let value_cost: Vec<(u64, f64)> = result_cost
        .map(|r| r.value.iter().map(|(s, v)| (s.pack(), *v)).collect())
        .unwrap_or_default();
    TrainedModel {
        goal_hash,
        item_class,
        bundle_schema_version,
        engine_schema_version,
        q_table,
        value_path_length,
        value_cost,
        reward_kind,
    }
}

/// Per-(goal, item-class) cache of trained models. Loaded lazily from
/// the bundle's trained-model artefact.
#[derive(Debug, Clone, Default)]
pub struct TrainedModelCache {
    by_key: AHashMap<(u64, ItemClassId), TrainedModel>,
}

impl TrainedModelCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a trained model into the cache.
    pub fn insert(&mut self, model: TrainedModel) {
        let key = (model.goal_hash, model.item_class.clone());
        self.by_key.insert(key, model);
    }

    /// Lookup a trained model by goal hash + item class.
    #[must_use]
    pub fn lookup(&self, goal_hash: u64, item_class: &ItemClassId) -> Option<&TrainedModel> {
        self.by_key.get(&(goal_hash, item_class.clone()))
    }

    /// Number of cached models.
    #[must_use]
    pub fn len(&self) -> usize {
        self.by_key.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_key.is_empty()
    }
}

/// Score a candidate action against the trained policy.
///
/// Returns the model's `Q(state, action)` value when the model has the
/// pair, or `None` when it doesn't. The advisor uses `None` to fall
/// back to v2 heuristic scoring.
#[must_use]
pub fn score_with_trained_policy(
    model: &TrainedModel,
    state: FeatureVec,
    action: &AdvisorAction,
) -> Option<f64> {
    let packed = state.pack();
    model
        .q_table
        .iter()
        .find(|e| e.state == packed && &e.action == action)
        .map(|e| e.q)
}

/// Detect simulator-vs-trained-model drift.
///
/// Compares the model's expected accumulated reward (`sum of
/// per-step rewards` along the model's argmax-Q rollout) against the
/// supplied MC-rollout reward (the live engine simulator's depth-N
/// observed reward). When the divergence exceeds `threshold_relative`
/// (default 0.5 = 50% relative error), returns
/// [`SimToRealVerdict::DowngradeToBeamSearch`] so the planner falls
/// back. Otherwise returns [`SimToRealVerdict::Trust`].
#[must_use]
pub fn sim_to_real_gap(
    model_expected_reward: f64,
    mc_observed_reward: f64,
    threshold_relative: f64,
) -> SimToRealVerdict {
    if !model_expected_reward.is_finite() || !mc_observed_reward.is_finite() {
        return SimToRealVerdict::DowngradeToBeamSearch;
    }
    let denom = model_expected_reward.abs().max(1e-9);
    let rel = (model_expected_reward - mc_observed_reward).abs() / denom;
    if rel > threshold_relative {
        SimToRealVerdict::DowngradeToBeamSearch
    } else {
        SimToRealVerdict::Trust
    }
}

/// Output of [`sim_to_real_gap`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimToRealVerdict {
    /// Trained policy and engine simulator agree; trust the policy.
    Trust,
    /// Divergence exceeds threshold; fall back to beam search for this
    /// state. Logged at the call site for offline analysis.
    DowngradeToBeamSearch,
}

#[cfg(test)]
mod tests {
    use super::*;
    use poc2_engine::ids::{ConceptId, CurrencyId};
    use poc2_market::DivEquiv;
    use poc2_strategies::{Target, TargetSpec};

    fn fv(rarity: u8, target_match: u16) -> FeatureVec {
        FeatureVec {
            rarity,
            target_match,
            n_prefixes: 0,
            n_suffixes: 0,
            has_hidden_desecrated: false,
            has_fractured: false,
            is_corrupted: false,
            has_hinekora_lock: false,
            extra_flags: 0,
        }
    }

    fn act(id: &str) -> AdvisorAction {
        AdvisorAction::ApplyCurrency {
            currency: CurrencyId::from(id),
            omens: vec![],
        }
    }

    fn es_goal() -> Goal {
        Goal::new(
            Target {
                prefixes: vec![TargetSpec {
                    concept: Some(ConceptId::from("EnergyShield")),
                    concept_any: vec![],
                    affix: None,
                    count: 1,
                    min_tier: None,
                    allow_hybrid: true,
                }],
                suffixes: vec![],
                constraints: vec![],
            },
            DivEquiv::point(100.0),
        )
    }

    #[test]
    fn goal_hash_is_stable_for_identical_goals() {
        let a = es_goal();
        let b = es_goal();
        assert_eq!(goal_hash(&a), goal_hash(&b));
    }

    #[test]
    fn goal_hash_differs_when_concept_differs() {
        let a = es_goal();
        let b = Goal::new(
            Target {
                prefixes: vec![TargetSpec {
                    concept: Some(ConceptId::from("Life")),
                    concept_any: vec![],
                    affix: None,
                    count: 1,
                    min_tier: None,
                    allow_hybrid: true,
                }],
                suffixes: vec![],
                constraints: vec![],
            },
            DivEquiv::point(100.0),
        );
        assert_ne!(goal_hash(&a), goal_hash(&b));
    }

    #[test]
    fn goal_hash_differs_when_count_differs() {
        let a = es_goal();
        let mut b = es_goal();
        b.target.prefixes[0].count = 3;
        assert_ne!(goal_hash(&a), goal_hash(&b));
    }

    #[test]
    fn cache_round_trip_preserves_q_values() {
        let mut cache = TrainedModelCache::new();
        let class = ItemClassId::from("BodyArmour");
        let model = TrainedModel {
            goal_hash: 42,
            item_class: class.clone(),
            bundle_schema_version: 1,
            engine_schema_version: 1,
            q_table: vec![
                QEntry {
                    state: fv(1, 0).pack(),
                    action: act("ChaosOrb"),
                    q: -2.5,
                },
                QEntry {
                    state: fv(1, 0).pack(),
                    action: act("RegalOrb"),
                    q: -3.0,
                },
            ],
            value_path_length: vec![(fv(1, 0).pack(), -2.5)],
            value_cost: vec![],
            reward_kind: RewardKind::PathLength,
        };
        cache.insert(model);
        assert_eq!(cache.len(), 1);
        let looked_up = cache.lookup(42, &class).expect("model should exist");
        assert_eq!(looked_up.q_table.len(), 2);
        let q_chaos = score_with_trained_policy(looked_up, fv(1, 0), &act("ChaosOrb")).unwrap();
        let q_regal = score_with_trained_policy(looked_up, fv(1, 0), &act("RegalOrb")).unwrap();
        assert!((q_chaos - (-2.5)).abs() < 1e-9);
        assert!((q_regal - (-3.0)).abs() < 1e-9);
    }

    #[test]
    fn lookup_misses_on_unknown_goal_or_class() {
        let mut cache = TrainedModelCache::new();
        cache.insert(TrainedModel {
            goal_hash: 42,
            item_class: ItemClassId::from("BodyArmour"),
            bundle_schema_version: 1,
            engine_schema_version: 1,
            q_table: vec![],
            value_path_length: vec![],
            value_cost: vec![],
            reward_kind: RewardKind::PathLength,
        });
        assert!(cache.lookup(42, &ItemClassId::from("BodyArmour")).is_some());
        assert!(cache.lookup(99, &ItemClassId::from("BodyArmour")).is_none());
        assert!(cache.lookup(42, &ItemClassId::from("Helmet")).is_none());
    }

    #[test]
    fn score_with_trained_policy_returns_none_on_unknown_pair() {
        let model = TrainedModel {
            goal_hash: 0,
            item_class: ItemClassId::from("BodyArmour"),
            bundle_schema_version: 1,
            engine_schema_version: 1,
            q_table: vec![QEntry {
                state: fv(1, 0).pack(),
                action: act("ChaosOrb"),
                q: -2.5,
            }],
            value_path_length: vec![],
            value_cost: vec![],
            reward_kind: RewardKind::PathLength,
        };
        assert!(score_with_trained_policy(&model, fv(2, 0), &act("ChaosOrb")).is_none());
        assert!(score_with_trained_policy(&model, fv(1, 0), &act("RegalOrb")).is_none());
    }

    #[test]
    fn sim_to_real_gap_trusts_close_predictions() {
        // Model says expected reward = -10; live MC says -10.5. ~5% off.
        let v = sim_to_real_gap(-10.0, -10.5, 0.5);
        assert_eq!(v, SimToRealVerdict::Trust);
    }

    #[test]
    fn sim_to_real_gap_downgrades_on_large_drift() {
        // Model says expected reward = -2; live MC says -10. 4× off.
        let v = sim_to_real_gap(-2.0, -10.0, 0.5);
        assert_eq!(v, SimToRealVerdict::DowngradeToBeamSearch);
    }

    #[test]
    fn sim_to_real_gap_downgrades_on_nan_or_infinity() {
        let v = sim_to_real_gap(f64::NAN, -10.0, 0.5);
        assert_eq!(v, SimToRealVerdict::DowngradeToBeamSearch);
        let v = sim_to_real_gap(-10.0, f64::INFINITY, 0.5);
        assert_eq!(v, SimToRealVerdict::DowngradeToBeamSearch);
    }
}
