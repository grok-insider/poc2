//! Forward-chaining rule engine.
//!
//! For each rule in the [`RuleSet`], evaluate its `when` predicate against
//! the item; if true, emit the rule's [`Suggestion`]s. Returns suggestions
//! sorted by `priority` (descending), with the originating rule id
//! attached for traceability.

use poc2_engine::item::Item;
use poc2_engine::registry::ModRegistry;
use poc2_strategies::{eval, PredicateContext};

use crate::rule::{Rule, RuleId, RuleSet, Suggestion};

/// One result row: a suggestion plus the rule that produced it.
#[derive(Debug, Clone, PartialEq)]
pub struct EngineResult<'a> {
    pub rule: &'a Rule,
    pub suggestion: &'a Suggestion,
}

impl EngineResult<'_> {
    pub fn rule_id(&self) -> &RuleId {
        &self.rule.id
    }
}

/// Evaluate every rule in `set` against `item`, returning matching
/// suggestions in priority order (highest first, ties broken by rule
/// insertion order).
///
/// Convenience overload that builds a default
/// [`PredicateContext`] from the registry alone. Predicates that need
/// market / cost / stash data evaluate to `false` under this entry
/// point — use [`evaluate_with_ctx`] when those are available.
#[must_use]
pub fn evaluate<'a>(
    set: &'a RuleSet,
    item: &Item,
    registry: &ModRegistry,
) -> Vec<EngineResult<'a>> {
    let ctx = PredicateContext::new(registry);
    evaluate_with_ctx(set, item, &ctx)
}

/// Evaluate every rule in `set` against `item`, using the supplied
/// [`PredicateContext`] for market / cost / stash predicates.
#[must_use]
pub fn evaluate_with_ctx<'a>(
    set: &'a RuleSet,
    item: &Item,
    ctx: &PredicateContext<'_>,
) -> Vec<EngineResult<'a>> {
    let mut out: Vec<EngineResult<'a>> = Vec::new();
    for rule in set.iter() {
        if !eval(&rule.when, item, ctx) {
            continue;
        }
        for suggestion in &rule.then {
            out.push(EngineResult { rule, suggestion });
        }
    }
    // Sort by priority descending; stable so insertion order breaks ties.
    out.sort_by_key(|r| std::cmp::Reverse(r.suggestion.priority));
    out
}

#[cfg(test)]
mod tests {
    use poc2_engine::ids::{ItemClassId, ModId};
    use poc2_engine::item::{ModRoll, QualityKind, Rarity};
    use poc2_engine::mods::ModKind;
    use smallvec::smallvec;

    use super::*;
    use crate::rule::{Category, Confidence, Suggestion, SuggestionAction};
    use poc2_engine::item::AffixType;
    use poc2_strategies::{CmpOp, ItemPredicate, ValuePredicate};

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

    fn rule_for_normal_apply_transmute() -> Rule {
        Rule {
            id: RuleId::from("transmute_on_normal"),
            category: Category::Other,
            when: ItemPredicate::All(vec![
                ItemPredicate::Rarity(Rarity::Normal),
                ItemPredicate::Ilvl(ValuePredicate {
                    op: CmpOp::Gte,
                    value: 82,
                }),
            ]),
            then: smallvec![Suggestion {
                action: SuggestionAction::ApplyCurrency {
                    currency: poc2_engine::ids::CurrencyId::from("PerfectOrbOfTransmutation"),
                    omens: vec![],
                },
                note:
                    "Apply Perfect Transmutation to upgrade Normal to Magic with a high-tier mod."
                        .into(),
                priority: 100,
            }],
            explanation: "Normal -> Magic step uses Perfect Transmutation when ilvl gates allow."
                .into(),
            source: "test".into(),
            confidence: Confidence::Community,
        }
    }

    fn rule_for_rare_with_4_mods_fracture() -> Rule {
        Rule {
            id: RuleId::from("fracture_when_4_mods"),
            category: Category::Fracture,
            when: ItemPredicate::All(vec![
                ItemPredicate::Rarity(Rarity::Rare),
                ItemPredicate::AffixCount {
                    affix: AffixType::Prefix,
                    count: ValuePredicate {
                        op: CmpOp::Gte,
                        value: 2,
                    },
                },
            ]),
            then: smallvec![Suggestion {
                action: SuggestionAction::ApplyCurrency {
                    currency: poc2_engine::ids::CurrencyId::from("FracturingOrb"),
                    omens: vec![],
                },
                note: "≥2 prefixes on a Rare suggests preparing a fracture.".into(),
                priority: 200,
            }],
            explanation: "Fracture-then-finish workflow.".into(),
            source: "test".into(),
            confidence: Confidence::Community,
        }
    }

    #[test]
    fn evaluate_returns_matching_rules_only() {
        let registry = ModRegistry::from_mods(vec![]);
        let set = RuleSet::from_rules(vec![
            rule_for_normal_apply_transmute(),
            rule_for_rare_with_4_mods_fracture(),
        ]);

        // Normal item -> only the transmute rule fires.
        let item = empty_item(Rarity::Normal);
        let results = evaluate(&set, &item, &registry);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].rule.id.0, "transmute_on_normal");
    }

    #[test]
    fn evaluate_sorts_by_priority_descending() {
        let registry = ModRegistry::from_mods(vec![]);
        let set = RuleSet::from_rules(vec![
            rule_for_normal_apply_transmute(),    // priority 100
            rule_for_rare_with_4_mods_fracture(), // priority 200
        ]);
        let mut item = empty_item(Rarity::Rare);
        for _ in 0..2 {
            item.prefixes.push(ModRoll {
                mod_id: ModId::from("X"),
                affix_type: AffixType::Prefix,
                kind: ModKind::Explicit,
                values: smallvec![],
                is_fractured: false,
            });
        }
        let results = evaluate(&set, &item, &registry);
        assert_eq!(results.len(), 1);
        // Priority 200 fracture rule fires.
        assert_eq!(results[0].rule.id.0, "fracture_when_4_mods");
    }
}
