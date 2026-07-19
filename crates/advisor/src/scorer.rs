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
use poc2_engine::item::{AffixType, Item};
use poc2_engine::registry::ModRegistry;
use poc2_market::{DivEquiv, Valuator};

use crate::action::AdvisorAction;
use crate::goal::Goal;

/// Tuneable weights for the utility function. Default values picked by
/// hand; M4.4 will tune these against the canonical test fixture.
#[derive(Debug, Clone, Copy)]
pub struct ScoringWeights {
    /// How much we trade off cost against probability. Higher → cost
    /// matters more.
    pub lambda: f64,
    /// Variance penalty (cost band width as proxy).
    pub mu: f64,
    /// Weight on goal-progress: multiplied by the terminal node's
    /// fraction-of-target-specs-satisfied (`[0, 1]`) and ADDED to the score in
    /// [`crate::planner`]'s `score_node`, on top of the multiplicative
    /// reliability×progress attainment term. The multiplicative term already
    /// zeroes out no-progress actions; this additive booster makes the planner
    /// prefer the building step that reaches MORE of the target even when it's
    /// the riskier path.
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
            // Goal-progress is the primary ranking signal among building actions
            // (a +1-spec gain ⇒ +progress_bonus/total, which outweighs the
            // ≤0.36 prior gap), while λ·cost still discriminates within a tier.
            progress_bonus: 1.0,
            prior_weight: 0.4,
        }
    }
}

// =========================================================================
// Concept-occupancy heuristic (Phase B.1)
// =========================================================================

/// Per-affix-slot count of mods on `item` that satisfy any of the goal's
/// target specs for that slot ("keepers"). The numbers feed
/// [`occupancy_adjustment`] which boosts protective actions and
/// penalises risky ones.
///
/// Returns `(prefix_keepers, suffix_keepers)`.
#[must_use]
pub fn count_keepers(item: &Item, goal: &Goal, registry: &ModRegistry) -> (u8, u8) {
    let mut prefix = 0u8;
    let mut suffix = 0u8;
    // For each currently-rolled mod, see if it satisfies any of the
    // goal's target specs on its side.
    for roll in &item.prefixes {
        if roll_matches_any_target(&goal.target.prefixes, roll, registry, AffixType::Prefix) {
            prefix = prefix.saturating_add(1);
        }
    }
    for roll in &item.suffixes {
        if roll_matches_any_target(&goal.target.suffixes, roll, registry, AffixType::Suffix) {
            suffix = suffix.saturating_add(1);
        }
    }
    (prefix, suffix)
}

fn roll_matches_any_target(
    specs: &[poc2_strategies::TargetSpec],
    roll: &poc2_engine::ModRoll,
    registry: &ModRegistry,
    affix: AffixType,
) -> bool {
    let Some(def) = registry.get(&roll.mod_id) else {
        return false;
    };
    for spec in specs {
        if let Some(want_affix) = spec.affix {
            if want_affix != affix {
                continue;
            }
        }
        let concept_match = if let Some(c) = &spec.concept {
            def.concept_set.iter().any(|x| x == c)
        } else if !spec.concept_any.is_empty() {
            def.concept_set
                .iter()
                .any(|x| spec.concept_any.iter().any(|y| x == y))
        } else {
            // Spec has no concept restriction → any mod satisfies it.
            true
        };
        if concept_match {
            if !spec.allow_hybrid && def.is_hybrid() {
                continue;
            }
            return true;
        }
    }
    false
}

/// Score adjustment for an action's effect on existing keepers.
///
/// Returns a delta to add to the base score. Positive when the action
/// *protects* keepers; negative when it puts them at risk.
///
/// Heuristics (mirroring `docs/80-crafter-helper-v2-plan.md` §7.B.1):
///
/// - **Regal on Magic with two keepers** — high penalty. Regal might
///   roll onto the locked side and freeze the displacement vector.
/// - **Aug on Magic with one keeper, empty other side** — small boost.
///   Adds a mod without disturbing the keeper.
/// - **Annul on Magic/Rare with keepers** — penalty proportional to
///   keeper count (Annul is a uniform random non-fractured remove).
/// - **Chaos on Rare with multiple keepers** — penalty (Chaos removes
///   one then adds one; expected loss of one keeper per roll).
/// - **Essence on item with empty target slot** — boost (locks one mod
///   in a deterministic way without touching keepers).
/// - **Divine / Fracture / Lock** — neutral; tier-fix bonus comes from
///   [`tier_fix_adjustment`] in B.2.
#[must_use]
pub fn occupancy_adjustment(
    action: &AdvisorAction,
    item: &Item,
    goal: &Goal,
    registry: &ModRegistry,
) -> f64 {
    let AdvisorAction::ApplyCurrency { currency, .. } = action else {
        return 0.0;
    };
    let (pre, suf) = count_keepers(item, goal, registry);
    let id = currency.as_str();
    match id {
        // Regal: promotes Magic → Rare, adds 1 random mod. The new mod
        // can roll on either side; if both Magic slots are keepers, the
        // new mod adds to the kept-but-now-shared rare pool — net
        // negative because the third slot consumes the user's beam.
        "RegalOrb" | "GreaterRegalOrb" | "PerfectRegalOrb" => {
            if pre + suf >= 2 {
                -0.6
            } else if pre + suf == 1 {
                -0.2
            } else {
                0.0
            }
        }
        // Augment: adds 1 mod on the empty side of a Magic. Pure boost
        // when one keeper is locked.
        "OrbOfAugmentation" | "GreaterOrbOfAugmentation" | "PerfectOrbOfAugmentation" => {
            if pre + suf == 1 {
                0.4
            } else {
                0.1
            }
        }
        // Annul: removes one random non-fractured mod. Penalty scales
        // with keeper density.
        "OrbOfAnnulment" => {
            let total_mods = item.prefixes.len() as f64 + item.suffixes.len() as f64;
            if total_mods <= 0.0 {
                0.0
            } else {
                let keeper_total = f64::from(pre) + f64::from(suf);
                -0.8 * (keeper_total / total_mods)
            }
        }
        // Chaos: remove + add. Expected loss of one keeper if any are
        // present and not fractured.
        "ChaosOrb" | "GreaterChaosOrb" | "PerfectChaosOrb" => {
            let total_mods = item.prefixes.len() as f64 + item.suffixes.len() as f64;
            if total_mods <= 0.0 {
                0.0
            } else {
                let keeper_total = f64::from(pre) + f64::from(suf);
                -0.5 * (keeper_total / total_mods)
            }
        }
        // Exalt: adds 1 mod to a Rare. Neutral when there's still room.
        "ExaltedOrb" | "GreaterExaltedOrb" | "PerfectExaltedOrb" => 0.0,
        // Essence (any tier) is detected by the id starting with "Essence"
        // / "Lesser…" / "Greater…" / "Perfect…" / "Corrupted…" prefix
        // followed by "EssenceOf".
        s if is_essence_id(s) => {
            // Essence locks one specific mod into one specific side. If
            // applied with the keeper side already full, it forces the
            // locked mod onto the empty side — protective.
            if pre + suf >= 1 {
                0.5
            } else {
                0.2
            }
        }
        _ => 0.0,
    }
}

fn is_essence_id(s: &str) -> bool {
    s.starts_with("EssenceOf")
        || s.starts_with("LesserEssenceOf")
        || s.starts_with("GreaterEssenceOf")
        || s.starts_with("PerfectEssenceOf")
        || s.starts_with("CorruptedEssenceOf")
}

/// Compute the divine-equivalent cost of one application of an action.
///
/// Sums the currency cost and the cost of every omen in the action.
/// Unknown ids contribute zero — the advisor surfaces a "missing price"
/// warning upstream.
///
/// Phase B.6: a `Reveal` carrying `bone` + `omen` charges the bone's
/// price plus the omen's price (when set). Reveals without a bone
/// remain zero-cost — they're abstract "use a bone" hints from the
/// strategy DSL.
#[must_use]
pub fn action_cost(action: &AdvisorAction, valuator: &Valuator) -> DivEquiv {
    let mut total = DivEquiv::ZERO;
    match action {
        AdvisorAction::ApplyCurrency { currency, omens } => {
            if let Some(c) = valuator.get(currency) {
                total = total.plus(c);
            }
            for o in omens {
                if let Some(c) = valuator.get(&CurrencyId::from(o.as_str())) {
                    total = total.plus(c);
                }
            }
        }
        AdvisorAction::ApplyHinekorasLock => {
            if let Some(c) = valuator.get(&CurrencyId::from("HinekorasLock")) {
                total = total.plus(c);
            }
        }
        AdvisorAction::Reveal { bone, omen, .. } => {
            if let Some(b) = bone {
                if let Some(c) = valuator.get(b) {
                    total = total.plus(c);
                }
            }
            if let Some(o) = omen {
                if let Some(c) = valuator.get(&CurrencyId::from(o.as_str())) {
                    total = total.plus(c);
                }
            }
        }
        // Stop / Abandon / Guidance / Recombine / ActivateOmen / Recurring are free
        // at the leaf level. (Recurring's cost is computed at the
        // outer Recommendation level via the LoopEstimate.)
        _ => {}
    }
    total
}

/// Score a candidate. Higher = better.
///
/// The `risk` parameter is a `[0, 1]` slider that re-weights the
/// variance penalty per Phase B.3 of the v2 plan:
///
/// - **`risk < 0.3` (cautious)** — variance penalty is amplified ×3,
///   pushing high-variance options below their deterministic siblings
///   even when they have higher expected progress.
/// - **`0.3 ≤ risk ≤ 0.7` (balanced)** — variance penalty is the
///   nominal `mu * band_width` blend.
/// - **`risk > 0.7` (greedy)** — variance penalty fades out (×0.3 at
///   risk=1.0), so the score reduces to "raw expected progress minus
///   cost". Maximises chance of upside per orb.
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
    let variance_weight = variance_weight_for_risk(risk_clamped);
    success_prob - weights.lambda * cost_point - weights.mu * variance_weight * cost_band_width
        + weights.prior_weight * prior
}

/// Map the user's risk slider to the variance-penalty multiplier
/// per Phase B.3. Designed so the cautious band (< 0.3) makes
/// variance dominate, the balanced band keeps it nominal, and the
/// greedy band (> 0.7) effectively ignores variance.
fn variance_weight_for_risk(risk: f64) -> f64 {
    if risk < 0.3 {
        3.0
    } else if risk > 0.7 {
        // Linear ramp from 1.0 at risk=0.7 to 0.3 at risk=1.0.
        let t = ((risk - 0.7) / 0.3).clamp(0.0, 1.0);
        1.0 - 0.7 * t
    } else {
        1.0
    }
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
