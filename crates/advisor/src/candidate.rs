//! Candidate-action generation.
//!
//! The candidate generator is the heart of the advisor's first stage:
//! given a state, it enumerates the (small, plausible) set of next-step
//! actions to expand in the beam search. Three sources contribute:
//!
//! 1. **Rules** ([`poc2_rules`]) — every rule whose `when` predicate
//!    fires emits one or more suggestions. Cheap (forward chain), high
//!    signal.
//! 2. **Strategies** ([`poc2_strategies`]) — every strategy in the
//!    registry whose preconditions match emits the action of its current
//!    entry step. Multi-step lookahead happens via the planner re-running
//!    the generator at deeper depths.
//! 3. **Heuristics** — a small, hard-coded fallback set so that the
//!    advisor still produces something useful when both rules and
//!    strategies fall silent (e.g., "Normal item with no rules firing
//!    → suggest Transmute").
//!
//! The generator filters by stash availability when [`Stash::unlimited`]
//! is false, so the advisor never recommends an action the user can't
//! take.

use poc2_engine::ids::CurrencyId;
use poc2_engine::item::{Item, Rarity};
use poc2_engine::patch::PatchVersion;
use poc2_rules::RuleSet;
use poc2_strategies::{eval_all, PredicateContext, StrategyRegistry};

use crate::action::{from_rule_action, from_strategy_action, AdvisorAction};
use crate::recommendation::RecommendationSource;
use crate::stash::Stash;

/// One candidate action plus the source / prior info the planner needs.
#[derive(Debug, Clone)]
pub struct Candidate {
    /// Concrete action.
    pub action: AdvisorAction,
    /// Where it came from.
    pub source: RecommendationSource,
    /// Source's confidence in this action being correct, in `[0, 1]`.
    /// Higher = more sure. Used as a soft prior during scoring.
    pub prior: f64,
    /// Source's priority signal (0..=255 roughly). Used for tie-breaking.
    pub priority: u32,
    /// Free-form rationale string the source attached.
    pub rationale: String,
}

/// Build the candidate set for a given state.
///
/// Returns deduplicated candidates: if multiple sources propose the same
/// `(currency, omens)` action, the highest-priority one wins.
///
/// `ctx` carries the registry plus optional cost / stash / valuator /
/// expected-sale-price data used by [`poc2_strategies::ItemPredicate`]
/// variants such as `CostSpent`, `StashHas`, and `ExpectedSalePrice`.
/// `stash` is also passed separately because action-affordability is a
/// runtime check (not a predicate) and may differ in v1.x when "buyable
/// now" prices feed into the affordability calculus.
#[must_use]
pub fn generate_candidates(
    item: &Item,
    ctx: &PredicateContext<'_>,
    rules: &RuleSet,
    strategies: &StrategyRegistry,
    stash: &Stash,
    patch: PatchVersion,
) -> Vec<Candidate> {
    let mut out: Vec<Candidate> = Vec::new();

    // ------- Rule-emitted candidates -------------------------------------
    for r in poc2_rules::evaluate_with_ctx(rules, item, ctx) {
        let action = from_rule_action(&r.suggestion.action);
        if !is_action_affordable(&action, stash) {
            continue;
        }
        let confidence = r.rule.confidence;
        let source = RecommendationSource::Rule {
            id: r.rule.id.0.clone(),
            confidence,
        };
        let prior = match confidence {
            poc2_rules::Confidence::Verified => 0.9,
            poc2_rules::Confidence::Community => 0.7,
            poc2_rules::Confidence::Experimental => 0.5,
        };
        out.push(Candidate {
            action,
            source,
            prior,
            priority: r.suggestion.priority,
            rationale: r.suggestion.note.clone(),
        });
    }

    // ------- Strategy-emitted candidates ---------------------------------
    let class = poc2_engine::ids::ItemClassId::from(item.base.as_str());
    for strategy in strategies.for_class(&class, patch) {
        // Precondition gate — same as the executor's `enter`.
        if !eval_all(&strategy.preconditions, item, ctx) {
            continue;
        }
        let Some(entry) = strategy.entry() else {
            continue;
        };
        let Some(action) = from_strategy_action(&entry.action) else {
            continue;
        };
        if !is_action_affordable(&action, stash) {
            continue;
        }
        let prior = match strategy.confidence {
            poc2_strategies::Confidence::Verified => 0.9,
            poc2_strategies::Confidence::Community => 0.7,
            poc2_strategies::Confidence::Experimental => 0.5,
        };
        let priority = match strategy.expected_success_prob {
            Some((_, hi)) => (hi * 255.0).round() as u32,
            None => 100,
        };
        out.push(Candidate {
            action,
            source: RecommendationSource::Strategy {
                id: strategy.id.0.clone(),
                step: entry.id.0.clone(),
            },
            prior,
            priority,
            rationale: strategy.name.clone(),
        });
    }

    // ------- Heuristic fallback ------------------------------------------
    // If neither rules nor strategies fired we fall back to a tiny set of
    // "always plausible" moves so the advisor never returns empty.
    if out.is_empty() {
        for c in heuristic_fallback(item) {
            if is_action_affordable(&c.action, stash) {
                out.push(c);
            }
        }
    }

    // ------- Deduplicate by action; keep highest priority ----------------
    out.sort_by_key(|c| std::cmp::Reverse(c.priority));
    let mut seen: ahash::AHashSet<AdvisorAction> = ahash::AHashSet::new();
    out.retain(|c| seen.insert(c.action.clone()));
    out
}

/// Stash-affordability filter. Non-currency actions are always affordable.
fn is_action_affordable(action: &AdvisorAction, stash: &Stash) -> bool {
    match action {
        AdvisorAction::ApplyCurrency { currency, omens } => stash.can_afford(currency, omens),
        _ => true,
    }
}

/// Heuristic fallback set when nothing else fires. Intentionally small —
/// these are the "obvious next moves" given an item's rarity and slot
/// fill state.
fn heuristic_fallback(item: &Item) -> Vec<Candidate> {
    let mut out = Vec::new();
    let mk = |currency: &str, name: &str, prior: f64, priority: u32, rationale: &str| Candidate {
        action: AdvisorAction::ApplyCurrency {
            currency: CurrencyId::from(currency),
            omens: vec![],
        },
        source: RecommendationSource::Heuristic { name: name.into() },
        prior,
        priority,
        rationale: rationale.into(),
    };

    match item.rarity {
        Rarity::Normal => {
            out.push(mk(
                "OrbOfTransmutation",
                "fallback-transmute-on-normal",
                0.5,
                90,
                "Normal item: Transmute promotes to Magic.",
            ));
        }
        Rarity::Magic => {
            if item.prefixes.is_empty() || item.suffixes.is_empty() {
                out.push(mk(
                    "OrbOfAugmentation",
                    "fallback-aug-on-magic",
                    0.5,
                    85,
                    "Magic with empty slot: Augment fills it.",
                ));
            }
            out.push(mk(
                "RegalOrb",
                "fallback-regal-on-magic",
                0.5,
                80,
                "Magic with both slots filled: Regal promotes to Rare.",
            ));
        }
        Rarity::Rare => {
            // No clear default — surface guidance instead of guessing.
            out.push(Candidate {
                action: AdvisorAction::Guidance {
                    note: "Rare item; further moves depend on goal.".into(),
                },
                source: RecommendationSource::Heuristic {
                    name: "fallback-rare-guidance".into(),
                },
                prior: 0.3,
                priority: 50,
                rationale: "No matching rule fired; strategy library is silent.".into(),
            });
        }
        Rarity::Unique => {}
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use poc2_engine::ids::ItemClassId;
    use poc2_engine::item::{QualityKind, Rarity};
    use poc2_engine::registry::ModRegistry;
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

    #[test]
    fn rules_fire_for_normal_with_seed_rule() {
        let reg = ModRegistry::from_mods(vec![]);
        let rules = RuleSet::from_rules(poc2_rules::seed_rules());
        let strategies = StrategyRegistry::default();
        let stash = Stash::unlimited();
        let item = empty_item(Rarity::Normal);
        let ctx = PredicateContext::new(&reg).with_stash(&stash);
        let cands = generate_candidates(
            &item,
            &ctx,
            &rules,
            &strategies,
            &stash,
            PatchVersion::PATCH_0_4_0,
        );
        assert!(!cands.is_empty(), "rules should fire on Normal ilvl 82");
        // Highest-priority candidate should be the Perfect Transmute (R001).
        let top = &cands[0];
        assert!(
            matches!(&top.source, RecommendationSource::Rule { id, .. } if id.starts_with("R001"))
        );
    }

    #[test]
    fn fallback_emits_when_no_rules_match() {
        // Sanctified items should bypass most rules but emit at least guidance.
        let reg = ModRegistry::from_mods(vec![]);
        let rules = RuleSet::default();
        let strategies = StrategyRegistry::default();
        let stash = Stash::unlimited();
        let mut item = empty_item(Rarity::Magic);
        item.prefixes.clear();
        let ctx = PredicateContext::new(&reg).with_stash(&stash);
        let cands = generate_candidates(
            &item,
            &ctx,
            &rules,
            &strategies,
            &stash,
            PatchVersion::PATCH_0_4_0,
        );
        assert!(!cands.is_empty());
        // Should suggest Augment.
        assert!(cands
            .iter()
            .any(|c| matches!(&c.source, RecommendationSource::Heuristic { .. })));
    }

    #[test]
    fn stash_filters_unavailable_actions() {
        let reg = ModRegistry::from_mods(vec![]);
        let rules = RuleSet::from_rules(poc2_rules::seed_rules());
        let strategies = StrategyRegistry::default();
        // Empty stash → no affordable actions.
        let stash = Stash::new();
        let item = empty_item(Rarity::Normal);
        let ctx = PredicateContext::new(&reg).with_stash(&stash);
        let cands = generate_candidates(
            &item,
            &ctx,
            &rules,
            &strategies,
            &stash,
            PatchVersion::PATCH_0_4_0,
        );
        // Some candidates may still emerge as Stop / Abandon / Guidance.
        for c in &cands {
            assert!(
                !c.action.is_mutating()
                    || stash.can_afford(
                        c.action.currency_id().expect("mutating without currency"),
                        c.action.omens()
                    )
            );
        }
    }

    #[test]
    fn cost_spent_predicate_threads_through_ctx() {
        // A rule that fires only when CostSpent > 5 div should fire when
        // the ctx carries cost > 5 and not fire when it doesn't.
        use poc2_engine::ids::CurrencyId;
        use poc2_strategies::{CmpOp, FloatValuePredicate, ItemPredicate};

        let reg = ModRegistry::from_mods(vec![]);
        let stash = Stash::unlimited();
        let strategies = StrategyRegistry::default();
        let item = empty_item(Rarity::Normal);

        // Build a tiny ruleset: one rule with CostSpent > 5 → Guidance.
        let rule = poc2_rules::Rule {
            id: poc2_rules::RuleId::from("test-cost-rule"),
            category: poc2_rules::Category::Budget,
            when: ItemPredicate::CostSpent(FloatValuePredicate {
                op: CmpOp::Gt,
                value: 5.0,
            }),
            then: smallvec::smallvec![poc2_rules::Suggestion {
                action: poc2_rules::SuggestionAction::Abandon {
                    reason: "budget exceeded".into(),
                },
                note: "test".into(),
                priority: 100,
            }],
            explanation: "test".into(),
            source: "test".into(),
            confidence: poc2_rules::Confidence::Verified,
        };
        let _ = CurrencyId::from("ChaosOrb"); // silence unused warning
        let rules = RuleSet::from_rules(vec![rule]);

        let cheap_ctx = PredicateContext::new(&reg)
            .with_stash(&stash)
            .with_cost(2.0);
        let cands_cheap = generate_candidates(
            &item,
            &cheap_ctx,
            &rules,
            &strategies,
            &stash,
            PatchVersion::PATCH_0_4_0,
        );
        assert!(
            !cands_cheap
                .iter()
                .any(|c| matches!(c.action, AdvisorAction::Abandon { .. })),
            "abandon rule should not fire below threshold"
        );

        let pricey_ctx = PredicateContext::new(&reg)
            .with_stash(&stash)
            .with_cost(10.0);
        let cands_pricey = generate_candidates(
            &item,
            &pricey_ctx,
            &rules,
            &strategies,
            &stash,
            PatchVersion::PATCH_0_4_0,
        );
        assert!(
            cands_pricey
                .iter()
                .any(|c| matches!(c.action, AdvisorAction::Abandon { .. })),
            "abandon rule should fire above threshold"
        );
    }
}
