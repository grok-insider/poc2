//! Beam-search planner.
//!
//! See [`ADR-0007`](../../../docs/adr/0007-advisor-beam-search.md) for the
//! algorithm. In short:
//!
//! 1. Build the initial frontier with the input item.
//! 2. For each depth: expand each frontier node via [`crate::candidate`]
//!    generation; simulate every candidate to produce a child node;
//!    score; keep top-`width`.
//! 3. After the configured depth, group children by their *first action*
//!    (the action taken at depth 1) and pick the best frontier
//!    representative under each first-action bucket.
//! 4. Return top-N as [`Recommendation`]s.
//!
//! v1 uses a single deterministic RNG sample per candidate (no Monte
//! Carlo aggregation); the success probability is approximated as
//! `1.0 if the engine returned Ok else 0.0`. M5+ extends to true Monte
//! Carlo with `mc_samples` per candidate.

use poc2_engine::base_registry::BaseRegistry;
use poc2_engine::currency::CurrencyResolver;
use poc2_engine::ids::ItemClassId;
use poc2_engine::item::Item;
use poc2_engine::omen::OmenSet;
use poc2_engine::patch::{League, PatchVersion};
use poc2_engine::registry::ModRegistry;
use poc2_market::{DivEquiv, Valuator};
use poc2_rules::RuleSet;
use poc2_strategies::{PluginPredicateDispatch, PredicateContext, StrategyRegistry};

use crate::action::AdvisorAction;
use crate::candidate::{generate_candidates_with_goal, Candidate};
use crate::featurize::featurize;
use crate::goal::{is_satisfied_with_ctx, should_abandon_with_ctx, Goal};
use crate::recommendation::{LoopEstimate, Recommendation, RecommendationSource};
use crate::scorer::{action_cost, occupancy_adjustment, score, ScoringWeights};
use crate::simulator::simulate_n;
use crate::stash::Stash;
use crate::training::{goal_hash, score_with_trained_policy, TrainedModelCache};

/// Beam-search configuration.
#[derive(Debug, Clone, Copy)]
pub struct BeamConfig {
    /// How many frontier nodes to keep at each depth.
    pub width: u32,
    /// How many rounds of expansion to perform. Depth 1 corresponds to
    /// "single-step recommendation" (no lookahead).
    pub depth: u32,
    /// Risk preference in `[0, 1]`. 0 = cautious (use max-cost band),
    /// 1 = greedy (use expected-cost band).
    pub risk: f64,
    /// How many top recommendations to return.
    pub top_n: u32,
    /// Deterministic RNG seed for the simulator.
    pub seed: u64,
    /// Monte Carlo samples per candidate (Phase C.1, default 50).
    /// `1` reverts to v1's deterministic-single-sample behaviour.
    pub mc_samples: u32,
    /// Scoring weights.
    pub weights: ScoringWeights,
    /// M16.4 — trained-policy uplift weight. When a [`TrainedModelCache`]
    /// hit lookup returns `Q(s, a)` for a candidate's first action, the
    /// node score becomes `q * trained_uplift_weight + base_score`. The
    /// default `1000.0` is large enough that any trained-policy decision
    /// dominates the v2 heuristic score (which lives in `~[-100, 100]`),
    /// while letting the heuristic rank the within-Q-tier ties.
    pub trained_uplift_weight: f64,
}

impl Default for BeamConfig {
    fn default() -> Self {
        Self {
            width: 5,
            depth: 3,
            risk: 0.5,
            top_n: 3,
            seed: 0,
            mc_samples: 50,
            weights: ScoringWeights::default(),
            trained_uplift_weight: 1000.0,
        }
    }
}

/// One frontier node — an item state reached by some action sequence.
#[derive(Debug, Clone)]
struct PlanNode {
    /// Current item state.
    item: Item,
    /// Sum of expected costs paid to reach this state.
    accumulated_cost: DivEquiv,
    /// Joint success probability of getting here (product of per-step
    /// MC means).
    accumulated_prob: f64,
    /// Joint variance of `accumulated_prob` under the independence
    /// assumption (the per-step variance compounded multiplicatively).
    /// Surfaced as the recommendation's `prob_stderr`.
    accumulated_prob_var: f64,
    /// Action sequence we took. `path[0]` is the first action.
    path: Vec<AdvisorAction>,
    /// Source of `path[0]` — used to label the resulting Recommendation.
    first_source: Option<RecommendationSource>,
    /// Rationale of `path[0]`.
    first_rationale: String,
    /// Prior of `path[0]` candidate.
    first_prior: f64,
    /// Cumulative concept-occupancy score adjustment (Phase B.1).
    /// Positive when each step protected keepers; negative when steps
    /// risked them. Added to `score_node`'s base utility so chains
    /// that respect what's already locked outrank chains that don't.
    occupancy_score_delta: f64,
    /// Already-terminated path? (Stop / Abandon / goal-satisfied / abandon-criteria-fired)
    terminated: bool,
}

impl PlanNode {
    fn root(item: Item) -> Self {
        Self {
            item,
            accumulated_cost: DivEquiv::ZERO,
            accumulated_prob: 1.0,
            accumulated_prob_var: 0.0,
            path: Vec::new(),
            first_source: None,
            first_rationale: String::new(),
            first_prior: 1.0,
            occupancy_score_delta: 0.0,
            terminated: false,
        }
    }
}

/// Bundle of inputs for one beam-search run. Cuts down on argument noise
/// and matches the "`recommend(item, goal, ...)`" public-API shape.
pub struct PlanInput<'a> {
    pub item: Item,
    pub goal: Goal,
    pub rules: &'a RuleSet,
    pub strategies: &'a StrategyRegistry,
    pub registry: &'a ModRegistry,
    pub resolver: &'a dyn CurrencyResolver,
    pub valuator: &'a Valuator,
    pub stash: &'a Stash,
    pub patch: PatchVersion,
    /// League ruleset (Standard vs the current challenge league). Gates
    /// items disabled in the challenge league — notably the Recombinator,
    /// which 0.5 "Return of the Ancients" removed from Runes of Aldur but
    /// kept in Standard. Defaults to [`League::Challenge`] in the public
    /// builder. The candidate generator drops Recombine actions when
    /// `recombinator_available(patch, league)` is false.
    pub league: League,
    pub config: BeamConfig,
    /// Plugin-host bridge for custom predicates (Phase F.3). `None`
    /// means the planner runs without plugin custom predicates;
    /// every `ItemPredicate::Custom` evaluates to false.
    pub plugin_dispatch: Option<&'a dyn PluginPredicateDispatch>,
    /// M14.5/M14.6 — base-id → item-class registry for currency gates
    /// that need to know an item's class without consulting `Item.base`'s
    /// placeholder convention. `None` falls back to the back-compat path
    /// where `ItemClassId::from(item.base.as_str())` is treated as the
    /// class id directly. Tests typically pass `None`.
    pub base_registry: Option<&'a BaseRegistry>,
    /// M16.4 — pre-trained Q-table cache. When supplied and the cache
    /// has a model matching `(goal_hash(&goal), item_class)`, the
    /// planner uses [`score_with_trained_policy`] to assign Q-values to
    /// candidate first-actions; Q dominates and the v2 heuristic score
    /// becomes a tiebreaker. `None` means the planner runs unchanged.
    pub trained_models: Option<&'a TrainedModelCache>,
}

/// Build a [`PredicateContext`] for `item` against the planner inputs +
/// the per-node accumulated cost. Predicates that reference cost / stash
/// / valuator data evaluate against this context; everything else
/// continues to read the item directly. Plugin custom predicates
/// dispatch via `input.plugin_dispatch` when set (Phase F.3). The item's
/// class is resolved once here (via `input.base_registry`) so class-gated
/// predicates and the candidate generator share one resolution per node.
fn ctx_for_node<'a>(
    input: &'a PlanInput<'a>,
    item: &Item,
    accumulated_cost: DivEquiv,
) -> PredicateContext<'a> {
    let mut ctx = PredicateContext::new(input.registry)
        .with_cost(accumulated_cost.expected)
        .with_valuator(input.valuator)
        .with_stash(input.stash)
        .with_item_class(item_class_for(item, input.base_registry));
    if let Some(dispatch) = input.plugin_dispatch {
        ctx = ctx.with_plugin_dispatch(dispatch);
    }
    ctx
}

/// Run beam search; return the top-N first-action recommendations.
#[must_use]
pub fn plan(input: &PlanInput<'_>) -> Vec<Recommendation> {
    let cfg = input.config;

    // Short-circuit: if the goal is already met at the root, the only
    // recommendation is to stop. The advisor's job is done.
    let root_ctx = ctx_for_node(input, &input.item, DivEquiv::ZERO);
    if is_satisfied_with_ctx(&input.goal, &input.item, &root_ctx) {
        return vec![Recommendation {
            action: AdvisorAction::Stop,
            source: RecommendationSource::Heuristic {
                name: "goal-already-met".into(),
            },
            expected_cost: DivEquiv::ZERO,
            expected_prob: 1.0,
            goal_progress: 1.0,
            prob_stderr: 0.0,
            score: f64::INFINITY,
            rationale: "Item already satisfies the target; stop and equip or sell.".into(),
            depth: 0,
            loop_estimate: None,
        }];
    }

    let mut frontier = vec![PlanNode::root(input.item.clone())];
    let mut all_terminated: Vec<PlanNode> = Vec::new();
    // Depth-1 advisory (`Guidance`) tips, surfaced only as a fallback when
    // the beam produces no concrete crafting recommendation.
    let mut advisory: Vec<PlanNode> = Vec::new();

    for depth in 1..=cfg.depth {
        let mut next: Vec<(PlanNode, f64)> = Vec::new();
        for node in &frontier {
            if node.terminated {
                next.push((node.clone(), 0.0));
                continue;
            }
            let node_ctx = ctx_for_node(input, &node.item, node.accumulated_cost);
            // Goal already met → terminate.
            if is_satisfied_with_ctx(&input.goal, &node.item, &node_ctx) {
                let mut t = node.clone();
                t.terminated = true;
                all_terminated.push(t.clone());
                next.push((t, f64::INFINITY));
                continue;
            }
            // Abandon criteria → terminate but with low score.
            if should_abandon_with_ctx(&input.goal, &node.item, &node_ctx) {
                let mut t = node.clone();
                t.terminated = true;
                all_terminated.push(t.clone());
                next.push((t, f64::NEG_INFINITY));
                continue;
            }

            let cands = generate_candidates_with_goal(
                &node.item,
                &node_ctx,
                input.rules,
                input.strategies,
                input.resolver,
                input.stash,
                input.patch,
                input.league,
                Some(&input.goal),
                input.registry,
                input.base_registry,
            );
            for cand in cands {
                // `Guidance` is non-mutating meta-advice: expanding it yields
                // a child identical to the parent, which would waste beam
                // slots and crowd out concrete crafting branches. Collect
                // such tips separately (depth-1 only) and surface them as a
                // fallback when no concrete step exists; never expand them in
                // the beam.
                if matches!(cand.action, AdvisorAction::Guidance { .. }) {
                    if depth == 1 {
                        let child = expand_with_candidate(node, &cand, depth, input);
                        advisory.push(child);
                    }
                    continue;
                }
                let child = expand_with_candidate(node, &cand, depth, input);
                let s = score_node(&child, input, &cfg);
                next.push((child, s));
            }
        }
        // Prune to top `width`.
        next.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        next.truncate(cfg.width as usize);
        frontier = next.into_iter().map(|(n, _)| n).collect();
        if frontier.is_empty() {
            break;
        }
    }

    // Group by first action; pick the highest-scoring node per group, then
    // rank and truncate to top-N.
    let grouped = group_by_first_action(&frontier, &all_terminated, input, &cfg);

    // Fallback: if the beam produced no concrete recommendation (e.g. the
    // only candidates were advisory guidance), surface the best advisory tip
    // so the UI is never empty.
    if grouped.is_empty() {
        return advisory_recommendations(&advisory, input, &cfg);
    }

    grouped
        .into_iter()
        .map(|(n, s)| node_to_recommendation(&n, s, input))
        .collect()
}

/// Group terminal/frontier nodes by their first action, keeping the
/// highest-scoring node per first-action, then sort by score and truncate to
/// `top_n`.
fn group_by_first_action(
    frontier: &[PlanNode],
    all_terminated: &[PlanNode],
    input: &PlanInput<'_>,
    cfg: &BeamConfig,
) -> Vec<(PlanNode, f64)> {
    let mut by_first: ahash::AHashMap<AdvisorAction, (PlanNode, f64)> = ahash::AHashMap::new();
    for node in frontier.iter().chain(all_terminated.iter()) {
        let Some(key) = node.path.first().cloned() else {
            continue;
        };
        let s = score_node(node, input, cfg);
        by_first
            .entry(key)
            .and_modify(|(existing, score)| {
                if s > *score {
                    *existing = node.clone();
                    *score = s;
                }
            })
            .or_insert((node.clone(), s));
    }
    let mut grouped: Vec<(PlanNode, f64)> = by_first.into_values().collect();
    grouped.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    grouped.truncate(cfg.top_n as usize);
    grouped
}

/// Rank depth-1 advisory (`Guidance`) nodes as a fallback recommendation set
/// when the beam produced no concrete crafting step.
fn advisory_recommendations(
    advisory: &[PlanNode],
    input: &PlanInput<'_>,
    cfg: &BeamConfig,
) -> Vec<Recommendation> {
    let mut adv: Vec<(PlanNode, f64)> = advisory
        .iter()
        .filter(|n| !n.path.is_empty())
        .map(|n| {
            let s = score_node(n, input, cfg);
            (n.clone(), s)
        })
        .collect();
    adv.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    adv.truncate(cfg.top_n as usize);
    adv.into_iter()
        .map(|(n, s)| node_to_recommendation(&n, s, input))
        .collect()
}

/// Build the child node by running an MC sweep of `cand` against
/// `node.item`. Per-step probability comes from the MC mean; variance
/// compounds multiplicatively under independence.
fn expand_with_candidate(
    node: &PlanNode,
    cand: &Candidate,
    depth: u32,
    input: &PlanInput<'_>,
) -> PlanNode {
    let mut next = node.clone();
    let omens = OmenSet::new();
    let seed_base = input
        .config
        .seed
        .wrapping_add(u64::from(depth))
        .wrapping_add(node.path.len() as u64);
    // Phase B.1 — compute the occupancy adjustment using the *pre-action*
    // item, since the heuristic asks "does this currency risk what's
    // already locked here". The simulator overwrites `next.item` below.
    let occ = occupancy_adjustment(&cand.action, &node.item, &input.goal, input.registry);
    next.occupancy_score_delta += occ;

    let mc = simulate_n(
        &node.item,
        &cand.action,
        &omens,
        input.registry,
        input.resolver,
        input.patch,
        seed_base,
        input.config.mc_samples,
    );
    next.item = mc.primary.item;
    let action_cost_band = action_cost(&cand.action, input.valuator);
    next.accumulated_cost = next.accumulated_cost.plus(action_cost_band);
    // Step success probability with a small floor/ceiling to avoid
    // multiplying an entire tree by 0 or 1 (which masks the rest of
    // the per-step variance contributions).
    let step_prob = mc.mean_success_prob.clamp(0.05, 0.95);
    let step_var = mc.prob_stderr.powi(2);
    // Variance of a product (under independence): if X = A*B,
    //   Var(X) = E[A]^2 * Var(B) + E[B]^2 * Var(A) + Var(A)*Var(B)
    let prev_p = next.accumulated_prob;
    let prev_var = next.accumulated_prob_var;
    next.accumulated_prob = prev_p * step_prob;
    next.accumulated_prob_var =
        prev_p.powi(2) * step_var + step_prob.powi(2) * prev_var + prev_var * step_var;
    next.path.push(cand.action.clone());
    if next.first_source.is_none() {
        next.first_source = Some(cand.source.clone());
        next.first_rationale.clone_from(&cand.rationale);
        next.first_prior = cand.prior;
    }
    if cand.action.is_terminal() {
        next.terminated = true;
    }
    next
}

/// Score a frontier node against the goal.
fn score_node(node: &PlanNode, input: &PlanInput<'_>, cfg: &BeamConfig) -> f64 {
    let ctx = ctx_for_node(input, &node.item, node.accumulated_cost);
    let progress = if is_satisfied_with_ctx(&input.goal, &node.item, &ctx) {
        1.0
    } else {
        partial_progress(&node.item, &input.goal, input.registry)
    };
    // Goal-attainment = execution-reliability × goal-progress. Keeping these
    // MULTIPLIED is what suppresses cheap no-ops: an action that makes zero
    // progress earns zero attainment credit no matter how "reliable" it is, so
    // a safe-but-useless Annul/Divine can't ride its high per-step success to
    // the top. On top of that we add an explicit, weighted structural-progress
    // reward (activates the previously dead `ScoringWeights.progress_bonus`) so
    // that — among building steps — the one reaching MORE of the target wins
    // even when it's the riskier path. (Premature Divine on a still-partial item
    // is handled separately by the `tier_fix_candidates` gate.)
    let success_prob = node.accumulated_prob * progress;
    let base = score(
        success_prob,
        node.accumulated_cost,
        node.first_prior,
        cfg.risk,
        cfg.weights,
    ) + cfg.weights.progress_bonus * progress;
    // Phase B.1 — fold the cumulative occupancy adjustment in. We also
    // amplify by the risk-aware variance penalty so a cautious user
    // (low risk) feels the protection bonus more strongly than a
    // greedy user; this keeps the variance/protection signals aligned.
    let risk_factor = 1.0 - cfg.risk.clamp(0.0, 1.0).powi(2);
    let heuristic = base + node.occupancy_score_delta * risk_factor;

    // M16.4 — trained-policy uplift. When a model exists for this
    // (goal, item-class) and the lookup hits the (state, first-action)
    // pair, blend the Q-value into the score. Q dominates the
    // heuristic; the heuristic stays as a tiebreaker. Lookup miss →
    // pure heuristic ranking.
    if let Some(q) = trained_policy_q(node, input, cfg.risk) {
        return q * cfg.trained_uplift_weight + heuristic;
    }
    heuristic
}

/// Look up the trained policy's Q-value for a node's **first action** from
/// the **root state**, when a model exists for the current
/// `(goal, item-class)`.
///
/// `Q(featurize(input.item), path[0])` — deliberately the root state, not
/// the node's: recommendations are ranked by their first action, and
/// `Q(s0, a1)` is exactly the trained policy's preference over first
/// actions. Every descendant of the same first action shares the Q score,
/// so the v2 heuristic ranks within-path ties at any depth. (The
/// historical `Q(fv(node), path[0])` keyed the CURRENT state to the FIRST
/// action — a pair that usually doesn't exist in the model at depth ≥ 2,
/// silently dropping deep nodes back to heuristic scale mid-frontier.)
///
/// When the artefact carries the cost-reward twin, the returned value is
/// the docs/81 §6.3 risk blend `(1 − risk)·Q_cost + risk·Q_steps` — a
/// cautious (low-risk) user prioritizes expected cost, a greedy one
/// expected steps. Path-only artefacts fall back to `Q_steps` alone.
///
/// Returns `None` when:
///
/// - no cache supplied
/// - node is the root (no first action yet)
/// - cache miss for the goal hash + class
/// - the path-length model has no entry for `(root_state, first_action)`
///
/// The advisor uses `None` as the signal to fall back to v2 heuristic
/// scoring.
fn trained_policy_q(node: &PlanNode, input: &PlanInput<'_>, risk: f64) -> Option<f64> {
    let cache = input.trained_models?;
    let first_action = node.path.first()?;
    let item_class = item_class_for(&input.item, input.base_registry);
    let goal_h = goal_hash(&input.goal);
    let (path_model, cost_model) = cache.lookup_pair(goal_h, &item_class)?;
    let root_fv = featurize(&input.item, &input.goal, input.registry);
    let q_steps = score_with_trained_policy(path_model, root_fv, first_action)?;
    let q_cost = cost_model.and_then(|m| m.q_at(root_fv, first_action));
    Some(match q_cost {
        Some(qc) => {
            let r = risk.clamp(0.0, 1.0);
            (1.0 - r) * qc + r * q_steps
        }
        None => q_steps,
    })
}

/// Resolve `Item.base` → `ItemClassId`, honouring [`BaseRegistry`]
/// when supplied. Routes through `BaseRegistry::resolve_item_class`
/// (against the shared empty registry when none is supplied) so the
/// legacy placeholder fallback — and its misclassification warning —
/// live in exactly one place.
fn item_class_for(item: &Item, base_registry: Option<&BaseRegistry>) -> ItemClassId {
    base_registry
        .unwrap_or(&poc2_engine::base_registry::EMPTY)
        .resolve_item_class(item)
}

/// Roughly: fraction of the goal's target specs that are satisfied.
///
/// This is intentionally registry-only (no cost / market context): partial
/// progress is a structural property of the item's mods + the target spec,
/// not of the planner state. Constraints that depend on cost/market are
/// counted separately by [`is_satisfied_with_ctx`] in [`score_node`].
fn partial_progress(item: &Item, goal: &Goal, registry: &ModRegistry) -> f64 {
    let total = goal.target.prefixes.len() + goal.target.suffixes.len();
    if total == 0 {
        return 1.0;
    }
    let ctx = PredicateContext::new(registry);
    let mut hits = 0;
    for spec in &goal.target.prefixes {
        let g = Goal {
            target: poc2_strategies::Target {
                prefixes: vec![spec.clone()],
                suffixes: vec![],
                constraints: vec![],
            },
            abandon_criteria: vec![],
            budget: goal.budget,
        };
        if is_satisfied_with_ctx(&g, item, &ctx) {
            hits += 1;
        }
    }
    for spec in &goal.target.suffixes {
        let g = Goal {
            target: poc2_strategies::Target {
                prefixes: vec![],
                suffixes: vec![spec.clone()],
                constraints: vec![],
            },
            abandon_criteria: vec![],
            budget: goal.budget,
        };
        if is_satisfied_with_ctx(&g, item, &ctx) {
            hits += 1;
        }
    }
    f64::from(hits) / total as f64
}

fn node_to_recommendation(
    node: &PlanNode,
    score_value: f64,
    input: &PlanInput<'_>,
) -> Recommendation {
    let valuator = input.valuator;
    let action = node.path[0].clone();
    let cost = action_cost(&action, valuator);
    let source = node
        .first_source
        .clone()
        .unwrap_or(RecommendationSource::Heuristic {
            name: "fallback".into(),
        });
    let loop_estimate = if let AdvisorAction::Recurring { inner, .. } = &action {
        Some(estimate_loop_iterations(inner, node, valuator))
    } else {
        None
    };
    // Display metrics describe the user's CURRENT item, not the plan's
    // single-rollout terminal state. `goal_progress` is the fraction of goal
    // specs the item ALREADY satisfies — a stable, intuitive "n/m specs" bar
    // (the noisy terminal state of one Monte-Carlo rollout would make it jump
    // around). `expected_prob` is the honest P(reach goal) = execution-
    // reliability × that closeness, so a useless step far from the goal reads
    // low instead of the old ~90% raw step-execution probability. (Ranking, in
    // `score_node`, still uses the terminal projected progress to reward
    // building plans.)
    let ctx = ctx_for_node(input, &input.item, DivEquiv::ZERO);
    let progress = if is_satisfied_with_ctx(&input.goal, &input.item, &ctx) {
        1.0
    } else {
        partial_progress(&input.item, &input.goal, input.registry)
    };
    Recommendation {
        action,
        source,
        expected_cost: cost,
        expected_prob: node.accumulated_prob * progress,
        prob_stderr: node.accumulated_prob_var.sqrt() * progress,
        goal_progress: progress,
        score: score_value,
        rationale: node.first_rationale.clone(),
        depth: node.path.len() as u32,
        loop_estimate,
    }
}

/// Estimate the iteration count and total cost for a Recurring step.
///
/// We fold `(per-iter success prob, per-iter cost)` into a geometric
/// expectation. With `p` = single-iteration progress probability and
/// per-iter cost `c`:
///
/// - Mean iterations until success ≈ `1 / max(p, 0.05)`.
/// - Stderr ≈ `sqrt((1 - p) / p^2)` (standard geometric stderr).
/// - Total cost band = inner cost × mean iterations.
///
/// The `node` carries the planner's MC-derived per-step success
/// probability via `accumulated_prob`; we use that as the proxy for
/// per-iteration progress. If the planner discovered the loop without
/// a probability sample (e.g., heuristic-emitted), we fall back to
/// `0.5` which produces a 2-iteration central estimate.
fn estimate_loop_iterations(
    inner: &[AdvisorAction],
    node: &PlanNode,
    valuator: &Valuator,
) -> LoopEstimate {
    let p = if node.accumulated_prob > 0.0 && node.accumulated_prob < 1.0 {
        node.accumulated_prob
    } else {
        0.5
    };
    let p_floored = p.max(0.05);
    let mean = 1.0 / p_floored;
    let stderr = ((1.0 - p_floored) / (p_floored * p_floored)).sqrt();
    let mut per_iter_cost = DivEquiv::ZERO;
    for leaf in inner {
        per_iter_cost = per_iter_cost.plus(action_cost(leaf, valuator));
    }
    LoopEstimate::new(mean, stderr, per_iter_cost)
}

#[cfg(test)]
mod tests {
    use super::*;
    use poc2_engine::currency::DefaultCurrencyResolver;
    use poc2_engine::ids::ItemClassId;
    use poc2_engine::item::{ModRoll, QualityKind, Rarity};
    use poc2_strategies::Target;
    use smallvec::smallvec;

    fn empty_item(rarity: Rarity) -> Item {
        Item {
            base: ItemClassId::from("BodyArmour").as_str().into(),
            ilvl: 82,
            rarity,
            corrupted: false,
            sanctified: false,
            mirrored: false,
            quality: 0,
            quality_kind: QualityKind::Untagged,
            implicits: smallvec![],
            prefixes: smallvec![],
            suffixes: smallvec![],
            enchantments: smallvec![],
            hidden_desecrated: None,
            sockets: smallvec![],
            hinekora_lock: None,
        }
    }

    /// A Magic Body Armour with one filler prefix — yields multiple
    /// *concrete* depth-1 candidates (Augmentation to fill the open suffix,
    /// Regal to promote, plus Greater/Perfect variants), which the
    /// trained-policy reorder test needs.
    fn magic_one_prefix_item() -> Item {
        let mut it = empty_item(Rarity::Magic);
        it.prefixes.push(ModRoll {
            mod_id: poc2_engine::ids::ModId::from("FillerPrefix"),
            affix_type: poc2_engine::item::AffixType::Prefix,
            kind: poc2_engine::mods::ModKind::Explicit,
            values: smallvec![],
            is_fractured: false,
        });
        it
    }

    /// Build a non-trivial goal so [`is_satisfied`] returns false at root,
    /// forcing the planner to actually generate candidates.
    fn three_es_prefix_goal() -> Goal {
        use poc2_strategies::TargetSpec;
        Goal::new(
            Target {
                prefixes: vec![TargetSpec {
                    concept: Some(poc2_engine::ConceptId::from("EnergyShield")),
                    concept_any: vec![],
                    affix: None,
                    count: 3,
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
    fn plan_returns_recommendations_for_normal_with_seed_rules() {
        let registry = ModRegistry::from_mods(vec![], vec![]);
        let resolver = DefaultCurrencyResolver::new();
        let rules = RuleSet::from_rules(poc2_rules::seed_rules());
        let strategies = StrategyRegistry::default();
        let valuator = Valuator::default();
        let stash = Stash::unlimited();

        let input = PlanInput {
            item: empty_item(Rarity::Normal),
            goal: three_es_prefix_goal(),
            rules: &rules,
            strategies: &strategies,
            registry: &registry,
            resolver: &resolver,
            valuator: &valuator,
            stash: &stash,
            patch: PatchVersion::PATCH_0_4_0,
            league: League::current(),
            plugin_dispatch: None,
            base_registry: None,
            trained_models: None,
            config: BeamConfig {
                width: 3,
                depth: 1,
                top_n: 3,
                ..BeamConfig::default()
            },
        };
        let recs = plan(&input);
        assert!(!recs.is_empty(), "advisor returned no recommendations");
        // Highest-score recommendation should be a Perfect Transmute (R001 fires).
        // Top-3 should include it even if priority weighting moves a guidance
        // rule above it (post-A.5 the rule catalogue grew to ~100 rules,
        // some of which are higher-priority guidance/warnings).
        let has_transmute = recs.iter().take(3).any(|r| {
            matches!(
                &r.action,
                AdvisorAction::ApplyCurrency { currency, .. }
                    if currency.as_str() == "PerfectOrbOfTransmutation"
            )
        });
        assert!(
            has_transmute,
            "Perfect Transmute should appear in the top-3 recommendations; got: {:#?}",
            recs.iter()
                .map(|r| (r.action.clone(), r.source.clone(), r.score))
                .collect::<Vec<_>>()
        );
        let cites_r001 = recs
            .iter()
            .take(3)
            .any(|r| matches!(&r.source, RecommendationSource::Rule { id, .. } if id.starts_with("R001")));
        assert!(
            cites_r001,
            "R001 should be cited in the top-3 recommendations"
        );
    }

    #[test]
    fn plan_with_empty_target_emits_stop() {
        let registry = ModRegistry::from_mods(vec![], vec![]);
        let resolver = DefaultCurrencyResolver::new();
        let rules = RuleSet::default();
        let strategies = StrategyRegistry::default();
        let valuator = Valuator::default();
        let stash = Stash::unlimited();
        let input = PlanInput {
            item: empty_item(Rarity::Magic),
            goal: Goal::new(Target::default(), DivEquiv::point(50.0)),
            rules: &rules,
            strategies: &strategies,
            registry: &registry,
            resolver: &resolver,
            valuator: &valuator,
            stash: &stash,
            patch: PatchVersion::PATCH_0_4_0,
            league: League::current(),
            plugin_dispatch: None,
            base_registry: None,
            trained_models: None,
            config: BeamConfig::default(),
        };
        let recs = plan(&input);
        assert_eq!(recs.len(), 1);
        assert!(matches!(recs[0].action, AdvisorAction::Stop));
    }

    #[test]
    fn plan_filters_unaffordable_actions() {
        let registry = ModRegistry::from_mods(vec![], vec![]);
        let resolver = DefaultCurrencyResolver::new();
        let rules = RuleSet::from_rules(poc2_rules::seed_rules());
        let strategies = StrategyRegistry::default();
        let valuator = Valuator::default();
        let stash = Stash::new(); // empty
        let input = PlanInput {
            item: empty_item(Rarity::Normal),
            goal: three_es_prefix_goal(),
            rules: &rules,
            strategies: &strategies,
            registry: &registry,
            resolver: &resolver,
            valuator: &valuator,
            stash: &stash,
            patch: PatchVersion::PATCH_0_4_0,
            league: League::current(),
            plugin_dispatch: None,
            base_registry: None,
            trained_models: None,
            config: BeamConfig::default(),
        };
        let recs = plan(&input);
        for r in &recs {
            // Whatever survives the empty stash must be non-currency
            // (Stop / Abandon / Guidance) since the user owns nothing.
            assert!(
                !r.action.is_mutating()
                    || matches!(
                        r.action,
                        AdvisorAction::ApplyHinekorasLock | AdvisorAction::Reveal { .. }
                    )
            );
        }
    }

    // ---------------------------------------------------------------
    // M16.4 — trained-policy uplift wiring
    // ---------------------------------------------------------------

    #[test]
    fn trained_policy_q_returns_none_without_cache() {
        // No cache supplied → trained_policy_q must return None and the
        // planner falls back to v2 heuristic ranking (already tested
        // above). This is the regression guard against accidentally
        // forcing the trained-policy path when callers omit the field.
        let registry = ModRegistry::from_mods(vec![], vec![]);
        let resolver = DefaultCurrencyResolver::new();
        let rules = RuleSet::default();
        let strategies = StrategyRegistry::default();
        let valuator = Valuator::default();
        let stash = Stash::unlimited();
        let input = PlanInput {
            item: empty_item(Rarity::Normal),
            goal: three_es_prefix_goal(),
            rules: &rules,
            strategies: &strategies,
            registry: &registry,
            resolver: &resolver,
            valuator: &valuator,
            stash: &stash,
            patch: PatchVersion::PATCH_0_4_0,
            league: League::current(),
            plugin_dispatch: None,
            base_registry: None,
            trained_models: None,
            config: BeamConfig::default(),
        };
        let dummy_node = PlanNode::root(input.item.clone());
        // Empty path → no first action → None even if a cache had one.
        assert!(trained_policy_q(&dummy_node, &input, 0.5).is_none());
    }

    /// Score a fresh `Normal`-rarity body armour through the planner
    /// twice — once without a cache (baseline heuristic), once with a
    /// cache that assigns an enormous Q to a *non-default* candidate.
    /// Verify the trained policy successfully reorders the
    /// recommendations.
    #[test]
    fn trained_policy_uplift_reorders_recommendations_for_in_set_action() {
        use crate::training::value_iteration::ValueIterationResult;
        use crate::training::{trained_model_from, RewardKind, TrainedModelCache};
        use ahash::AHashMap;

        let registry = ModRegistry::from_mods(vec![], vec![]);
        let resolver = DefaultCurrencyResolver::new();
        let rules = RuleSet::from_rules(poc2_rules::seed_rules());
        let strategies = StrategyRegistry::default();
        let valuator = Valuator::default();
        let stash = Stash::unlimited();
        // Magic item with one prefix → multiple concrete depth-1 candidates,
        // so the trained-policy uplift has an underdog to promote.
        let item = magic_one_prefix_item();
        let goal = three_es_prefix_goal();
        let goal_h = goal_hash(&goal);

        // Baseline run with no cache to discover what the planner
        // generated as candidates and what the heuristic ranked them.
        let baseline_input = PlanInput {
            item: item.clone(),
            goal: goal.clone(),
            rules: &rules,
            strategies: &strategies,
            registry: &registry,
            resolver: &resolver,
            valuator: &valuator,
            stash: &stash,
            patch: PatchVersion::PATCH_0_4_0,
            league: League::current(),
            plugin_dispatch: None,
            base_registry: None,
            trained_models: None,
            config: BeamConfig {
                width: 8,
                depth: 1,
                top_n: 8,
                ..BeamConfig::default()
            },
        };
        let baseline = plan(&baseline_input);
        assert!(baseline.len() >= 2, "need at least 2 recs for the test");
        let baseline_top = baseline[0].action.clone();
        // Pick the second-place action as the trained-policy
        // pet-favourite; we want to verify the uplift can flip it to
        // first place.
        let underdog = baseline[1].action.clone();
        assert_ne!(baseline_top, underdog);

        // Build a model that assigns a huge Q value to `underdog` from
        // the root state's featurization.
        let root_fv = featurize(&item, &goal, &registry);
        let mut q: AHashMap<(crate::featurize::FeatureVec, AdvisorAction), f64> = AHashMap::new();
        q.insert((root_fv, underdog.clone()), 999.0);
        let result = ValueIterationResult {
            value: AHashMap::new(),
            q,
            iterations: 0,
            final_delta: 0.0,
        };
        let model = trained_model_from(
            goal_h,
            ItemClassId::from("BodyArmour"),
            poc2_data::BUNDLE_SCHEMA_VERSION,
            poc2_engine::ENGINE_SCHEMA_VERSION,
            RewardKind::PathLength,
            &result,
            None,
        );
        let mut cache = TrainedModelCache::new();
        cache.insert(model);

        let trained_input = PlanInput {
            trained_models: Some(&cache),
            item: item.clone(),
            goal: goal.clone(),
            rules: &rules,
            strategies: &strategies,
            registry: &registry,
            resolver: &resolver,
            valuator: &valuator,
            stash: &stash,
            patch: PatchVersion::PATCH_0_4_0,
            league: League::current(),
            plugin_dispatch: None,
            base_registry: None,
            config: BeamConfig {
                width: 8,
                depth: 1,
                top_n: 8,
                ..BeamConfig::default()
            },
        };
        let trained = plan(&trained_input);
        let trained_top = trained
            .first()
            .map(|r| r.action.clone())
            .expect("cache-augmented run must produce a recommendation");
        assert_eq!(
            trained_top, underdog,
            "trained-policy uplift should force the underdog to top: \
             baseline={baseline:#?}, trained={trained:#?}"
        );
    }

    // ---------------------------------------------------------------
    // Anti-myopia — de-diluted goal-progress scoring
    // ---------------------------------------------------------------

    /// White-box guard for the goal-progress scoring. Two nodes with identical
    /// reliability / cost / prior but terminal progress 0 vs 1 differ in score by
    /// `accumulated_prob·Δprogress` (the multiplicative attainment term) PLUS
    /// `progress_bonus·Δprogress` (the additive structural booster). With
    /// `accumulated_prob = 0.5` and Δprogress = 1, the delta is exactly
    /// `0.5 + progress_bonus`. Pins both halves of the progress signal and fails
    /// if either the multiplicative term or the booster is dropped.
    #[test]
    fn score_node_rewards_goal_progress() {
        use poc2_engine::ids::{ConceptId, ModGroupId, StatId, TagId};
        use poc2_engine::item::AffixType;
        use poc2_engine::mods::{ModDefinition, ModDomain, ModFlags, ModGroup, SpawnWeight};
        use poc2_engine::patch::PatchRange;
        use poc2_engine::ModStat;

        // Registry with one ES prefix mod so an item carrying it satisfies a
        // one-ES-prefix goal (progress 1.0); an empty item is progress 0.0.
        let es_mod = ModDefinition {
            id: poc2_engine::ids::ModId::from("ES1"),
            name: Some("ES".into()),
            mod_group: ModGroup(ModGroupId::from("ES1")),
            affix_type: AffixType::Prefix,
            kind: poc2_engine::mods::ModKind::Explicit,
            domain: ModDomain::Item,
            tags: smallvec![],
            concept_set: smallvec![ConceptId::from("EnergyShield")],
            spawn_weights: smallvec![SpawnWeight {
                tag: TagId::from("BodyArmour"),
                weight: 1
            }],
            stats: smallvec![ModStat {
                stat_id: StatId::from("es"),
                min: 100.0,
                max: 200.0,
            }],
            required_level: 1,
            tier: None,
            allowed_item_classes: smallvec![ItemClassId::from("BodyArmour")],
            patch_range: PatchRange::ALL,
            flags: ModFlags::empty(),
            text_template: None,
        };
        let registry = ModRegistry::from_mods(vec![es_mod], vec![]);
        let resolver = DefaultCurrencyResolver::new();
        let rules = RuleSet::default();
        let strategies = StrategyRegistry::default();
        let valuator = Valuator::default();
        let stash = Stash::unlimited();
        let goal = Goal::new(
            Target {
                prefixes: vec![poc2_strategies::TargetSpec {
                    concept: Some(ConceptId::from("EnergyShield")),
                    concept_any: vec![],
                    affix: Some(AffixType::Prefix),
                    count: 1,
                    min_tier: None,
                    allow_hybrid: true,
                }],
                suffixes: vec![],
                constraints: vec![],
            },
            DivEquiv::point(100.0),
        );
        let input = PlanInput {
            item: empty_item(Rarity::Magic),
            goal,
            rules: &rules,
            strategies: &strategies,
            registry: &registry,
            resolver: &resolver,
            valuator: &valuator,
            stash: &stash,
            patch: PatchVersion::PATCH_0_4_0,
            league: League::current(),
            plugin_dispatch: None,
            base_registry: None,
            trained_models: None,
            config: BeamConfig::default(),
        };
        let cfg = input.config;

        // Same reliability / cost / prior; only terminal progress differs.
        let mut node_a = PlanNode::root(empty_item(Rarity::Magic));
        node_a.accumulated_prob = 0.5;
        let mut item_b = empty_item(Rarity::Magic);
        item_b.prefixes.push(ModRoll {
            mod_id: poc2_engine::ids::ModId::from("ES1"),
            affix_type: AffixType::Prefix,
            kind: poc2_engine::mods::ModKind::Explicit,
            values: smallvec![150.0],
            is_fractured: false,
        });
        let mut node_b = PlanNode::root(item_b);
        node_b.accumulated_prob = 0.5;

        let sa = score_node(&node_a, &input, &cfg);
        let sb = score_node(&node_b, &input, &cfg);
        // accumulated_prob (0.5) × Δprogress (1.0) + progress_bonus × Δprogress.
        let expected_delta = 0.5 + cfg.weights.progress_bonus;
        assert!(
            (sb - sa - expected_delta).abs() < 1e-9,
            "progress reward must be reliability×progress + progress_bonus×progress \
             (== {expected_delta}); got delta {}",
            sb - sa
        );
        assert!(
            sb > sa,
            "the progress-bearing node must rank strictly higher"
        );
    }
}
