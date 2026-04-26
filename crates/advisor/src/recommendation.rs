//! [`Recommendation`] — what the advisor returns to the UI.
//!
//! Each recommendation is one ranked suggestion: the action to take,
//! the source that produced it (rule, strategy, or fallback heuristic),
//! a divine-equivalent cost band, an estimated success probability, and
//! a human-readable rationale.
//!
//! The advisor returns `Vec<Recommendation>` sorted by `score` descending
//! (highest utility first). The UI typically renders the top 3-5.

use poc2_market::DivEquiv;
use serde::{Deserialize, Serialize};

use crate::action::AdvisorAction;

/// Where this recommendation originated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RecommendationSource {
    /// Emitted by a rule in the [`poc2_rules`] catalogue.
    Rule {
        /// Rule id (stable string from the seed catalogue or loaded TOML).
        id: String,
        /// Rule confidence band (verified / community / experimental).
        confidence: poc2_rules::Confidence,
    },
    /// Emitted by a strategy in the [`poc2_strategies`] library.
    Strategy {
        /// Strategy id.
        id: String,
        /// Step id within the strategy that produced this action.
        step: String,
    },
    /// Emitted by a hard-coded heuristic in the advisor itself
    /// (e.g., the "transmute on Normal" fallback).
    Heuristic { name: String },
}

/// One ranked recommendation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Recommendation {
    /// The action proposed.
    pub action: AdvisorAction,
    /// Where the action came from (rule / strategy / heuristic).
    pub source: RecommendationSource,
    /// Divine-equivalent cost band of taking the action ONCE
    /// (from [`poc2_market::Valuator`]).
    pub expected_cost: DivEquiv,
    /// Estimated success probability of *this single step* in `[0, 1]`.
    /// 1.0 for non-probabilistic actions (Hinekora's Lock, Stop, Abandon).
    pub expected_prob: f64,
    /// Final utility score the planner used to rank this. Higher = better.
    pub score: f64,
    /// Human-readable explanation surfaced in the UI.
    pub rationale: String,
    /// Beam-search depth at which this recommendation was found.
    /// Depth 1 = immediate; deeper = found via lookahead.
    pub depth: u32,
}

impl Recommendation {
    /// True if the action is the same currency+omens combo as `other`.
    /// Used by the planner to deduplicate beam frontier results that
    /// converge on the same first move.
    #[must_use]
    pub fn shares_first_action(&self, other: &Recommendation) -> bool {
        self.action == other.action
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use poc2_engine::ids::CurrencyId;

    #[test]
    fn recommendation_round_trips_through_serde() {
        let r = Recommendation {
            action: AdvisorAction::ApplyCurrency {
                currency: CurrencyId::from("ChaosOrb"),
                omens: vec![],
            },
            source: RecommendationSource::Rule {
                id: "R001-test".into(),
                confidence: poc2_rules::Confidence::Verified,
            },
            expected_cost: DivEquiv::point(0.125),
            expected_prob: 0.5,
            score: 4.0,
            rationale: "Chaos spam toward target.".into(),
            depth: 1,
        };
        let s = serde_json::to_string(&r).unwrap();
        let back: Recommendation = serde_json::from_str(&s).unwrap();
        assert_eq!(back, r);
    }
}
