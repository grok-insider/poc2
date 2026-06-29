//! [`Recommendation`] — what the advisor returns to the UI.
//!
//! Each recommendation is one ranked suggestion: the action to take,
//! the source that produced it (rule, strategy, or fallback heuristic),
//! a divine-equivalent cost band, an estimated success probability, and
//! a human-readable rationale.
//!
//! The advisor returns `Vec<Recommendation>` sorted by `score` descending
//! (highest utility first). The UI typically renders the top 3-5.

use poc2_engine::ids::ConceptId;
use poc2_engine::item::AffixType;
use poc2_market::DivEquiv;
use serde::{Deserialize, Serialize};

use crate::action::AdvisorAction;

// =========================================================================
// Stop predicates (Phase B.5)
// =========================================================================

/// One concept the user wants present at a minimum tier.
///
/// Used by [`StopPredicate`] to describe when a recurring step
/// (Annul + Chaos loop, Greater Essence chain, etc.) should stop.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ConceptCriterion {
    /// Concept this criterion targets (e.g., `EnergyShield`,
    /// `FireResistance`).
    pub concept: ConceptId,
    /// Tier ladder index — `1` means "any tier", larger numbers force
    /// progressively higher-rolled mods. Values follow `min_tier` semantics
    /// from `Goal.target.prefixes[].min_tier` so the same predicate engine
    /// can decide goal-met and stop-loop.
    pub min_tier: u8,
    /// Restrict this criterion to a specific affix slot. `None` matches
    /// either prefix or suffix.
    #[serde(default)]
    pub affix: Option<AffixType>,
}

/// Stop condition for a recurring step. The loop exits as soon as
/// **all** criteria are simultaneously satisfied, or `max_mods` (when
/// set) is reached, whichever comes first.
///
/// Surfaced in the UI as a friendly list ("Stop when: T1 ES on prefix
/// AND T1 Cold Resistance on suffix").
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct StopPredicate {
    /// Conjunction of concept-tier-affix criteria that must hold.
    #[serde(default)]
    pub concepts: Vec<ConceptCriterion>,
    /// Optional cap on visible-mod count. Used for "stop when item has 4
    /// mods" guards. `None` means uncapped.
    #[serde(default)]
    pub max_mods: Option<u8>,
}

impl StopPredicate {
    /// Empty predicate — never satisfied; useful for tests where the
    /// loop is expected to run for the configured iteration cap.
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// Convenience: render a one-line "Stop when: ..." summary the UI
    /// can print directly above the recurring-step card. Returns
    /// `"Stop when: never"` for the empty predicate.
    #[must_use]
    pub fn summary(&self) -> String {
        if self.concepts.is_empty() && self.max_mods.is_none() {
            return "Stop when: never".into();
        }
        let mut parts: Vec<String> = self
            .concepts
            .iter()
            .map(|c| {
                let affix = match c.affix {
                    Some(AffixType::Prefix) => " on prefix",
                    Some(AffixType::Suffix) => " on suffix",
                    Some(AffixType::Implicit) => " on implicit",
                    Some(AffixType::Enchantment) => " on enchantment",
                    None => "",
                };
                format!("T{}+ {}{}", c.min_tier, c.concept.as_str(), affix)
            })
            .collect();
        if let Some(cap) = self.max_mods {
            parts.push(format!("≤ {cap} visible mods"));
        }
        format!("Stop when: {}", parts.join(" AND "))
    }
}

// =========================================================================
// Recurring-step iteration estimate (Phase B.4)
// =========================================================================

/// Estimated iterations and total cost for a [`AdvisorAction::Recurring`]
/// step. Computed via Monte Carlo against the inner sequence's
/// per-step success probability; the stderr lets the UI show
/// "this loop runs 8 ± 3 times".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoopEstimate {
    /// Mean iteration count to reach the stop predicate.
    #[serde(default)]
    pub mean_iterations: f64,
    /// Standard deviation of iteration count (Monte Carlo stderr × √n).
    #[serde(default)]
    pub iter_stderr: f64,
    /// Mean total divine-equivalent cost across all iterations.
    #[serde(default = "DivEquiv::zero")]
    pub total_cost: DivEquiv,
}

impl Default for LoopEstimate {
    fn default() -> Self {
        Self {
            mean_iterations: 0.0,
            iter_stderr: 0.0,
            total_cost: DivEquiv::ZERO,
        }
    }
}

impl LoopEstimate {
    /// Construct an estimate from a per-iteration cost band and the
    /// expected mean / stderr iteration count.
    #[must_use]
    pub fn new(mean_iterations: f64, iter_stderr: f64, per_iter_cost: DivEquiv) -> Self {
        Self {
            mean_iterations,
            iter_stderr,
            total_cost: per_iter_cost.scale(mean_iterations.max(0.0)),
        }
    }
}

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
    /// Estimated probability of *reaching the goal* via this plan, in `[0, 1]`
    /// — execution-reliability × [`Self::goal_progress`]. NOT the raw
    /// step-execution probability (a safe no-op far from the goal would read
    /// ~90% on that; this reads low until the item actually carries the target
    /// mods). 1.0 for a satisfied goal via a non-probabilistic terminal action.
    pub expected_prob: f64,
    /// Structural goal-progress of the user's CURRENT item, in `[0, 1]`: the
    /// fraction of the goal's target specs (prefixes + suffixes) it already
    /// satisfies. `1.0` once the goal is met. Deterministic (not the noisy
    /// single-rollout terminal state) so the "n/m specs" bar is stable; the UI
    /// also uses it to colour the headline.
    #[serde(default)]
    pub goal_progress: f64,
    /// Standard error of the P(reach goal) estimate (Phase C.1 Monte Carlo),
    /// scaled by [`Self::goal_progress`] so the band stays proportionate.
    /// `0.0` when planner ran with `mc_samples = 1` or for non-probabilistic
    /// actions. UI renders this as the `± stderr` band.
    #[serde(default)]
    pub prob_stderr: f64,
    /// Final utility score the planner used to rank this. Higher = better.
    pub score: f64,
    /// Human-readable explanation surfaced in the UI.
    pub rationale: String,
    /// Beam-search depth at which this recommendation was found.
    /// Depth 1 = immediate; deeper = found via lookahead.
    pub depth: u32,
    /// Loop estimate — `Some` only when `action` is
    /// [`AdvisorAction::Recurring`]. Carries the iteration mean / stderr
    /// and total cost so the UI can render
    /// "this loop runs 8 ± 3 times costing 4-12 div".
    #[serde(default)]
    pub loop_estimate: Option<LoopEstimate>,
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
            goal_progress: 0.5,
            prob_stderr: 0.07,
            score: 4.0,
            rationale: "Chaos spam toward target.".into(),
            depth: 1,
            loop_estimate: None,
        };
        let s = serde_json::to_string(&r).unwrap();
        let back: Recommendation = serde_json::from_str(&s).unwrap();
        assert_eq!(back, r);
    }

    #[test]
    fn recurring_recommendation_round_trips_through_serde() {
        use poc2_engine::ids::ConceptId;
        let r = Recommendation {
            action: AdvisorAction::Recurring {
                inner: vec![
                    AdvisorAction::ApplyCurrency {
                        currency: CurrencyId::from("OrbOfAnnulment"),
                        omens: vec![],
                    },
                    AdvisorAction::ApplyCurrency {
                        currency: CurrencyId::from("ChaosOrb"),
                        omens: vec![],
                    },
                ],
                stop: StopPredicate {
                    concepts: vec![ConceptCriterion {
                        concept: ConceptId::from("EnergyShield"),
                        min_tier: 1,
                        affix: Some(AffixType::Prefix),
                    }],
                    max_mods: Some(6),
                },
            },
            source: RecommendationSource::Heuristic {
                name: "loop-collapse-annul-chaos".into(),
            },
            expected_cost: DivEquiv::point(0.5),
            expected_prob: 0.6,
            goal_progress: 0.6,
            prob_stderr: 0.1,
            score: 3.5,
            rationale: "Annul + Chaos until T1 ES on prefix.".into(),
            depth: 1,
            loop_estimate: Some(LoopEstimate {
                mean_iterations: 8.0,
                iter_stderr: 3.0,
                total_cost: DivEquiv::point(4.0),
            }),
        };
        let s = serde_json::to_string(&r).unwrap();
        let back: Recommendation = serde_json::from_str(&s).unwrap();
        assert_eq!(back, r);
    }

    #[test]
    fn stop_predicate_summary_renders_concepts() {
        use poc2_engine::ids::ConceptId;
        let p = StopPredicate {
            concepts: vec![ConceptCriterion {
                concept: ConceptId::from("EnergyShield"),
                min_tier: 1,
                affix: Some(AffixType::Prefix),
            }],
            max_mods: Some(6),
        };
        let s = p.summary();
        assert!(s.contains("EnergyShield"));
        assert!(s.contains("prefix"));
        assert!(s.contains("≤ 6"));
    }

    #[test]
    fn stop_predicate_empty_summary_is_never() {
        assert_eq!(StopPredicate::empty().summary(), "Stop when: never");
    }
}
