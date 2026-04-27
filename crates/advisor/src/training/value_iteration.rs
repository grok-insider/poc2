//! M16.3 — Q-value iteration solver.
//!
//! Solves the Bellman equation over a learned [`TableModel`] to produce
//! a Q-table:
//!
//! ```text
//! Q(s, a) = R(s, a) + γ × Σ_{s'} P(s' | s, a) × V(s')
//! V(s)    = max_a Q(s, a)
//! ```
//!
//! Iterates until `||V_new − V_old||_∞ < theta` or `max_iters` is hit.
//! For episodic MDPs at γ=1 (the v3 default), convergence typically
//! finishes in ≤ 50 iterations.
//!
//! ## Reward functions
//!
//! Two reward functions ship in v3:
//! - **Path-length**: `R(s, a) = -1` for non-terminal steps, `R(s_goal, _) = 0`.
//!   Minimizes expected number of steps to goal.
//! - **Cost**: `R(s, a) = -cost(a)`, `R(s_goal, _) = 0`. Minimizes
//!   expected divine-equivalent total spend.
//!
//! The user's risk slider blends the two Q-tables at advisor query time;
//! both are computed and shipped per-goal.
//!
//! ## Self-loop handling
//!
//! Cyclic state graphs (chaos-spam attractors, annul-chaos cycles) work
//! naturally: the Bellman fixed-point includes the self-loop term, and
//! `Q(s, a)` resolves to the geometric expectation `-1 / p_success` for
//! a path-length reward. This is the entire reason the trained policy
//! beats beam search on long-tail crafts (see plan §7.1).
//!
//! Reference: `docs/81-engine-training-and-rule-encoding-plan.md` §6.3
//! Tier 3.3.

use ahash::AHashMap;

use crate::action::AdvisorAction;
use crate::featurize::FeatureVec;
use crate::training::model_learner::{StateActionAlias, TableModel};

/// Knobs for [`value_iteration`].
#[derive(Debug, Clone, Copy)]
pub struct ValueIterationConfig {
    /// Discount factor `γ`. Use `1.0` for episodic MDPs (the v3 default).
    pub gamma: f64,
    /// Convergence threshold for the infinity norm of the value-fn delta.
    /// Iteration stops when `max_s |V_new(s) − V_old(s)| < theta`.
    pub theta: f64,
    /// Hard cap on iterations to prevent runaway in pathological models.
    pub max_iters: u32,
}

impl Default for ValueIterationConfig {
    fn default() -> Self {
        Self {
            gamma: 1.0,
            theta: 1e-6,
            max_iters: 1000,
        }
    }
}

/// Output of [`value_iteration`].
#[derive(Debug, Clone)]
pub struct ValueIterationResult {
    /// `V(s)` for each reachable state.
    pub value: AHashMap<FeatureVec, f64>,
    /// `Q(s, a)` for each `(state, action)` pair the model has data for.
    /// Keyed by the original (unaliased) action so callers can read
    /// trained-policy values directly without alias-aware indirection.
    pub q: AHashMap<(FeatureVec, AdvisorAction), f64>,
    /// Number of iterations actually run.
    pub iterations: u32,
    /// Final infinity-norm delta. Below `theta` indicates true convergence;
    /// at-or-above `theta` indicates the iteration was capped.
    pub final_delta: f64,
}

impl ValueIterationResult {
    /// `argmax_a Q(s, a)` for the trained policy. Returns `None` for
    /// states with no Q entries.
    #[must_use]
    pub fn best_action(&self, state: FeatureVec) -> Option<&AdvisorAction> {
        let mut best: Option<(&AdvisorAction, f64)> = None;
        for ((s, a), q) in &self.q {
            if *s != state {
                continue;
            }
            match best {
                None => best = Some((a, *q)),
                Some((_, best_q)) if *q > best_q => best = Some((a, *q)),
                _ => {}
            }
        }
        best.map(|(a, _)| a)
    }
}

/// Run Q-value iteration on the supplied transition model.
///
/// `actions` is the candidate action enumeration evaluated at each
/// state. `is_terminal(s)` reports whether `s` is an absorbing state
/// (goal-satisfied or abandon-fired). `reward_fn(s, a)` is the
/// immediate reward for taking action `a` in state `s` — reward `0` for
/// terminal states, `-1` for path-length, `-cost(a)` for cost reward.
///
/// `model` is consulted alias-aware so this function is a drop-in for
/// both aliased and non-aliased trained models.
#[must_use]
#[allow(clippy::too_many_arguments)] // callers pass each tunable explicitly
pub fn value_iteration(
    model: &TableModel,
    actions: &[AdvisorAction],
    enable_aliasing: bool,
    is_terminal: impl Fn(&FeatureVec) -> bool,
    reward_fn: impl Fn(&FeatureVec, &AdvisorAction) -> f64,
    config: ValueIterationConfig,
) -> ValueIterationResult {
    // Enumerate the reachable state set: every state appearing as either
    // a key (via `aliases()` lookups) or a value in the transition map.
    let mut states: ahash::AHashSet<FeatureVec> = ahash::AHashSet::new();
    for alias in model.aliases() {
        // Pull the source state out of `Pair` aliases. Other variants are
        // afterstate-collapsed — they don't expose a single source state,
        // so we materialize the source set from the action enumeration
        // separately by also walking next-state distributions.
        if let StateActionAlias::Pair(s, _) = alias {
            states.insert(*s);
        }
        for next in model.distribution_pairs_by_alias(alias).unwrap_or_default() {
            states.insert(next.0);
        }
    }
    if states.is_empty() {
        return ValueIterationResult {
            value: AHashMap::new(),
            q: AHashMap::new(),
            iterations: 0,
            final_delta: 0.0,
        };
    }

    let mut value: AHashMap<FeatureVec, f64> = AHashMap::with_capacity(states.len());
    for s in &states {
        value.insert(*s, 0.0);
    }
    let mut iterations = 0u32;
    let mut final_delta = f64::INFINITY;

    while iterations < config.max_iters {
        let mut delta = 0.0_f64;
        let mut new_value: AHashMap<FeatureVec, f64> = AHashMap::with_capacity(states.len());

        for s in &states {
            if is_terminal(s) {
                new_value.insert(*s, 0.0);
                continue;
            }
            // Pick V_new(s) = max_a Q(s, a).
            let mut best_q = f64::NEG_INFINITY;
            for action in actions {
                let q = compute_q(
                    *s,
                    action,
                    enable_aliasing,
                    model,
                    &value,
                    &reward_fn,
                    config,
                );
                if q > best_q {
                    best_q = q;
                }
            }
            // If no action yields a finite Q (e.g., model has no entry
            // for any action at this state), default to 0 — the policy
            // falls back to beam search at runtime for such states.
            let v_new = if best_q.is_finite() { best_q } else { 0.0 };
            let v_old = value.get(s).copied().unwrap_or(0.0);
            delta = delta.max((v_new - v_old).abs());
            new_value.insert(*s, v_new);
        }

        value = new_value;
        iterations += 1;
        final_delta = delta;
        if delta < config.theta {
            break;
        }
    }

    // Final pass: materialize Q(s, a) per-(state, action) for every
    // pair the model knows about.
    let mut q: AHashMap<(FeatureVec, AdvisorAction), f64> = AHashMap::new();
    for s in &states {
        if is_terminal(s) {
            continue;
        }
        for action in actions {
            let q_val = compute_q(
                *s,
                action,
                enable_aliasing,
                model,
                &value,
                &reward_fn,
                config,
            );
            if q_val.is_finite() {
                q.insert((*s, action.clone()), q_val);
            }
        }
    }

    ValueIterationResult {
        value,
        q,
        iterations,
        final_delta,
    }
}

fn compute_q(
    state: FeatureVec,
    action: &AdvisorAction,
    enable_aliasing: bool,
    model: &TableModel,
    value: &AHashMap<FeatureVec, f64>,
    reward_fn: &impl Fn(&FeatureVec, &AdvisorAction) -> f64,
    config: ValueIterationConfig,
) -> f64 {
    let Some(dist) = model.distribution(state, action, enable_aliasing) else {
        return f64::NEG_INFINITY;
    };
    let r = reward_fn(&state, action);
    let mut expected_v_next = 0.0;
    for (next, p) in dist {
        let v_next = value.get(next).copied().unwrap_or(0.0);
        expected_v_next += p * v_next;
    }
    r + config.gamma * expected_v_next
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::training::model_learner::TableModelBuilder;
    use poc2_engine::ids::CurrencyId;

    fn fv(id: u8) -> FeatureVec {
        FeatureVec {
            rarity: id,
            target_match: u16::from(id),
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

    /// Build a 3-state model: s0 (initial) → s1 (goal) with prob 0.5
    /// per attempt; otherwise self-loop on s0.
    fn build_chaos_spam_model() -> TableModel {
        let mut b = TableModelBuilder::new();
        let alias = StateActionAlias::Pair(fv(0), act("Chaos"));
        // 50/50: success → s1 (goal), failure → s0 (self-loop)
        b.add(alias.clone(), fv(1), 50);
        b.add(alias, fv(0), 50);
        // s1 has no actions (terminal).
        b.finalize()
    }

    #[test]
    fn value_iteration_handles_self_loops_geometric_convergence() {
        // Chaos-spam attractor: at s0 action "Chaos" goes to goal (s1)
        // with p=0.5 each attempt, else self-loops. Path-length reward:
        // V(s1) = 0; V(s0) = -1 + 0.5 × V(s1) + 0.5 × V(s0)
        //                  = -1 + 0.5 × V(s0)
        // Solve: 0.5 × V(s0) = -1, so V(s0) = -2.
        //
        // i.e., expected steps to goal = 1/0.5 = 2.
        let model = build_chaos_spam_model();
        let actions = vec![act("Chaos")];
        let result = value_iteration(
            &model,
            &actions,
            true,
            |s| *s == fv(1),
            |_s, _a| -1.0,
            ValueIterationConfig::default(),
        );
        let v_s0 = *result.value.get(&fv(0)).expect("s0 should be valued");
        let v_s1 = *result.value.get(&fv(1)).expect("s1 should be valued");
        assert!((v_s1 - 0.0).abs() < 1e-6, "terminal state V should be 0");
        assert!(
            (v_s0 - (-2.0)).abs() < 1e-3,
            "Bellman fixed-point should give V(s0) = -2; got {v_s0}"
        );
    }

    /// 3-state path: s0 → s1 → s2 (goal). Each step always succeeds
    /// (probability 1). Path-length reward: V(s2)=0, V(s1)=-1, V(s0)=-2.
    fn build_deterministic_chain_model() -> TableModel {
        let mut b = TableModelBuilder::new();
        let alias_a = StateActionAlias::Pair(fv(0), act("Step"));
        b.add(alias_a, fv(1), 100);
        let alias_b = StateActionAlias::Pair(fv(1), act("Step"));
        b.add(alias_b, fv(2), 100);
        b.finalize()
    }

    #[test]
    fn value_iteration_converges_on_deterministic_chain() {
        let model = build_deterministic_chain_model();
        let actions = vec![act("Step")];
        let result = value_iteration(
            &model,
            &actions,
            true,
            |s| *s == fv(2),
            |_s, _a| -1.0,
            ValueIterationConfig::default(),
        );
        let v_s0 = *result.value.get(&fv(0)).unwrap();
        let v_s1 = *result.value.get(&fv(1)).unwrap();
        let v_s2 = *result.value.get(&fv(2)).unwrap();
        assert!((v_s0 - (-2.0)).abs() < 1e-6);
        assert!((v_s1 - (-1.0)).abs() < 1e-6);
        assert!((v_s2 - 0.0).abs() < 1e-9);
    }

    /// 5-state chain at p=0.5 per step: V(s0)=−10, V(s1)=−8, V(s2)=−6,
    /// V(s3)=−4, V(s4)=0. Geometric expectation per step is `1/p = 2`,
    /// so the chain costs `2 × distance_to_goal`.
    fn build_p_half_chain() -> TableModel {
        let mut b = TableModelBuilder::new();
        for i in 0..4 {
            let from = fv(i);
            let to = fv(i + 1);
            let alias = StateActionAlias::Pair(from, act("Step"));
            b.add(alias.clone(), to, 50);
            b.add(alias, from, 50);
        }
        b.finalize()
    }

    #[test]
    fn value_iteration_geometric_chain_matches_analytic() {
        let model = build_p_half_chain();
        let actions = vec![act("Step")];
        let result = value_iteration(
            &model,
            &actions,
            true,
            |s| *s == fv(4),
            |_s, _a| -1.0,
            ValueIterationConfig {
                max_iters: 5000,
                theta: 1e-9,
                gamma: 1.0,
            },
        );
        for (i, expected) in [(0, -8.0), (1, -6.0), (2, -4.0), (3, -2.0), (4, 0.0)] {
            let v = *result.value.get(&fv(i)).unwrap_or(&f64::NAN);
            assert!(
                (v - expected).abs() < 1e-3,
                "V(s{i}) should be {expected}; got {v}"
            );
        }
    }

    #[test]
    fn cost_reward_picks_cheaper_action_when_both_reach_goal() {
        // s0 has two actions:
        // - "Cheap": to goal s1 with p=0.1 (expected 10 steps), cost=1/step.
        // - "Pricey": to goal s1 with p=1.0 (1 step), cost=20/step.
        // Path-length reward: cheap=−10, pricey=−1. Pricey wins.
        // Cost reward: cheap E[cost] = 10×1 = 10; pricey = 1×20 = 20. Cheap wins.
        let mut b = TableModelBuilder::new();
        let alias_cheap = StateActionAlias::Pair(fv(0), act("Cheap"));
        b.add(alias_cheap.clone(), fv(1), 10);
        b.add(alias_cheap, fv(0), 90);
        let alias_pricey = StateActionAlias::Pair(fv(0), act("Pricey"));
        b.add(alias_pricey, fv(1), 100);
        let model = b.finalize();
        let actions = vec![act("Cheap"), act("Pricey")];

        // Path-length: pricey is best (Q=−1 vs cheap Q=−10).
        let path_result = value_iteration(
            &model,
            &actions,
            true,
            |s| *s == fv(1),
            |_s, _a| -1.0,
            ValueIterationConfig {
                max_iters: 1000,
                theta: 1e-9,
                gamma: 1.0,
            },
        );
        let q_cheap_path = *path_result.q.get(&(fv(0), act("Cheap"))).unwrap();
        let q_pricey_path = *path_result.q.get(&(fv(0), act("Pricey"))).unwrap();
        assert!(
            q_pricey_path > q_cheap_path,
            "path-length: pricey ({q_pricey_path}) should beat cheap ({q_cheap_path})"
        );
        assert!(
            (q_pricey_path - (-1.0)).abs() < 1e-6,
            "Q(s0, Pricey) should be -1; got {q_pricey_path}"
        );

        // Cost: cheap is best.
        let cost_result = value_iteration(
            &model,
            &actions,
            true,
            |s| *s == fv(1),
            |_s, a| {
                if a == &act("Cheap") {
                    -1.0
                } else {
                    -20.0
                }
            },
            ValueIterationConfig {
                max_iters: 1000,
                theta: 1e-9,
                gamma: 1.0,
            },
        );
        let q_cheap_cost = *cost_result.q.get(&(fv(0), act("Cheap"))).unwrap();
        let q_pricey_cost = *cost_result.q.get(&(fv(0), act("Pricey"))).unwrap();
        assert!(
            q_cheap_cost > q_pricey_cost,
            "cost-reward: cheap ({q_cheap_cost}) should beat pricey ({q_pricey_cost})"
        );
    }

    #[test]
    fn value_iteration_returns_zero_on_empty_model() {
        let model = TableModel::default();
        let result = value_iteration(
            &model,
            &[act("Step")],
            true,
            |_s| false,
            |_s, _a| -1.0,
            ValueIterationConfig::default(),
        );
        assert!(result.value.is_empty());
        assert!(result.q.is_empty());
    }

    #[test]
    fn best_action_returns_argmax_q() {
        let model = build_chaos_spam_model();
        let actions = vec![act("Chaos"), act("Pass")];
        let result = value_iteration(
            &model,
            &actions,
            true,
            |s| *s == fv(1),
            |_s, _a| -1.0,
            ValueIterationConfig::default(),
        );
        // Only "Chaos" has a transition entry from s0; "Pass" has none.
        // best_action should return Chaos.
        let best = result.best_action(fv(0));
        assert_eq!(best, Some(&act("Chaos")));
    }
}
