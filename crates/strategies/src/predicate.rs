//! Evaluate [`ItemPredicate`]s against an [`Item`] + [`ModRegistry`].
//!
//! The evaluator is pure (no I/O, no RNG) and used by:
//! - [`crate::executor`] to decide which step branch to take after an action
//! - The advisor's candidate filter to gate strategies on preconditions
//! - The rule engine (M3.d) to fire production rules

use poc2_engine::ids::{ConceptId, ItemClassId};
use poc2_engine::item::{AffixType, Item};
use poc2_engine::item_class::AttributePool;
use poc2_engine::mods::ModFlags;
use poc2_engine::registry::ModRegistry;

use crate::dsl::ItemPredicate;

/// Evaluate an [`ItemPredicate`] against an item.
///
/// Returns `false` for predicates that reference data the engine doesn't
/// yet expose (e.g., AttributePool — the engine's `Item` doesn't carry the
/// attribute pool denormalized, so predicates over it require a base
/// registry which lands in the data crate). M5+ wires that through.
#[allow(clippy::match_same_arms)] // Never and AttributePool* genuinely share the false branch
#[must_use]
pub fn eval(predicate: &ItemPredicate, item: &Item, registry: &ModRegistry) -> bool {
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

        ItemPredicate::HasConcept {
            concept,
            affix,
            min_tier: _, // tier ranking lands in M2.8 / M5; for now any-tier match
        } => has_concept(item, registry, concept, affix.as_ref()),

        ItemPredicate::HasFractured(b) => item.has_fractured() == *b,
        ItemPredicate::HasHiddenDesecrated(b) => item.hidden_desecrated.is_some() == *b,
        ItemPredicate::HasHinekoraLock(b) => item.hinekora_lock.is_some() == *b,

        ItemPredicate::All(ps) => ps.iter().all(|p| eval(p, item, registry)),
        ItemPredicate::Any(ps) => ps.iter().any(|p| eval(p, item, registry)),
        ItemPredicate::Not(p) => !eval(p, item, registry),
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
pub fn eval_all(preds: &[ItemPredicate], item: &Item, registry: &ModRegistry) -> bool {
    preds.iter().all(|p| eval(p, item, registry))
}

/// Are any of these predicates true?
pub fn eval_any(preds: &[ItemPredicate], item: &Item, registry: &ModRegistry) -> bool {
    preds.iter().any(|p| eval(p, item, registry))
}

/// Whether a `mod_id` is hybrid (has `ModFlags::HYBRID`).
pub fn is_hybrid_mod(mod_id: &poc2_engine::ids::ModId, registry: &ModRegistry) -> bool {
    registry
        .get(mod_id)
        .is_some_and(|d| d.flags.contains(ModFlags::HYBRID))
}

#[cfg(test)]
mod tests {
    use poc2_engine::ids::{ItemClassId, ModGroupId, ModId, StatId, TagId};
    use poc2_engine::item::{ModRoll, QualityKind, Rarity};
    use poc2_engine::mods::{
        ModDefinition, ModDomain, ModFlags, ModGroup, ModKind, ModStat, SpawnWeight,
    };
    use poc2_engine::patch::PatchRange;
    use smallvec::smallvec;

    use super::*;
    use crate::dsl::{CmpOp, ValuePredicate};

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
        let registry = ModRegistry::from_mods(vec![mod_es, mod_hybrid]);

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
        assert!(eval(
            &ItemPredicate::Ilvl(ValuePredicate {
                op: CmpOp::Gte,
                value: 82
            }),
            &item,
            &reg
        ));
        assert!(!eval(
            &ItemPredicate::Ilvl(ValuePredicate {
                op: CmpOp::Gte,
                value: 83
            }),
            &item,
            &reg
        ));
    }

    #[test]
    fn eval_rarity() {
        let (item, reg) = fixture_es_armour_with_es_prefix();
        assert!(eval(&ItemPredicate::Rarity(Rarity::Rare), &item, &reg));
        assert!(!eval(&ItemPredicate::Rarity(Rarity::Magic), &item, &reg));
    }

    #[test]
    fn eval_has_concept_atomic() {
        let (item, reg) = fixture_es_armour_with_es_prefix();
        assert!(eval(
            &ItemPredicate::HasConcept {
                concept: ConceptId::from("EnergyShield"),
                affix: None,
                min_tier: None,
            },
            &item,
            &reg
        ));
        assert!(eval(
            &ItemPredicate::HasConcept {
                concept: ConceptId::from("EnergyShield"),
                affix: Some(AffixType::Prefix),
                min_tier: None,
            },
            &item,
            &reg
        ));
        assert!(!eval(
            &ItemPredicate::HasConcept {
                concept: ConceptId::from("EnergyShield"),
                affix: Some(AffixType::Suffix),
                min_tier: None,
            },
            &item,
            &reg
        ));
    }

    #[test]
    fn eval_has_concept_hybrid_satisfies_es_target() {
        // Replace the prefix with the hybrid mod and verify the predicate
        // still matches EnergyShield (the user's "T1 ES flat or hybrid"
        // target case).
        let (mut item, reg) = fixture_es_armour_with_es_prefix();
        item.prefixes[0].mod_id = ModId::from("EsLifeHybrid1");
        assert!(eval(
            &ItemPredicate::HasConcept {
                concept: ConceptId::from("EnergyShield"),
                affix: Some(AffixType::Prefix),
                min_tier: None,
            },
            &item,
            &reg
        ));
        // And the same hybrid satisfies a Life predicate too.
        assert!(eval(
            &ItemPredicate::HasConcept {
                concept: ConceptId::from("Life"),
                affix: Some(AffixType::Prefix),
                min_tier: None,
            },
            &item,
            &reg
        ));
    }

    #[test]
    fn eval_compound_all_any_not() {
        let (item, reg) = fixture_es_armour_with_es_prefix();
        let p = ItemPredicate::All(vec![
            ItemPredicate::Ilvl(ValuePredicate {
                op: CmpOp::Gte,
                value: 82,
            }),
            ItemPredicate::Rarity(Rarity::Rare),
            ItemPredicate::Not(Box::new(ItemPredicate::Corrupted(true))),
        ]);
        assert!(eval(&p, &item, &reg));

        let p = ItemPredicate::Any(vec![
            ItemPredicate::Rarity(Rarity::Magic),
            ItemPredicate::Corrupted(false),
        ]);
        assert!(eval(&p, &item, &reg));
    }

    #[test]
    fn eval_affix_count_uses_value_predicate() {
        let (item, reg) = fixture_es_armour_with_es_prefix();
        // 1 prefix, 0 suffixes
        assert!(eval(
            &ItemPredicate::AffixCount {
                affix: AffixType::Prefix,
                count: ValuePredicate {
                    op: CmpOp::Eq,
                    value: 1
                },
            },
            &item,
            &reg
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
            &reg
        ));
    }

    #[test]
    fn eval_state_predicates() {
        let (mut item, reg) = fixture_es_armour_with_es_prefix();
        assert!(eval(&ItemPredicate::HasFractured(false), &item, &reg));
        item.prefixes[0].is_fractured = true;
        assert!(eval(&ItemPredicate::HasFractured(true), &item, &reg));

        assert!(eval(
            &ItemPredicate::HasHiddenDesecrated(false),
            &item,
            &reg
        ));
        assert!(eval(&ItemPredicate::HasHinekoraLock(false), &item, &reg));
        item.hinekora_lock = Some(42);
        assert!(eval(&ItemPredicate::HasHinekoraLock(true), &item, &reg));
    }

    #[test]
    fn always_never() {
        let (item, reg) = fixture_es_armour_with_es_prefix();
        assert!(eval(&ItemPredicate::Always, &item, &reg));
        assert!(!eval(&ItemPredicate::Never, &item, &reg));
    }
}
