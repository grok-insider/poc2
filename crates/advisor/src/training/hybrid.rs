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

/// Schema version of the trained-model artefact itself — bumped whenever
/// the **featurization semantics** change (independently of the bundle and
/// engine schemas), because a Q-table keyed on packed [`FeatureVec`]s is
/// only meaningful under the featurization it was trained with.
///
/// History:
/// - v1: presence-only `target_match` bitmap; budget included in
///   [`goal_hash`].
/// - v2: count-aware `target_match` (bit `i` = spec `i` FULLY satisfied
///   per `spec_satisfied`); budget dropped from [`goal_hash`].
pub const TRAINED_ARTEFACT_SCHEMA_VERSION: u32 = 2;

/// Serde default for artefacts written before the field existed (= v1),
/// so pre-v2 artefacts deserialize and are then rejected by the loader's
/// version guard.
fn artefact_schema_v1() -> u32 {
    1
}

/// Stable canonical hash of a [`Goal`] keyed on the externally-meaningful
/// fields. Used as the lookup key into [`TrainedModelCache`].
///
/// Two goals with the same `target.prefixes`, `target.suffixes`, and
/// `abandon_criteria` produce identical hashes. The **budget is
/// deliberately excluded**: the trained policy's transition dynamics and
/// terminal predicate don't depend on it, and hashing it made the cache
/// miss on every budget tweak (artefact schema v2). Field ordering inside
/// `Vec<TargetSpec>` matters — callers should canonicalize their target
/// specs before constructing goals if they want round-trip stability
/// across UI sessions.
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
    "::SUF_ABANDON_DIVIDER::".hash(&mut hasher);
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
    /// Artefact schema version (featurization semantics). Loaders refuse
    /// mismatches against [`TRAINED_ARTEFACT_SCHEMA_VERSION`]; artefacts
    /// written before the field existed default to v1 and are refused.
    #[serde(default = "artefact_schema_v1")]
    pub artefact_schema_version: u32,
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
    /// Runtime lookup index over `q_table`, keyed by packed state. Built
    /// by [`Self::build_index`] (the cache builds it on insert); never
    /// serialized. Empty = fall back to the linear scan.
    #[serde(skip)]
    q_index: AHashMap<u64, Vec<(AdvisorAction, f64)>>,
}

impl TrainedModel {
    /// Build the per-state lookup index over `q_table`. Idempotent.
    /// [`TrainedModelCache::insert`] calls this so plan-time lookups are
    /// O(actions-at-state) instead of a full-table linear scan per node.
    pub fn build_index(&mut self) {
        if !self.q_index.is_empty() {
            return;
        }
        let mut index: AHashMap<u64, Vec<(AdvisorAction, f64)>> = AHashMap::new();
        for e in &self.q_table {
            index
                .entry(e.state)
                .or_default()
                .push((e.action.clone(), e.q));
        }
        self.q_index = index;
    }

    /// `Q(state, action)` lookup — via the index when built, else a
    /// linear scan over `q_table` (models constructed directly in tests).
    #[must_use]
    pub fn q_at(&self, state: FeatureVec, action: &AdvisorAction) -> Option<f64> {
        let packed = state.pack();
        if self.q_index.is_empty() {
            return self
                .q_table
                .iter()
                .find(|e| e.state == packed && &e.action == action)
                .map(|e| e.q);
        }
        self.q_index
            .get(&packed)?
            .iter()
            .find(|(a, _)| a == action)
            .map(|(_, q)| *q)
    }
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
    let mut model = TrainedModel {
        goal_hash,
        item_class,
        artefact_schema_version: TRAINED_ARTEFACT_SCHEMA_VERSION,
        bundle_schema_version,
        engine_schema_version,
        q_table,
        value_path_length,
        value_cost,
        reward_kind,
        q_index: AHashMap::new(),
    };
    model.build_index();
    model
}

/// One cache slot: the canonical path-length model plus the optional
/// cost-reward twin used by the risk-slider Q-blend (docs/81 §6.3).
#[derive(Debug, Clone)]
struct TrainedEntry {
    path: TrainedModel,
    cost: Option<TrainedModel>,
}

/// Per-(goal, item-class) cache of trained models. Loaded lazily from
/// the bundle's trained-model artefact.
#[derive(Debug, Clone, Default)]
pub struct TrainedModelCache {
    by_key: AHashMap<(u64, ItemClassId), TrainedEntry>,
}

impl TrainedModelCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a trained (path-length) model into the cache without a cost
    /// twin. The model's lookup index is built on insertion.
    pub fn insert(&mut self, model: TrainedModel) {
        self.insert_pair(model, None);
    }

    /// Insert a path-length model together with its optional cost-reward
    /// twin (both solved from the same transition model). Lookup indexes
    /// are built on insertion.
    pub fn insert_pair(&mut self, mut path: TrainedModel, cost: Option<TrainedModel>) {
        path.build_index();
        let cost = cost.map(|mut m| {
            m.build_index();
            m
        });
        let key = (path.goal_hash, path.item_class.clone());
        self.by_key.insert(key, TrainedEntry { path, cost });
    }

    /// Lookup the canonical (path-length) trained model by goal hash +
    /// item class.
    #[must_use]
    pub fn lookup(&self, goal_hash: u64, item_class: &ItemClassId) -> Option<&TrainedModel> {
        self.by_key
            .get(&(goal_hash, item_class.clone()))
            .map(|e| &e.path)
    }

    /// Lookup both reward models: `(path_length, Option<cost>)`.
    #[must_use]
    pub fn lookup_pair(
        &self,
        goal_hash: u64,
        item_class: &ItemClassId,
    ) -> Option<(&TrainedModel, Option<&TrainedModel>)> {
        self.by_key
            .get(&(goal_hash, item_class.clone()))
            .map(|e| (&e.path, e.cost.as_ref()))
    }

    /// Number of cached models.
    #[must_use]
    pub fn len(&self) -> usize {
        self.by_key.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_key.is_empty()
    }

    /// Drop every cached model. Used when the engine's League ruleset or
    /// plugin content changes (both alter candidate enumeration / goal
    /// semantics, so cached policies may be stale).
    pub fn clear(&mut self) {
        self.by_key.clear();
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
    model.q_at(state, action)
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
    fn goal_hash_ignores_budget() {
        // Artefact schema v2: a budget tweak must NOT re-key the cache —
        // the trained policy's dynamics don't depend on it.
        let a = es_goal();
        let mut b = es_goal();
        b.budget = DivEquiv::point(1.0);
        assert_eq!(goal_hash(&a), goal_hash(&b));
    }

    #[test]
    fn q_at_uses_index_after_cache_insert() {
        let mut cache = TrainedModelCache::new();
        let class = ItemClassId::from("BodyArmour");
        let mut model = TrainedModel {
            goal_hash: 7,
            item_class: class.clone(),
            artefact_schema_version: TRAINED_ARTEFACT_SCHEMA_VERSION,
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
            q_index: AHashMap::new(),
        };
        // Unindexed lookup (linear scan) works…
        assert_eq!(model.q_at(fv(1, 0), &act("ChaosOrb")), Some(-2.5));
        // …and the indexed lookup agrees.
        model.build_index();
        assert_eq!(model.q_at(fv(1, 0), &act("ChaosOrb")), Some(-2.5));
        assert_eq!(model.q_at(fv(1, 0), &act("RegalOrb")), None);
        assert_eq!(model.q_at(fv(2, 0), &act("ChaosOrb")), None);

        cache.insert(model);
        let looked_up = cache.lookup(7, &class).unwrap();
        assert_eq!(looked_up.q_at(fv(1, 0), &act("ChaosOrb")), Some(-2.5));
    }

    #[test]
    fn lookup_pair_returns_cost_twin_when_inserted() {
        let mut cache = TrainedModelCache::new();
        let class = ItemClassId::from("BodyArmour");
        let mk = |reward_kind: RewardKind, q: f64| TrainedModel {
            goal_hash: 9,
            item_class: class.clone(),
            artefact_schema_version: TRAINED_ARTEFACT_SCHEMA_VERSION,
            bundle_schema_version: 1,
            engine_schema_version: 1,
            q_table: vec![QEntry {
                state: fv(1, 0).pack(),
                action: act("ChaosOrb"),
                q,
            }],
            value_path_length: vec![],
            value_cost: vec![],
            reward_kind,
            q_index: AHashMap::new(),
        };
        cache.insert_pair(
            mk(RewardKind::PathLength, -3.0),
            Some(mk(RewardKind::Cost, -12.0)),
        );
        let (path, cost) = cache.lookup_pair(9, &class).unwrap();
        assert_eq!(path.q_at(fv(1, 0), &act("ChaosOrb")), Some(-3.0));
        assert_eq!(cost.unwrap().q_at(fv(1, 0), &act("ChaosOrb")), Some(-12.0));
        // Plain lookup still returns the path model.
        assert!(cache.lookup(9, &class).is_some());
    }

    #[test]
    fn cache_round_trip_preserves_q_values() {
        let mut cache = TrainedModelCache::new();
        let class = ItemClassId::from("BodyArmour");
        let model = TrainedModel {
            goal_hash: 42,
            item_class: class.clone(),
            artefact_schema_version: TRAINED_ARTEFACT_SCHEMA_VERSION,
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
            q_index: AHashMap::new(),
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
            artefact_schema_version: TRAINED_ARTEFACT_SCHEMA_VERSION,
            bundle_schema_version: 1,
            engine_schema_version: 1,
            q_table: vec![],
            value_path_length: vec![],
            value_cost: vec![],
            reward_kind: RewardKind::PathLength,
            q_index: AHashMap::new(),
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
            artefact_schema_version: TRAINED_ARTEFACT_SCHEMA_VERSION,
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
            q_index: AHashMap::new(),
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
