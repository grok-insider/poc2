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
pub use candidate::{generate_candidates, generate_candidates_with_goal, Candidate};
pub use goal::{is_satisfied, should_abandon, Goal};
pub use planner::{plan, BeamConfig, PlanInput};
pub use recommendation::{Recommendation, RecommendationSource};
pub use recovery::collect_strategy_hints;
pub use scorer::{action_cost, count_keepers, occupancy_adjustment, score, ScoringWeights};
pub use simulator::{simulate, simulate_n, McOutcome, SimulationOutcome};
pub use stash::Stash;

// StreamingProgress + plan_streaming exported above via `pub fn`.

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
        plugin_dispatch: None,
        config: BeamConfig {
            width: top_n.max(3),
            depth: 1,
            risk,
            top_n,
            seed: 0,
            mc_samples: 1, // quick path skips MC
            weights: ScoringWeights::default(),
        },
    };
    plan(&input)
}

/// One streaming progress event. Emitted by [`plan_streaming`] at each
/// depth boundary so the UI can render progressively-better
/// recommendations.
#[derive(Debug, Clone)]
pub struct StreamingProgress {
    /// The depth that just completed.
    pub depth: u32,
    /// Recommendations at this depth (top-N as configured).
    pub recommendations: Vec<Recommendation>,
    /// True iff this is the final emission (`depth >= max_depth`).
    pub is_final: bool,
}

/// Run the planner at progressively-deeper depths (1 → 3 → 8 by
/// default), emitting `StreamingProgress` after each beam search.
///
/// This is the synchronous worker the Tauri layer wraps in a
/// `spawn_blocking` task. Cancellation is cooperative: the caller drops
/// the receiver to stop further sends.
///
/// `depths`: the depths to run at, in order. Defaults to `[1, 3, 8]`
/// when empty. Each entry overrides the input's `config.depth`.
#[allow(clippy::too_many_arguments)] // mirrors PlanInput shape
pub fn plan_streaming(
    input: &PlanInput<'_>,
    depths: &[u32],
    mut emit: impl FnMut(StreamingProgress),
) {
    let depth_list: &[u32] = if depths.is_empty() {
        &[1, 3, 8]
    } else {
        depths
    };
    let max_depth = *depth_list.iter().max().unwrap_or(&1);
    for (i, &d) in depth_list.iter().enumerate() {
        let mut local_input = PlanInput {
            item: input.item.clone(),
            goal: input.goal.clone(),
            rules: input.rules,
            strategies: input.strategies,
            registry: input.registry,
            resolver: input.resolver,
            valuator: input.valuator,
            stash: input.stash,
            patch: input.patch,
            plugin_dispatch: input.plugin_dispatch,
            config: input.config,
        };
        local_input.config.depth = d;
        // Earlier depths skip MC for snappy first-paint; the deepest run
        // uses the user's configured mc_samples.
        if i + 1 < depth_list.len() {
            local_input.config.mc_samples = local_input.config.mc_samples.min(5);
        }
        let recs = plan(&local_input);
        emit(StreamingProgress {
            depth: d,
            recommendations: recs,
            is_final: d >= max_depth || i + 1 == depth_list.len(),
        });
    }
}
