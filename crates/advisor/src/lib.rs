//! # poc2-advisor
//!
//! Beam-search optimal-path advisor.
//!
//! Given:
//! - Current item state
//! - [`Goal`] (target mods, abandon criteria, budget)
//! - [`Stash`] (currencies/omens the user owns)
//! - Patch version
//! - Risk preference (slider, cautious ↔ greedy)
//!
//! Produces:
//! - Top-N [`Recommendation`]s, ranked by utility
//! - Each recommendation cites its source rule/strategy/heuristic
//! - Recovery branches surfaced when step failures are likely
//!
//! ## Algorithm summary (per [ADR-0007](../../../docs/adr/0007-advisor-beam-search.md))
//!
//! 1. **Generate candidates** from rules + strategies + heuristics.
//! 2. **Beam-search** over candidate sequences (configurable width / depth).
//! 3. **Score nodes** via probability × cost (risk-adjusted).
//! 4. **Group by first action**, return top-N.
//!
//! ## Public API
//!
//! - [`plan`] — full planner entry point.
//! - [`recommend_quick`] — single-depth (rules-only) fast path; useful
//!   for the streaming UI's "first result in 200ms" guarantee.

#![forbid(unsafe_code)]
#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_sign_loss)]

pub mod action;
pub mod candidate;
pub mod goal;
pub mod planner;
pub mod recommendation;
pub mod recovery;
pub mod scorer;
pub mod simulator;
pub mod stash;

pub use action::{from_rule_action, from_strategy_action, AdvisorAction};
pub use candidate::{generate_candidates, Candidate};
pub use goal::{is_satisfied, should_abandon, Goal};
pub use planner::{plan, BeamConfig, PlanInput};
pub use recommendation::{Recommendation, RecommendationSource};
pub use recovery::collect_strategy_hints;
pub use scorer::{action_cost, score, ScoringWeights};
pub use simulator::{simulate, SimulationOutcome};
pub use stash::Stash;

use poc2_engine::currency::CurrencyResolver;
use poc2_engine::item::Item;
use poc2_engine::patch::PatchVersion;
use poc2_engine::registry::ModRegistry;
use poc2_market::Valuator;
use poc2_rules::RuleSet;
use poc2_strategies::StrategyRegistry;

/// Quick single-depth recommendation. Equivalent to a beam-search of
/// width = top_n and depth = 1. Usable for "first results in <200ms" UI.
#[must_use]
#[allow(clippy::too_many_arguments)] // public API mirrors planner inputs
pub fn recommend_quick(
    item: &Item,
    goal: &Goal,
    rules: &RuleSet,
    strategies: &StrategyRegistry,
    registry: &ModRegistry,
    resolver: &dyn CurrencyResolver,
    valuator: &Valuator,
    stash: &Stash,
    patch: PatchVersion,
    risk: f64,
    top_n: u32,
) -> Vec<Recommendation> {
    let input = PlanInput {
        item: item.clone(),
        goal: goal.clone(),
        rules,
        strategies,
        registry,
        resolver,
        valuator,
        stash,
        patch,
        config: BeamConfig {
            width: top_n.max(3),
            depth: 1,
            risk,
            top_n,
            seed: 0,
            weights: ScoringWeights::default(),
        },
    };
    plan(&input)
}
