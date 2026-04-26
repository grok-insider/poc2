//! Geometric distribution helpers.
//!
//! Probability tools for the most common crafting question: **"How many
//! attempts to first success when each attempt is independent with
//! probability p?"** This shows up everywhere:
//!
//! - Chaos-spam: each Chaos has probability `p = target_weight /
//!   total_weight_at_ilvl` of producing the target mod
//! - Annul + Augment spam: `p = target_weight / total_weight` per cycle
//! - Reveal-at-Well-of-Souls: `p = num_target_options / 3` (or `/6` with
//!   Abyssal Echoes)
//! - Fracturing Orb: `p = num_target_visible_mods / total_visible_mods`
//! - Vaal corruption favorable outcome: `p = 1/6` (1/5 with Omen of
//!   Corruption)
//!
//! ## Cost-aware EV
//!
//! When each attempt costs `c` divines, expected total cost to first
//! success is `c / p` (the geometric distribution's mean times unit cost).
//! When the budget is bounded, [`prob_within_budget`] gives the cdf at
//! `n = budget / c` attempts.

/// Expected attempts to first success in a Bernoulli sequence with
/// per-trial probability `p`. Returns `None` for `p <= 0` or `p > 1`.
///
/// `E[X] = 1 / p` for `X ~ Geom(p)` with support `{1, 2, 3, ...}`.
#[must_use]
pub fn expected_attempts(p: f64) -> Option<f64> {
    if p <= 0.0 || p > 1.0 {
        return None;
    }
    Some(1.0 / p)
}

/// Probability of at least one success in `n` independent trials with
/// per-trial probability `p`. Returns `None` for invalid inputs.
///
/// `P(X <= n) = 1 - (1 - p)^n`.
#[must_use]
pub fn prob_within_budget(p: f64, n: u32) -> Option<f64> {
    if !(0.0..=1.0).contains(&p) {
        return None;
    }
    if p == 0.0 {
        return Some(0.0);
    }
    let q = (1.0 - p).powi(n.try_into().ok()?);
    Some(1.0 - q)
}

/// Number of attempts `n` such that the cumulative success probability
/// reaches `target`. Returns `None` if unreachable.
///
/// `n >= log(1 - target) / log(1 - p)` (ceil).
#[must_use]
pub fn attempts_for_confidence(p: f64, target: f64) -> Option<u32> {
    if p <= 0.0 || p > 1.0 || !(0.0..1.0).contains(&target) {
        return None;
    }
    if target == 0.0 {
        return Some(0);
    }
    let log_q = (1.0 - p).ln();
    let log_remain = (1.0 - target).ln();
    let raw = log_remain / log_q;
    let n = raw.ceil();
    if !n.is_finite() || n < 0.0 {
        return None;
    }
    // Cap at u32::MAX to avoid panic on extreme inputs.
    let capped = n.min(f64::from(u32::MAX));
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Some(capped as u32)
}

/// Expected divine cost to first success when each trial costs
/// `cost_per_trial` divines. `None` for invalid `p`.
#[must_use]
pub fn expected_cost(p: f64, cost_per_trial: f64) -> Option<f64> {
    expected_attempts(p).map(|e| e * cost_per_trial)
}

/// Combined probability of finishing under a divine budget. Equivalent to
/// `prob_within_budget(p, floor(budget / cost_per_trial))`.
#[must_use]
pub fn prob_finish_under_budget(p: f64, cost_per_trial: f64, budget_div: f64) -> Option<f64> {
    if cost_per_trial <= 0.0 || budget_div < 0.0 {
        return None;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let n = (budget_div / cost_per_trial).floor() as u32;
    prob_within_budget(p, n)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f64, b: f64, eps: f64) -> bool {
        (a - b).abs() < eps
    }

    #[test]
    fn expected_attempts_basic() {
        assert_eq!(expected_attempts(0.0), None);
        assert_eq!(expected_attempts(-0.1), None);
        assert_eq!(expected_attempts(1.5), None);
        assert!(approx(expected_attempts(0.5).unwrap(), 2.0, 1e-9));
        assert!(approx(expected_attempts(0.1).unwrap(), 10.0, 1e-9));
        assert!(approx(expected_attempts(1.0).unwrap(), 1.0, 1e-9));
    }

    #[test]
    fn prob_within_budget_basic() {
        // p=0.5, n=2 → 1 - 0.25 = 0.75
        assert!(approx(prob_within_budget(0.5, 2).unwrap(), 0.75, 1e-9));
        // p=0.1, n=10 → ~0.6513
        assert!(approx(prob_within_budget(0.1, 10).unwrap(), 0.6513, 1e-3));
        // p=0 always 0
        assert!(approx(prob_within_budget(0.0, 100).unwrap(), 0.0, 1e-9));
    }

    #[test]
    fn attempts_for_confidence_basic() {
        // p=0.5, target=0.99: need ~6.65 → 7
        assert_eq!(attempts_for_confidence(0.5, 0.99).unwrap(), 7);
        // p=0.1, target=0.5: need ~6.58 → 7
        assert_eq!(attempts_for_confidence(0.1, 0.5).unwrap(), 7);
        // p=0.001, target=0.95: need ~2995 → 2995
        assert!(attempts_for_confidence(0.001, 0.95).unwrap() > 2900);
    }

    #[test]
    fn cost_helpers() {
        // p=0.1, 0.5 div per trial → 5 div expected
        assert!(approx(expected_cost(0.1, 0.5).unwrap(), 5.0, 1e-9));
        // p=0.1, 0.5 div per trial, 10 div budget → n=20, ~0.878
        let r = prob_finish_under_budget(0.1, 0.5, 10.0).unwrap();
        assert!(approx(r, 0.8784, 1e-3));
    }

    #[test]
    fn invalid_inputs_return_none() {
        assert!(prob_within_budget(-0.1, 5).is_none());
        assert!(prob_within_budget(1.1, 5).is_none());
        assert!(attempts_for_confidence(0.5, 1.0).is_none());
        assert!(attempts_for_confidence(0.0, 0.5).is_none());
        assert!(prob_finish_under_budget(0.5, 0.0, 10.0).is_none());
    }
}
