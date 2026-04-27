//! Evaluate [`ItemPredicate`]s against an [`Item`] + [`PredicateContext`].
//!
//! The evaluator is pure (no I/O, no RNG) and used by:
//! - [`crate::executor`] to decide which step branch to take after an action
//! - The advisor's candidate filter to gate strategies on preconditions
//! - The rule engine to fire production rules
//!
//! ## PredicateContext
//!
//! Predicates that compare against market or planner state (`StashHas`,
//! `CostSpent`, `ExpectedSalePrice`) need additional context beyond
//! `Item + ModRegistry`. [`PredicateContext`] bundles all of these so
//! callers can build it once per evaluation and pass it through.
//!
//! Backward-compat: the simplest form `PredicateContext::new(registry)`
//! omits market/cost data and returns `false` for any predicate that
//! depends on it. This keeps existing callers (executor, registry-only
//! tests) working unchanged.

use poc2_engine::ids::{ConceptId, CurrencyId, ItemClassId};
use poc2_engine::item::{AffixType, Item};
use poc2_engine::item_class::AttributePool;
use poc2_engine::mods::{ModFlags, ModKind};
use poc2_engine::registry::ModRegistry;
use poc2_market::Valuator;

use crate::dsl::ItemPredicate;

/// Read-only stash interface used by [`ItemPredicate::StashHas`].
///
/// Defined here as a trait so [`crate`] doesn't have to depend on the
/// concrete `Stash` type that lives in [`poc2_advisor`]. The advisor
/// crate provides the impl.
pub trait StashView: Send + Sync {
    /// How many of `currency` does the user own? `0` for unknown ids.
    fn currency_count(&self, currency: &CurrencyId) -> u32;
}

/// Bridge to the plugin host's custom-predicate dispatcher (Phase F.3).
///
/// Defined as a trait so this crate doesn't depend on
/// `poc2_plugin_host` (which would pull in wasmtime). The host crate
/// provides an `impl PluginPredicateDispatch for PluginHost`.
pub trait PluginPredicateDispatch: Send + Sync {
    /// Evaluate `plugin_id::name(item, args)`. `Err` on plugin-side
    /// failure; the predicate evaluator surfaces the error as
    /// `false` to keep a misbehaving plugin from tanking a planning
    /// session.
    fn dispatch(
        &self,
        plugin_id: &str,
        name: &str,
        item: &Item,
        args: &serde_json::Value,
    ) -> Result<bool, String>;
}

/// Bundles every input the predicate evaluator needs.
///
/// Build via the [`PredicateContext::new`] constructor + chained
/// `with_*` setters. Fields that aren't relevant to a given evaluation
/// can be left at their defaults; predicates that depend on them
/// evaluate to `false` rather than panicking.
pub struct PredicateContext<'a> {
    pub registry: &'a ModRegistry,
    /// Cumulative cost paid by the planner so far, in
    /// divine-equivalent (using the `expected` mid of each step's
    /// cost band). `0.0` when the caller doesn't track cost.
    pub cost_so_far_div: f64,
    /// Live valuator, if available. Used by predicates that want to
    /// compute current item value (M5+ extension hook).
    pub valuator: Option<&'a Valuator>,
    /// User's stash, if available. Drives [`ItemPredicate::StashHas`].
    pub stash: Option<&'a dyn StashView>,
    /// Estimated sale price of the current item state, in
    /// divine-equivalent. Drives [`ItemPredicate::ExpectedSalePrice`].
    pub expected_sale_price_div: Option<f64>,
    /// Plugin host bridge (Phase F.3). Drives
    /// [`ItemPredicate::Custom`]. `None` means custom predicates
    /// always evaluate to false.
    pub plugin_dispatch: Option<&'a dyn PluginPredicateDispatch>,
}

impl<'a> PredicateContext<'a> {
    /// Build a context with only a registry. Cost / valuator / stash /
    /// expected-sale-price are all defaulted; predicates depending on
    /// them return `false`.
    #[must_use]
    pub fn new(registry: &'a ModRegistry) -> Self {
        Self {
            registry,
            cost_so_far_div: 0.0,
            valuator: None,
            stash: None,
            expected_sale_price_div: None,
            plugin_dispatch: None,
        }
    }

    #[must_use]
    pub fn with_cost(mut self, cost_div: f64) -> Self {
        self.cost_so_far_div = cost_div;
        self
    }

    #[must_use]
    pub fn with_valuator(mut self, valuator: &'a Valuator) -> Self {
        self.valuator = Some(valuator);
        self
    }

    #[must_use]
    pub fn with_stash(mut self, stash: &'a dyn StashView) -> Self {
        self.stash = Some(stash);
        self
    }

    #[must_use]
    pub fn with_expected_sale_price(mut self, price_div: f64) -> Self {
        self.expected_sale_price_div = Some(price_div);
        self
    }

    #[must_use]
    pub fn with_plugin_dispatch(mut self, dispatch: &'a dyn PluginPredicateDispatch) -> Self {
        self.plugin_dispatch = Some(dispatch);
        self
    }
}

/// Evaluate an [`ItemPredicate`] against an item with the supplied
/// [`PredicateContext`].
///
/// Returns `false` for predicates that reference data the engine doesn't
/// yet expose (e.g., `AttributePool*` — the engine's `Item` doesn't
/// carry the attribute pool denormalized) or for context-dependent
/// predicates whose context is missing (e.g., `StashHas` with no stash).
#[allow(clippy::match_same_arms)] // intentional default-false branches
#[must_use]
pub fn eval(predicate: &ItemPredicate, item: &Item, ctx: &PredicateContext<'_>) -> bool {
    match predicate {
        ItemPredicate::Always => true,
        ItemPredicate::Never => false,

        ItemPredicate::Ilvl(p) => p.matches(i64::from(item.ilvl)),
        ItemPredicate::Rarity(r) => item.rarity == *r,
        ItemPredicate::Corrupted(b) => item.corrupted == *b,
        ItemPredicate::Sanctified(b) => item.sanctified == *b,
        ItemPredicate::Mirrored(b) => item.mirrored == *b,
        ItemPredicate::ItemClass(c) => &class_of_item(item) == c,
        ItemPredicate::ItemClassAny(cs) => cs.iter().any(|c| c == &class_of_item(item)),

        // AttributePool predicates need a BaseRegistry — TODO(M5).
        ItemPredicate::AttributePool(_) | ItemPredicate::AttributePoolAny(_) => false,

        ItemPredicate::AffixCount { affix, count } => {
            let n = affix_slot_count(item, *affix);
            count.matches(i64::try_from(n).unwrap_or(i64::MAX))
        }

        ItemPredicate::ModCount(p) => {
            let total = item.prefixes.len() + item.suffixes.len();
            p.matches(i64::try_from(total).unwrap_or(i64::MAX))
        }

        ItemPredicate::Quality(p) => p.matches(i64::from(item.quality)),

        ItemPredicate::HasConcept {
            concept,
            affix,
            min_tier: _, // tier ranking lands in M2.8 / M5; for now any-tier match
        } => has_concept(item, ctx.registry, concept, affix.as_ref()),

        ItemPredicate::HasFractured(b) => item.has_fractured() == *b,
        ItemPredicate::HasHiddenDesecrated(b) => item.hidden_desecrated.is_some() == *b,
        ItemPredicate::HasDesecratedRevealed(b) => has_desecrated_revealed(item) == *b,
        ItemPredicate::HasHinekoraLock(b) => item.hinekora_lock.is_some() == *b,

        ItemPredicate::StashHas { currency, count } => match ctx.stash {
            Some(stash) => count.matches(i64::from(stash.currency_count(currency))),
            None => false,
        },

        ItemPredicate::CostSpent(p) => p.matches(ctx.cost_so_far_div),

        ItemPredicate::ExpectedSalePrice(p) => match ctx.expected_sale_price_div {
            Some(price) => p.matches(price),
            None => false,
        },

        ItemPredicate::All(ps) => ps.iter().all(|p| eval(p, item, ctx)),
        ItemPredicate::Any(ps) => ps.iter().any(|p| eval(p, item, ctx)),
        ItemPredicate::Not(p) => !eval(p, item, ctx),

        ItemPredicate::Custom {
            plugin_id,
            name,
            args,
        } => match ctx.plugin_dispatch {
            Some(dispatch) => dispatch
                .dispatch(plugin_id, name, item, args)
                .unwrap_or(false),
            None => false,
        },
    }
}

fn affix_slot_count(item: &Item, affix: AffixType) -> usize {
    match affix {
        AffixType::Prefix => item.prefixes.len(),
        AffixType::Suffix => item.suffixes.len(),
        AffixType::Implicit => item.implicits.len(),
        AffixType::Enchantment => item.enchantments.len(),
    }
}

/// True iff at least one prefix or suffix [`ModRoll`] is of kind
/// [`ModKind::Desecrated`].
fn has_desecrated_revealed(item: &Item) -> bool {
    item.prefixes
        .iter()
        .chain(item.suffixes.iter())
        .any(|m| m.kind == ModKind::Desecrated)
}

/// True iff at least one mod on the item produces `concept`. Hybrid mods
/// match if their `concept_set` contains the concept. Restricts to the
/// given affix slot when `affix` is `Some(_)`.
fn has_concept(
    item: &Item,
    registry: &ModRegistry,
    concept: &ConceptId,
    affix: Option<&AffixType>,
) -> bool {
    let check_slot = |slot: &[poc2_engine::ModRoll]| -> bool {
        slot.iter().any(|m| {
            let Some(def) = registry.get(&m.mod_id) else {
                return false;
            };
            // Atomic mods: concept_set has 1 entry, and HYBRID flag is off.
            // Either way, set membership decides.
            def.concept_set.iter().any(|c| c == concept)
        })
    };
    match affix {
        Some(AffixType::Prefix) => check_slot(&item.prefixes),
        Some(AffixType::Suffix) => check_slot(&item.suffixes),
        Some(AffixType::Implicit) => check_slot(&item.implicits),
        Some(AffixType::Enchantment) => check_slot(&item.enchantments),
        None => check_slot(&item.prefixes) || check_slot(&item.suffixes),
    }
}

fn class_of_item(item: &Item) -> ItemClassId {
    // Until BaseRegistry lands, we use the convention that test fixtures
    // and synthetic items put the class id into the base field.
    ItemClassId::from(item.base.as_str())
}

/// Trivial extension trait for [`AttributePool`] used by some predicate
/// implementations. Kept small for now; expanded when the base registry
/// wires through.
pub trait AttributePoolExt {
    fn matches(self, other: AttributePool) -> bool;
}
impl AttributePoolExt for AttributePool {
    fn matches(self, other: AttributePool) -> bool {
        self == other
    }
}

/// Bundle a full evaluation so consumers can pass everything in one shot.
pub fn eval_all(preds: &[ItemPredicate], item: &Item, ctx: &PredicateContext<'_>) -> bool {
    preds.iter().all(|p| eval(p, item, ctx))
}

/// Are any of these predicates true?
pub fn eval_any(preds: &[ItemPredicate], item: &Item, ctx: &PredicateContext<'_>) -> bool {
    preds.iter().any(|p| eval(p, item, ctx))
}

/// Whether a `mod_id` is hybrid (has `ModFlags::HYBRID`).
pub fn is_hybrid_mod(mod_id: &poc2_engine::ids::ModId, registry: &ModRegistry) -> bool {
    registry
        .get(mod_id)
        .is_some_and(|d| d.flags.contains(ModFlags::HYBRID))
}

#[cfg(test)]
mod tests {
    use ahash::AHashMap;
    use poc2_engine::ids::{ItemClassId, ModGroupId, ModId, StatId, TagId};
    use poc2_engine::item::{ModRoll, QualityKind, Rarity};
    use poc2_engine::mods::{
        ModDefinition, ModDomain, ModFlags, ModGroup, ModKind, ModStat, SpawnWeight,
    };
    use poc2_engine::patch::PatchRange;
    use smallvec::smallvec;

    use super::*;
    use crate::dsl::{CmpOp, FloatValuePredicate, ValuePredicate};

    /// Test stash impl backed by a hashmap.
    struct TestStash {
        counts: AHashMap<CurrencyId, u32>,
    }
    impl StashView for TestStash {
        fn currency_count(&self, id: &CurrencyId) -> u32 {
            self.counts.get(id).copied().unwrap_or(0)
        }
    }

    fn fixture_es_armour_with_es_prefix() -> (Item, ModRegistry) {
        let mod_es = ModDefinition {
            id: ModId::from("EsPrefix1"),
            name: None,
            mod_group: ModGroup(ModGroupId::from("EsGroup")),
            affix_type: AffixType::Prefix,
            kind: ModKind::Explicit,
            domain: ModDomain::Item,
            tags: smallvec![],
            concept_set: smallvec![ConceptId::from("EnergyShield")],
            spawn_weights: smallvec![SpawnWeight {
                tag: TagId::from("BodyArmour"),
                weight: 1
            }],
            stats: smallvec![ModStat {
                stat_id: StatId::from("local_energy_shield"),
                min: 50.0,
                max: 80.0
            }],
            required_level: 75,
            allowed_item_classes: smallvec![ItemClassId::from("BodyArmour")],
            patch_range: PatchRange::ALL,
            flags: ModFlags::empty(),
            text_template: None,
        };
        let mod_hybrid = ModDefinition {
            id: ModId::from("EsLifeHybrid1"),
            concept_set: smallvec![ConceptId::from("EnergyShield"), ConceptId::from("Life")],
            flags: ModFlags::HYBRID,
            ..mod_es.clone()
        };
        let registry = ModRegistry::from_mods(vec![mod_es, mod_hybrid], vec![]);

        let item = Item {
            base: ItemClassId::from("BodyArmour").as_str().into(),
            ilvl: 82,
            rarity: Rarity::Rare,
            corrupted: false,
            sanctified: false,
            mirrored: false,
            quality: 0,
            quality_kind: QualityKind::Untagged,
            implicits: smallvec![],
            prefixes: smallvec![ModRoll {
                mod_id: ModId::from("EsPrefix1"),
                affix_type: AffixType::Prefix,
                kind: ModKind::Explicit,
                values: smallvec![60.0],
                is_fractured: false,
            }],
            suffixes: smallvec![],
            enchantments: smallvec![],
            hidden_desecrated: None,
            sockets: smallvec![],
            hinekora_lock: None,
        };
        (item, registry)
    }

    #[test]
    fn eval_ilvl_constraint() {
        let (item, reg) = fixture_es_armour_with_es_prefix();
        let ctx = PredicateContext::new(&reg);
        assert!(eval(
            &ItemPredicate::Ilvl(ValuePredicate {
                op: CmpOp::Gte,
                value: 82
            }),
            &item,
            &ctx
        ));
        assert!(!eval(
            &ItemPredicate::Ilvl(ValuePredicate {
                op: CmpOp::Gte,
                value: 83
            }),
            &item,
            &ctx
        ));
    }

    #[test]
    fn eval_rarity() {
        let (item, reg) = fixture_es_armour_with_es_prefix();
        let ctx = PredicateContext::new(&reg);
        assert!(eval(&ItemPredicate::Rarity(Rarity::Rare), &item, &ctx));
        assert!(!eval(&ItemPredicate::Rarity(Rarity::Magic), &item, &ctx));
    }

    #[test]
    fn eval_has_concept_atomic() {
        let (item, reg) = fixture_es_armour_with_es_prefix();
        let ctx = PredicateContext::new(&reg);
        assert!(eval(
            &ItemPredicate::HasConcept {
                concept: ConceptId::from("EnergyShield"),
                affix: None,
                min_tier: None,
            },
            &item,
            &ctx
        ));
        assert!(eval(
            &ItemPredicate::HasConcept {
                concept: ConceptId::from("EnergyShield"),
                affix: Some(AffixType::Prefix),
                min_tier: None,
            },
            &item,
            &ctx
        ));
        assert!(!eval(
            &ItemPredicate::HasConcept {
                concept: ConceptId::from("EnergyShield"),
                affix: Some(AffixType::Suffix),
                min_tier: None,
            },
            &item,
            &ctx
        ));
    }

    #[test]
    fn eval_has_concept_hybrid_satisfies_es_target() {
        let (mut item, reg) = fixture_es_armour_with_es_prefix();
        item.prefixes[0].mod_id = ModId::from("EsLifeHybrid1");
        let ctx = PredicateContext::new(&reg);
        assert!(eval(
            &ItemPredicate::HasConcept {
                concept: ConceptId::from("EnergyShield"),
                affix: Some(AffixType::Prefix),
                min_tier: None,
            },
            &item,
            &ctx
        ));
        assert!(eval(
            &ItemPredicate::HasConcept {
                concept: ConceptId::from("Life"),
                affix: Some(AffixType::Prefix),
                min_tier: None,
            },
            &item,
            &ctx
        ));
    }

    #[test]
    fn eval_compound_all_any_not() {
        let (item, reg) = fixture_es_armour_with_es_prefix();
        let ctx = PredicateContext::new(&reg);
        let p = ItemPredicate::All(vec![
            ItemPredicate::Ilvl(ValuePredicate {
                op: CmpOp::Gte,
                value: 82,
            }),
            ItemPredicate::Rarity(Rarity::Rare),
            ItemPredicate::Not(Box::new(ItemPredicate::Corrupted(true))),
        ]);
        assert!(eval(&p, &item, &ctx));

        let p = ItemPredicate::Any(vec![
            ItemPredicate::Rarity(Rarity::Magic),
            ItemPredicate::Corrupted(false),
        ]);
        assert!(eval(&p, &item, &ctx));
    }

    #[test]
    fn eval_affix_count_uses_value_predicate() {
        let (item, reg) = fixture_es_armour_with_es_prefix();
        let ctx = PredicateContext::new(&reg);
        assert!(eval(
            &ItemPredicate::AffixCount {
                affix: AffixType::Prefix,
                count: ValuePredicate {
                    op: CmpOp::Eq,
                    value: 1
                },
            },
            &item,
            &ctx
        ));
        assert!(eval(
            &ItemPredicate::AffixCount {
                affix: AffixType::Suffix,
                count: ValuePredicate {
                    op: CmpOp::Eq,
                    value: 0
                },
            },
            &item,
            &ctx
        ));
    }

    #[test]
    fn eval_state_predicates() {
        let (mut item, reg) = fixture_es_armour_with_es_prefix();
        let ctx = PredicateContext::new(&reg);
        assert!(eval(&ItemPredicate::HasFractured(false), &item, &ctx));
        item.prefixes[0].is_fractured = true;
        assert!(eval(&ItemPredicate::HasFractured(true), &item, &ctx));

        assert!(eval(
            &ItemPredicate::HasHiddenDesecrated(false),
            &item,
            &ctx
        ));
        assert!(eval(&ItemPredicate::HasHinekoraLock(false), &item, &ctx));
        item.hinekora_lock = Some(42);
        assert!(eval(&ItemPredicate::HasHinekoraLock(true), &item, &ctx));
    }

    #[test]
    fn always_never() {
        let (item, reg) = fixture_es_armour_with_es_prefix();
        let ctx = PredicateContext::new(&reg);
        assert!(eval(&ItemPredicate::Always, &item, &ctx));
        assert!(!eval(&ItemPredicate::Never, &item, &ctx));
    }

    // ------------------------------------------------------------------
    // New predicates (A.1)
    // ------------------------------------------------------------------

    #[test]
    fn eval_mod_count_total() {
        let (mut item, reg) = fixture_es_armour_with_es_prefix();
        let ctx = PredicateContext::new(&reg);
        // 1 prefix + 0 suffixes = 1 mod
        assert!(eval(
            &ItemPredicate::ModCount(ValuePredicate {
                op: CmpOp::Eq,
                value: 1
            }),
            &item,
            &ctx
        ));
        item.suffixes.push(ModRoll {
            mod_id: ModId::from("EsPrefix1"),
            affix_type: AffixType::Suffix,
            kind: ModKind::Explicit,
            values: smallvec![],
            is_fractured: false,
        });
        assert!(eval(
            &ItemPredicate::ModCount(ValuePredicate {
                op: CmpOp::Eq,
                value: 2
            }),
            &item,
            &ctx
        ));
    }

    #[test]
    fn eval_quality_predicate() {
        let (mut item, reg) = fixture_es_armour_with_es_prefix();
        let ctx = PredicateContext::new(&reg);
        item.quality = 17;
        assert!(eval(
            &ItemPredicate::Quality(ValuePredicate {
                op: CmpOp::Gte,
                value: 15
            }),
            &item,
            &ctx
        ));
        assert!(!eval(
            &ItemPredicate::Quality(ValuePredicate {
                op: CmpOp::Gte,
                value: 20
            }),
            &item,
            &ctx
        ));
    }

    #[test]
    fn eval_has_desecrated_revealed() {
        let (mut item, reg) = fixture_es_armour_with_es_prefix();
        let ctx = PredicateContext::new(&reg);
        assert!(eval(
            &ItemPredicate::HasDesecratedRevealed(false),
            &item,
            &ctx
        ));
        item.suffixes.push(ModRoll {
            mod_id: ModId::from("DesecPrefix1"),
            affix_type: AffixType::Suffix,
            kind: ModKind::Desecrated,
            values: smallvec![],
            is_fractured: false,
        });
        assert!(eval(
            &ItemPredicate::HasDesecratedRevealed(true),
            &item,
            &ctx
        ));
    }

    #[test]
    fn eval_stash_has_returns_false_without_stash() {
        let (item, reg) = fixture_es_armour_with_es_prefix();
        let ctx = PredicateContext::new(&reg);
        assert!(!eval(
            &ItemPredicate::StashHas {
                currency: CurrencyId::from("DivineOrb"),
                count: ValuePredicate {
                    op: CmpOp::Gte,
                    value: 1
                }
            },
            &item,
            &ctx
        ));
    }

    #[test]
    fn eval_stash_has_with_stash() {
        let (item, reg) = fixture_es_armour_with_es_prefix();
        let mut counts = AHashMap::new();
        counts.insert(CurrencyId::from("DivineOrb"), 3);
        let stash = TestStash { counts };
        let ctx = PredicateContext::new(&reg).with_stash(&stash);
        assert!(eval(
            &ItemPredicate::StashHas {
                currency: CurrencyId::from("DivineOrb"),
                count: ValuePredicate {
                    op: CmpOp::Gte,
                    value: 2
                }
            },
            &item,
            &ctx
        ));
        assert!(!eval(
            &ItemPredicate::StashHas {
                currency: CurrencyId::from("DivineOrb"),
                count: ValuePredicate {
                    op: CmpOp::Gte,
                    value: 5
                }
            },
            &item,
            &ctx
        ));
        assert!(!eval(
            &ItemPredicate::StashHas {
                currency: CurrencyId::from("UnknownOrb"),
                count: ValuePredicate {
                    op: CmpOp::Gte,
                    value: 1
                }
            },
            &item,
            &ctx
        ));
    }

    #[test]
    fn eval_cost_spent_uses_context_value() {
        let (item, reg) = fixture_es_armour_with_es_prefix();
        let ctx = PredicateContext::new(&reg).with_cost(50.0);
        assert!(eval(
            &ItemPredicate::CostSpent(FloatValuePredicate {
                op: CmpOp::Gt,
                value: 40.0
            }),
            &item,
            &ctx
        ));
        assert!(!eval(
            &ItemPredicate::CostSpent(FloatValuePredicate {
                op: CmpOp::Gt,
                value: 60.0
            }),
            &item,
            &ctx
        ));
    }

    #[test]
    fn eval_expected_sale_price_returns_false_without_value() {
        let (item, reg) = fixture_es_armour_with_es_prefix();
        let ctx = PredicateContext::new(&reg);
        assert!(!eval(
            &ItemPredicate::ExpectedSalePrice(FloatValuePredicate {
                op: CmpOp::Gte,
                value: 1.0
            }),
            &item,
            &ctx
        ));
    }

    #[test]
    fn eval_expected_sale_price_with_value() {
        let (item, reg) = fixture_es_armour_with_es_prefix();
        let ctx = PredicateContext::new(&reg).with_expected_sale_price(15.0);
        assert!(eval(
            &ItemPredicate::ExpectedSalePrice(FloatValuePredicate {
                op: CmpOp::Gte,
                value: 10.0
            }),
            &item,
            &ctx
        ));
    }
}
