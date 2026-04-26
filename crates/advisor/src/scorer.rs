//! Utility scoring for advisor candidates.
//!
//! The scorer assigns a real-valued utility to a `(plan_node, candidate)`
//! pair so the planner can rank and prune. Per ADR-0007:
//!
//! ```text
//! utility = success_prob - lambda * cost.risk_adjusted(risk) - mu * Var(cost)
//! ```
//!
//! Where:
//! - `success_prob` is the planner's estimate of reaching the goal from
//!   the post-action state.
//! - `cost` is the divine-equivalent cost of taking the action (one
//!   apply, plus omens), pulled from [`Valuator`].
//! - `risk` is the user's slider in `[0, 1]`: 0 = pessimistic (use the
//!   cost band's max), 1 = optimistic (use the cost band's expected).
//!   Note this differs from M5.2's `DivEquiv::risk_adjusted` which lerps
//!   `expected -> max` as risk grows; we invert it here so a "greedy"
//!   user (risk=1) gets the cheapest plan.
//! - `lambda` weights cost against probability. Default 1.0.
//! - `mu` adds variance penalty. Default 0.05.

use poc2_engine::ids::CurrencyId;
use poc2_market::{DivEquiv, Valuator};

use crate::action::AdvisorAction;

/// Tuneable weights for the utility function. Default values picked by
/// hand; M4.4 will tune these against the canonical test fixture.
#[derive(Debug, Clone, Copy)]
pub struct ScoringWeights {
    /// How much we trade off cost against probability. Higher → cost
    /// matters more.
    pub lambda: f64,
    /// Variance penalty (cost band width as proxy).
    pub mu: f64,
    /// Bonus added per matched concept on the post-state, scaled by
    /// inverse of count needed. Encourages the planner to value
    /// "made progress toward the target" even when the target isn't
    /// fully met yet.
    pub progress_bonus: f64,
    /// Per-rule prior weight: scales the rule/strategy/heuristic prior
    /// into the score.
    pub prior_weight: f64,
}

impl Default for ScoringWeights {
    fn default() -> Self {
        Self {
            lambda: 1.0,
            mu: 0.05,
            progress_bonus: 0.5,
            prior_weight: 0.4,
        }
    }
}

/// Compute the divine-equivalent cost of one application of an action.
///
/// Sums the currency cost and the cost of every omen in the action.
/// Unknown ids contribute zero — the advisor surfaces a "missing price"
/// warning upstream.
#[must_use]
pub fn action_cost(action: &AdvisorAction, valuator: &Valuator) -> DivEquiv {
    let mut total = DivEquiv::ZERO;
    if let AdvisorAction::ApplyCurrency { currency, omens } = action {
        if let Some(c) = valuator.get(currency) {
            total = total.plus(c);
        }
        for o in omens {
            // Omens share the namespace via `OmenId`. The valuator stores
            // them as plain currencies in M5.2; reuse that lookup.
            if let Some(c) = valuator.get(&CurrencyId::from(o.as_str())) {
                total = total.plus(c);
            }
        }
    } else if let AdvisorAction::ApplyHinekorasLock = action {
        if let Some(c) = valuator.get(&CurrencyId::from("HinekorasLock")) {
            total = total.plus(c);
        }
    }
    // Reveal / Stop / Abandon / Guidance are free.
    total
}

/// Score a candidate. Higher = better.
#[must_use]
pub fn score(
    success_prob: f64,
    cost: DivEquiv,
    prior: f64,
    risk: f64,
    weights: ScoringWeights,
) -> f64 {
    let risk_clamped = risk.clamp(0.0, 1.0);
    // Invert: a "greedy" user (risk=1) wants the optimistic cost
    // (= expected); a cautious user (risk=0) wants worst-case (= max).
    let cost_point = cost.risk_adjusted(1.0 - risk_clamped);
    let cost_band_width = (cost.max - cost.min).max(0.0);
    success_prob - weights.lambda * cost_point - weights.mu * cost_band_width
        + weights.prior_weight * prior
}

#[cfg(test)]
mod tests {
    use super::*;
    use poc2_engine::ids::OmenId;

    #[test]
    fn cost_for_apply_currency_pulls_valuator() {
        let v = Valuator::default();
        let action = AdvisorAction::ApplyCurrency {
            currency: CurrencyId::from("DivineOrb"),
            omens: vec![],
        };
        let c = action_cost(&action, &v);
        // Divine = 1.0 div by definition.
        assert!((c.expected - 1.0).abs() < 1e-9);
    }

    #[test]
    fn cost_includes_omens() {
        let mut v = Valuator::default();
        // Add a known-priced omen.
        v.set(
            CurrencyId::from("OmenOfDextralExaltation"),
            DivEquiv {
                min: 0.5,
                expected: 1.0,
                max: 2.0,
            },
        );
        let action = AdvisorAction::ApplyCurrency {
            currency: CurrencyId::from("DivineOrb"),
            omens: vec![OmenId::from("OmenOfDextralExaltation")],
        };
        let c = action_cost(&action, &v);
        // Divine (1.0) + omen (1.0) = 2.0 expected.
        assert!((c.expected - 2.0).abs() < 1e-9);
    }

    #[test]
    fn cost_zero_for_terminal_actions() {
        let v = Valuator::default();
        let stop = action_cost(&AdvisorAction::Stop, &v);
        let abandon = action_cost(&AdvisorAction::Abandon { reason: "x".into() }, &v);
        assert!(stop.expected.abs() < 1e-9);
        assert!(abandon.expected.abs() < 1e-9);
    }

    #[test]
    fn higher_success_prob_scores_higher_when_cost_equal() {
        let cost = DivEquiv::point(1.0);
        let w = ScoringWeights::default();
        let lo = score(0.1, cost, 0.5, 0.5, w);
        let hi = score(0.9, cost, 0.5, 0.5, w);
        assert!(hi > lo);
    }

    #[test]
    fn lower_cost_scores_higher_when_prob_equal() {
        let cheap = DivEquiv::point(1.0);
        let pricey = DivEquiv::point(10.0);
        let w = ScoringWeights::default();
        let s_cheap = score(0.5, cheap, 0.5, 0.5, w);
        let s_pricey = score(0.5, pricey, 0.5, 0.5, w);
        assert!(s_cheap > s_pricey);
    }

    #[test]
    fn risk_slider_swings_cost_toward_min_at_zero_or_max_at_one() {
        let band = DivEquiv {
            min: 1.0,
            expected: 5.0,
            max: 100.0,
        };
        let w = ScoringWeights::default();
        // Cautious (risk=0): use max → low score.
        // Greedy (risk=1): use expected → higher score.
        let cautious = score(0.5, band, 0.5, 0.0, w);
        let greedy = score(0.5, band, 0.5, 1.0, w);
        assert!(greedy > cautious);
    }
}
