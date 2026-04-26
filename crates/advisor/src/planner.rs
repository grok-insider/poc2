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
use poc2_strategies::{PredicateContext, StrategyRegistry};

use crate::action::AdvisorAction;
use crate::candidate::{generate_candidates, Candidate};
use crate::goal::{is_satisfied_with_ctx, should_abandon_with_ctx, Goal};
use crate::recommendation::{Recommendation, RecommendationSource};
use crate::scorer::{action_cost, score, ScoringWeights};
use crate::simulator::{simulate, SimulationOutcome};
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
    /// Joint success probability of getting here (product of step probs).
    accumulated_prob: f64,
    /// Action sequence we took. `path[0]` is the first action.
    path: Vec<AdvisorAction>,
    /// Source of `path[0]` — used to label the resulting Recommendation.
    first_source: Option<RecommendationSource>,
    /// Rationale of `path[0]`.
    first_rationale: String,
    /// Prior of `path[0]` candidate.
    first_prior: f64,
    /// Already-terminated path? (Stop / Abandon / goal-satisfied / abandon-criteria-fired)
    terminated: bool,
}

impl PlanNode {
    fn root(item: Item) -> Self {
        Self {
            item,
            accumulated_cost: DivEquiv::ZERO,
            accumulated_prob: 1.0,
            path: Vec::new(),
            first_source: None,
            first_rationale: String::new(),
            first_prior: 1.0,
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
}

/// Build a [`PredicateContext`] for `item` against the planner inputs +
/// the per-node accumulated cost. Predicates that reference cost / stash
/// / valuator data evaluate against this context; everything else
/// continues to read the item directly.
fn ctx_for_node<'a>(input: &'a PlanInput<'a>, accumulated_cost: DivEquiv) -> PredicateContext<'a> {
    PredicateContext::new(input.registry)
        .with_cost(accumulated_cost.expected)
        .with_valuator(input.valuator)
        .with_stash(input.stash)
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
            score: f64::INFINITY,
            rationale: "Item already satisfies the target; stop and equip or sell.".into(),
            depth: 0,
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

            let cands = generate_candidates(
                &node.item,
                &node_ctx,
                input.rules,
                input.strategies,
                input.stash,
                input.patch,
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

/// Build the child node by simulating `cand` against `node.item`.
fn expand_with_candidate(
    node: &PlanNode,
    cand: &Candidate,
    depth: u32,
    input: &PlanInput<'_>,
) -> PlanNode {
    // Clone node, append action.
    let mut next = node.clone();
    let omens = OmenSet::new();
    let outcome: SimulationOutcome = simulate(
        &node.item,
        &cand.action,
        &omens,
        input.registry,
        input.resolver,
        input.patch,
        input
            .config
            .seed
            .wrapping_add(u64::from(depth))
            .wrapping_add(node.path.len() as u64),
    );
    next.item = outcome.item;
    let action_cost_band = action_cost(&cand.action, input.valuator);
    next.accumulated_cost = next.accumulated_cost.plus(action_cost_band);
    let step_prob = if outcome.success { 0.95 } else { 0.05 };
    next.accumulated_prob *= step_prob;
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
    score(
        success_prob,
        node.accumulated_cost,
        node.first_prior,
        cfg.risk,
        cfg.weights,
    )
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
    Recommendation {
        action,
        source,
        expected_cost: cost,
        expected_prob: node.accumulated_prob,
        score: score_value,
        rationale: node.first_rationale.clone(),
        depth: node.path.len() as u32,
    }
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
        let top = &recs[0];
        assert!(matches!(
            &top.action,
            AdvisorAction::ApplyCurrency { currency, .. }
                if currency.as_str() == "PerfectOrbOfTransmutation"
        ));
        assert!(
            matches!(&top.source, RecommendationSource::Rule { id, .. } if id.starts_with("R001"))
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
