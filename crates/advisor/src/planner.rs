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

use poc2_engine::currency::CurrencyResolver;
use poc2_engine::item::Item;
use poc2_engine::omen::OmenSet;
use poc2_engine::patch::PatchVersion;
use poc2_engine::registry::ModRegistry;
use poc2_market::{DivEquiv, Valuator};
use poc2_rules::RuleSet;
use poc2_strategies::{PluginPredicateDispatch, PredicateContext, StrategyRegistry};

use crate::action::AdvisorAction;
use crate::candidate::{generate_candidates_with_goal, Candidate};
use crate::goal::{is_satisfied_with_ctx, should_abandon_with_ctx, Goal};
use crate::recommendation::{LoopEstimate, Recommendation, RecommendationSource};
use crate::scorer::{action_cost, occupancy_adjustment, score, ScoringWeights};
use crate::simulator::simulate_n;
use crate::stash::Stash;

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
    pub config: BeamConfig,
    /// Plugin-host bridge for custom predicates (Phase F.3). `None`
    /// means the planner runs without plugin custom predicates;
    /// every `ItemPredicate::Custom` evaluates to false.
    pub plugin_dispatch: Option<&'a dyn PluginPredicateDispatch>,
}

/// Build a [`PredicateContext`] for `item` against the planner inputs +
/// the per-node accumulated cost. Predicates that reference cost / stash
/// / valuator data evaluate against this context; everything else
/// continues to read the item directly. Plugin custom predicates
/// dispatch via `input.plugin_dispatch` when set (Phase F.3).
fn ctx_for_node<'a>(input: &'a PlanInput<'a>, accumulated_cost: DivEquiv) -> PredicateContext<'a> {
    let mut ctx = PredicateContext::new(input.registry)
        .with_cost(accumulated_cost.expected)
        .with_valuator(input.valuator)
        .with_stash(input.stash);
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
    let root_ctx = ctx_for_node(input, DivEquiv::ZERO);
    if is_satisfied_with_ctx(&input.goal, &input.item, &root_ctx) {
        return vec![Recommendation {
            action: AdvisorAction::Stop,
            source: RecommendationSource::Heuristic {
                name: "goal-already-met".into(),
            },
            expected_cost: DivEquiv::ZERO,
            expected_prob: 1.0,
            prob_stderr: 0.0,
            score: f64::INFINITY,
            rationale: "Item already satisfies the target; stop and equip or sell.".into(),
            depth: 0,
            loop_estimate: None,
        }];
    }

    let mut frontier = vec![PlanNode::root(input.item.clone())];
    let mut all_terminated: Vec<PlanNode> = Vec::new();

    for depth in 1..=cfg.depth {
        let mut next: Vec<(PlanNode, f64)> = Vec::new();
        for node in &frontier {
            if node.terminated {
                next.push((node.clone(), 0.0));
                continue;
            }
            let node_ctx = ctx_for_node(input, node.accumulated_cost);
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
                Some(&input.goal),
                input.registry,
            );
            for cand in cands {
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

    // Group by first action; pick the highest-scoring node per group.
    let mut by_first: ahash::AHashMap<AdvisorAction, (PlanNode, f64)> = ahash::AHashMap::new();
    let candidates_for_grouping = frontier.iter().chain(all_terminated.iter());
    for node in candidates_for_grouping {
        if node.path.is_empty() {
            continue;
        }
        let key = node.path[0].clone();
        let s = score_node(node, input, &cfg);
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
        .into_iter()
        .map(|(n, s)| node_to_recommendation(&n, s, input.valuator))
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
    let ctx = ctx_for_node(input, node.accumulated_cost);
    let progress = if is_satisfied_with_ctx(&input.goal, &node.item, &ctx) {
        1.0
    } else {
        partial_progress(&node.item, &input.goal, input.registry)
    };
    let success_prob = node.accumulated_prob * progress;
    let base = score(
        success_prob,
        node.accumulated_cost,
        node.first_prior,
        cfg.risk,
        cfg.weights,
    );
    // Phase B.1 — fold the cumulative occupancy adjustment in. We also
    // amplify by the risk-aware variance penalty so a cautious user
    // (low risk) feels the protection bonus more strongly than a
    // greedy user; this keeps the variance/protection signals aligned.
    let risk_factor = 1.0 - cfg.risk.clamp(0.0, 1.0).powi(2);
    base + node.occupancy_score_delta * risk_factor
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
    valuator: &Valuator,
) -> Recommendation {
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
    Recommendation {
        action,
        source,
        expected_cost: cost,
        expected_prob: node.accumulated_prob,
        prob_stderr: node.accumulated_prob_var.sqrt(),
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
    use poc2_engine::item::{QualityKind, Rarity};
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
        let registry = ModRegistry::from_mods(vec![]);
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
            plugin_dispatch: None,
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
        let registry = ModRegistry::from_mods(vec![]);
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
            plugin_dispatch: None,
            config: BeamConfig::default(),
        };
        let recs = plan(&input);
        assert_eq!(recs.len(), 1);
        assert!(matches!(recs[0].action, AdvisorAction::Stop));
    }

    #[test]
    fn plan_filters_unaffordable_actions() {
        let registry = ModRegistry::from_mods(vec![]);
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
            plugin_dispatch: None,
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
}
