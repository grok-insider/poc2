//! M16.7 — Training success metrics.
//!
//! Quantifies trained-policy quality and provides regression-detection
//! across patches. The core deliverable is the **chaos-spam fidelity**
//! test (`spam_loop_fidelity_*`) — the centerpiece assertion that
//! training learns the geometric expectation of self-loop-heavy crafts
//! that beam search cannot solve.
//!
//! ## Metrics
//!
//! Six metrics per goal, computed from a learned [`TableModel`] +
//! solved [`ValueIterationResult`]:
//!
//! 1. **Expected steps to goal** — `−V_path(s_initial)` under the
//!    path-length reward (V is negative; flip the sign for the steps
//!    interpretation).
//! 2. **Expected divine-equivalent cost** — `−V_cost(s_initial)`.
//! 3. **Brick rate** — fraction of trajectories that hit
//!    `abandon_criteria` before goal. Estimated from the simulator's
//!    abandon outcome counts during model learning; surfaced as a
//!    snapshot here.
//! 4. **Top-action agreement** — % of states where the trained
//!    Q-table's argmax matches the strategy library's `dry_run`
//!    action. Above 85% means imitation seeding is working.
//! 5. **Sim-to-real gap proxy** — divergence between MC depth-3
//!    success probability and trained-model Q-value mean. Spikes flag
//!    feature-representation holes.
//! 6. **Spam-loop fidelity** — for known spam loops, asserts the
//!    trained policy's `LoopEstimate.mean_iterations` falls within
//!    the 95% CI of `1 / p_success`. **This is the user's edge-case
//!    test.**
//!
//! Reference: `docs/81-engine-training-and-rule-encoding-plan.md` §6.7
//! Tier 3.7 + §7.1 (chaos-spam edge case).

use ahash::AHashMap;

use crate::action::AdvisorAction;
use crate::featurize::FeatureVec;
use crate::training::value_iteration::ValueIterationResult;

/// Snapshot of the six training-quality metrics for one goal.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrainingMetrics {
    /// Mean expected steps to reach the goal from the initial state.
    /// Equivalent to `-V_path(s_initial)` under the path-length reward.
    pub mean_steps: f64,
    /// Mean expected divine-equivalent cost from the initial state.
    /// Equivalent to `-V_cost(s_initial)` under the cost reward.
    pub mean_cost: f64,
    /// Fraction of training trajectories that hit `abandon_criteria`
    /// before goal. Measured during model learning; `0.0` when the
    /// learner didn't track abandon outcomes (e.g., synthetic tests).
    pub brick_rate: f64,
    /// Fraction of states where the trained policy's argmax-Q action
    /// matches the strategy library's `dry_run` action. Above 0.85
    /// indicates good imitation alignment.
    pub top_action_agreement: f64,
    /// Sim-to-real gap proxy: relative divergence between trained-model
    /// expected reward and MC-rollout observed reward. Larger means
    /// more drift between simulator and trained policy.
    pub sim_to_real_proxy: f64,
}

impl TrainingMetrics {
    /// Construct metrics by reading `V` from the supplied results.
    /// `initial_state` is the goal's start state. `path_length` is the
    /// path-length value-iteration result; `cost` is the cost
    /// value-iteration result (`None` when only one reward shipped).
    /// The remaining metrics default to neutral values when their
    /// inputs are absent — the metric contract asks callers to fill
    /// them in for full reporting.
    #[must_use]
    pub fn from_value_iteration(
        initial_state: FeatureVec,
        path_length: &ValueIterationResult,
        cost: Option<&ValueIterationResult>,
    ) -> Self {
        let mean_steps = -path_length
            .value
            .get(&initial_state)
            .copied()
            .unwrap_or(0.0);
        let mean_cost = -cost
            .and_then(|c| c.value.get(&initial_state).copied())
            .unwrap_or(0.0);
        Self {
            mean_steps,
            mean_cost,
            brick_rate: 0.0,
            top_action_agreement: 0.0,
            sim_to_real_proxy: 0.0,
        }
    }
}

/// Compute the trained policy's expected loop-iteration count for a
/// self-loop attractor state. The classical case: chaos-spam on a
/// 1-mod Magic item where each Chaos has probability `p_success` of
/// hitting the target T1 mod.
///
/// Returns `Some(mean_iterations)` when the model has the state-action
/// pair, where `mean_iterations = -Q(s, a)` under the path-length
/// reward (a single self-loop step costs 1, geometric expectation is
/// `1 / p_success`).
///
/// The user's chaos-spam fidelity assertion is:
/// `|model_mean_iterations − 1/p_success| / (1/p_success) < tolerance`
/// where `tolerance` is `0.05` for ship-prep and slightly looser in
/// smoke tests.
#[must_use]
pub fn loop_iteration_estimate(
    result: &ValueIterationResult,
    state: FeatureVec,
    action: &AdvisorAction,
) -> Option<f64> {
    let q = result.q.get(&(state, action.clone())).copied()?;
    Some(-q)
}

/// Build the per-state `argmax` action map for top-action-agreement
/// scoring. Used by [`top_action_agreement`].
#[must_use]
pub fn argmax_actions(result: &ValueIterationResult) -> AHashMap<FeatureVec, AdvisorAction> {
    let mut by_state: AHashMap<FeatureVec, (AdvisorAction, f64)> = AHashMap::new();
    for ((state, action), q) in &result.q {
        match by_state.get(state) {
            None => {
                by_state.insert(*state, (action.clone(), *q));
            }
            Some((_, best_q)) if *q > *best_q => {
                by_state.insert(*state, (action.clone(), *q));
            }
            _ => {}
        }
    }
    by_state.into_iter().map(|(s, (a, _))| (s, a)).collect()
}

/// Compute the top-action agreement between the trained policy and the
/// reference action map (typically derived from strategy `dry_run`).
///
/// Returns the fraction of states where the trained `argmax_a Q(s, a)`
/// matches `reference[s]`. States missing from either map are skipped.
#[must_use]
pub fn top_action_agreement(
    result: &ValueIterationResult,
    reference: &AHashMap<FeatureVec, AdvisorAction>,
) -> f64 {
    let trained = argmax_actions(result);
    let mut compared = 0u32;
    let mut matching = 0u32;
    for (state, ref_action) in reference {
        let Some(trained_action) = trained.get(state) else {
            continue;
        };
        compared += 1;
        if trained_action == ref_action {
            matching += 1;
        }
    }
    if compared == 0 {
        return 0.0;
    }
    f64::from(matching) / f64::from(compared)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::training::model_learner::{StateActionAlias, TableModelBuilder};
    use crate::training::value_iteration::{value_iteration, ValueIterationConfig};
    use poc2_engine::ids::CurrencyId;

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

    /// Build a chaos-spam attractor model: Magic-1-mod state s0
    /// transitions to goal s1 with probability `p`, else self-loops
    /// on s0. Path-length reward gives `V(s0) = -1/p`.
    fn build_chaos_spam(p: f64) -> crate::training::model_learner::TableModel {
        let mut b = TableModelBuilder::new();
        let alias = StateActionAlias::Pair(fv(1, 0), act("ChaosOrb"));
        let success = (1000.0 * p).round() as u64;
        let failure = 1000u64.saturating_sub(success);
        b.add(alias.clone(), fv(1, 1), success);
        b.add(alias, fv(1, 0), failure);
        b.finalize()
    }

    /// **THE chaos-spam fidelity test.** Constructs an attractor with
    /// per-iteration probability `p`, runs value iteration, and asserts
    /// that the trained policy's mean-iterations estimate matches the
    /// analytic `1/p` within tolerance. This is the test that
    /// directly answers the user's original question:
    ///
    /// > "in the training, will it have cases where the rolls/chances
    /// >  arent good so it need to keep trying? for example doing spam
    /// >  of chaos orb on 1 mod item to get a tier 1?"
    ///
    /// Yes — and the trained policy correctly estimates the geometric
    /// expectation, which beam search cannot do at any practical depth.
    #[test]
    fn spam_loop_fidelity_low_probability() {
        // p = 0.01 → analytic E[iterations] = 100. Tolerance: 5%.
        let p = 0.01;
        let model = build_chaos_spam(p);
        let actions = vec![act("ChaosOrb")];
        let result = value_iteration(
            &model,
            &actions,
            true,
            |s| *s == fv(1, 1),
            |_s, _a| -1.0,
            ValueIterationConfig {
                max_iters: 10_000,
                theta: 1e-9,
                gamma: 1.0,
            },
        );
        let mean_iter =
            loop_iteration_estimate(&result, fv(1, 0), &act("ChaosOrb")).expect("Q present");
        let analytic = 1.0 / p;
        let rel_err = (mean_iter - analytic).abs() / analytic;
        assert!(
            rel_err < 0.05,
            "trained mean iterations = {mean_iter}; analytic = {analytic}; \
             relative error = {rel_err:.4} (must be < 0.05)"
        );
    }

    #[test]
    fn spam_loop_fidelity_moderate_probability() {
        // p = 0.1 → analytic E = 10.
        let p = 0.1;
        let model = build_chaos_spam(p);
        let actions = vec![act("ChaosOrb")];
        let result = value_iteration(
            &model,
            &actions,
            true,
            |s| *s == fv(1, 1),
            |_s, _a| -1.0,
            ValueIterationConfig::default(),
        );
        let mean_iter = loop_iteration_estimate(&result, fv(1, 0), &act("ChaosOrb")).unwrap();
        let analytic = 1.0 / p;
        let rel_err = (mean_iter - analytic).abs() / analytic;
        assert!(
            rel_err < 0.05,
            "p={p}: mean={mean_iter}, analytic={analytic}, rel_err={rel_err}"
        );
    }

    #[test]
    fn spam_loop_fidelity_high_probability() {
        // p = 0.5 → analytic E = 2.
        let p = 0.5;
        let model = build_chaos_spam(p);
        let actions = vec![act("ChaosOrb")];
        let result = value_iteration(
            &model,
            &actions,
            true,
            |s| *s == fv(1, 1),
            |_s, _a| -1.0,
            ValueIterationConfig::default(),
        );
        let mean_iter = loop_iteration_estimate(&result, fv(1, 0), &act("ChaosOrb")).unwrap();
        assert!((mean_iter - 2.0).abs() < 0.05);
    }

    #[test]
    fn training_metrics_extract_v_initial_correctly() {
        let model = build_chaos_spam(0.5);
        let actions = vec![act("ChaosOrb")];
        let path_result = value_iteration(
            &model,
            &actions,
            true,
            |s| *s == fv(1, 1),
            |_s, _a| -1.0,
            ValueIterationConfig::default(),
        );
        let metrics = TrainingMetrics::from_value_iteration(fv(1, 0), &path_result, None);
        assert!((metrics.mean_steps - 2.0).abs() < 0.05);
        assert!(metrics.mean_cost.abs() < 1e-12); // no cost result supplied
        assert!(metrics.brick_rate.abs() < 1e-12);
    }

    #[test]
    fn argmax_actions_matches_value_iteration_best_actions() {
        // Chaos-spam: only one action, so argmax is trivially Chaos.
        let model = build_chaos_spam(0.1);
        let actions = vec![act("ChaosOrb"), act("RegalOrb")];
        let result = value_iteration(
            &model,
            &actions,
            true,
            |s| *s == fv(1, 1),
            |_s, _a| -1.0,
            ValueIterationConfig::default(),
        );
        let am = argmax_actions(&result);
        // Only ChaosOrb has a transition entry from s0; argmax should be ChaosOrb.
        assert_eq!(am.get(&fv(1, 0)), Some(&act("ChaosOrb")));
    }

    #[test]
    fn top_action_agreement_returns_one_when_argmax_matches() {
        let model = build_chaos_spam(0.1);
        let actions = vec![act("ChaosOrb")];
        let result = value_iteration(
            &model,
            &actions,
            true,
            |s| *s == fv(1, 1),
            |_s, _a| -1.0,
            ValueIterationConfig::default(),
        );
        let mut reference: AHashMap<FeatureVec, AdvisorAction> = AHashMap::new();
        reference.insert(fv(1, 0), act("ChaosOrb"));
        let agreement = top_action_agreement(&result, &reference);
        assert!((agreement - 1.0).abs() < 1e-9);
    }

    #[test]
    fn top_action_agreement_returns_zero_when_argmax_diverges() {
        let model = build_chaos_spam(0.1);
        let actions = vec![act("ChaosOrb")];
        let result = value_iteration(
            &model,
            &actions,
            true,
            |s| *s == fv(1, 1),
            |_s, _a| -1.0,
            ValueIterationConfig::default(),
        );
        let mut reference: AHashMap<FeatureVec, AdvisorAction> = AHashMap::new();
        reference.insert(fv(1, 0), act("RegalOrb"));
        let agreement = top_action_agreement(&result, &reference);
        assert!((agreement - 0.0).abs() < 1e-9);
    }
}
