//! [`Goal`] — what the user is crafting toward.
//!
//! A goal bundles three pieces of state the advisor needs to plan:
//!
//! 1. **Target** — what mods the finished item must have (concept-based,
//!    hybrid-aware, reusing [`poc2_strategies::Target`]).
//! 2. **Abandon criteria** — predicates that, if true, mean we should
//!    stop crafting (corrupted, sanctified, mirrored, or a budget cap).
//! 3. **Budget** — divine-equivalent ceiling on total spend.
//!
//! [`is_satisfied`] checks whether an [`Item`] meets the goal's mod
//! requirements (used by the planner to detect terminal nodes).

use poc2_engine::ids::ConceptId;
#[cfg(test)]
use poc2_engine::item::AffixType;
use poc2_engine::item::{Item, ModRoll};
use poc2_engine::registry::ModRegistry;
use poc2_market::DivEquiv;
use poc2_strategies::{eval_all, ItemPredicate, Target, TargetSpec};
use serde::{Deserialize, Serialize};

/// A complete goal description.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Goal {
    /// Required prefixes / suffixes / constraints.
    pub target: Target,
    /// Predicates that, when true, abandon the craft.
    #[serde(default)]
    pub abandon_criteria: Vec<ItemPredicate>,
    /// Divine-equivalent ceiling on total spend.
    pub budget: DivEquiv,
}

impl Goal {
    /// Build a goal from just a target + budget. No abandon criteria.
    #[must_use]
    pub fn new(target: Target, budget: DivEquiv) -> Self {
        Self {
            target,
            abandon_criteria: Vec::new(),
            budget,
        }
    }

    /// Convenience: empty target, budget only (used in tests).
    #[must_use]
    pub fn empty(budget: DivEquiv) -> Self {
        Self::new(Target::default(), budget)
    }
}

/// True iff the item meets every [`TargetSpec`] in the goal's target.
///
/// A [`TargetSpec`] matches when at least `count` mods on the relevant
/// affix slot satisfy the concept (or any concept in `concept_any`),
/// honoring `allow_hybrid`. Tier checks are skipped when `min_tier` is
/// `None` or no tier metadata is yet available — the M5 weights pass
/// will refine this.
#[must_use]
pub fn is_satisfied(goal: &Goal, item: &Item, registry: &ModRegistry) -> bool {
    for spec in &goal.target.prefixes {
        if !spec_satisfied(spec, &item.prefixes, registry) {
            return false;
        }
    }
    for spec in &goal.target.suffixes {
        if !spec_satisfied(spec, &item.suffixes, registry) {
            return false;
        }
    }
    if !goal.target.constraints.is_empty() && !eval_all(&goal.target.constraints, item, registry) {
        return false;
    }
    true
}

/// True iff the item should be abandoned (any abandon predicate true).
#[must_use]
pub fn should_abandon(goal: &Goal, item: &Item, registry: &ModRegistry) -> bool {
    goal.abandon_criteria
        .iter()
        .any(|p| poc2_strategies::eval(p, item, registry))
}

/// Is a single [`TargetSpec`] satisfied by a slot's mods?
fn spec_satisfied(spec: &TargetSpec, slot: &[ModRoll], registry: &ModRegistry) -> bool {
    // Build the candidate concept set.
    let mut concepts: Vec<&ConceptId> = Vec::new();
    if let Some(c) = &spec.concept {
        concepts.push(c);
    }
    for c in &spec.concept_any {
        if !concepts.contains(&c) {
            concepts.push(c);
        }
    }

    let mut matches = 0_u8;
    for roll in slot {
        // If a specific affix is required and this slot doesn't match it,
        // skip. (slot-level filtering is handled by the caller — we
        // already get prefix vs suffix slices — so this branch is moot
        // unless someone constructs a TargetSpec with affix=Implicit etc.)
        if let Some(req) = spec.affix {
            if roll.affix_type != req {
                continue;
            }
        }
        let Some(def) = registry.get(&roll.mod_id) else {
            continue;
        };
        // Hybrid handling: if allow_hybrid is false, hybrid mods don't
        // count toward concept matches. If allow_hybrid is true, every
        // entry in the mod's concept_set counts.
        let is_hybrid = def.flags.contains(poc2_engine::mods::ModFlags::HYBRID);
        if is_hybrid && !spec.allow_hybrid {
            continue;
        }
        let hits = def.concept_set.iter().any(|c| concepts.contains(&c));
        if hits {
            matches += 1;
            if matches >= spec.count {
                return true;
            }
        }
    }
    spec.count == 0 || matches >= spec.count
}

#[cfg(test)]
mod tests {
    use super::*;
    use poc2_engine::ids::{ItemClassId, ModGroupId, ModId, StatId, TagId};
    use poc2_engine::item::{ModRoll, QualityKind, Rarity};
    use poc2_engine::mods::{
        ModDefinition, ModDomain, ModFlags, ModGroup, ModKind, ModStat, SpawnWeight,
    };
    use poc2_engine::patch::PatchRange;
    use smallvec::smallvec;

    fn mk_es_mod(id: &str, hybrid: bool) -> ModDefinition {
        let concept_set = if hybrid {
            smallvec![
                poc2_engine::ConceptId::from("EnergyShield"),
                poc2_engine::ConceptId::from("Life"),
            ]
        } else {
            smallvec![poc2_engine::ConceptId::from("EnergyShield")]
        };
        let flags = if hybrid {
            ModFlags::HYBRID
        } else {
            ModFlags::empty()
        };
        ModDefinition {
            id: ModId::from(id),
            name: None,
            mod_group: ModGroup(ModGroupId::from(format!("ES-{id}"))),
            affix_type: AffixType::Prefix,
            kind: ModKind::Explicit,
            domain: ModDomain::Item,
            tags: smallvec![],
            concept_set,
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
            flags,
            text_template: None,
        }
    }

    fn mk_item_with_prefixes(prefixes: Vec<ModRoll>) -> Item {
        Item {
            base: ItemClassId::from("BodyArmour").as_str().into(),
            ilvl: 82,
            rarity: Rarity::Rare,
            corrupted: false,
            sanctified: false,
            mirrored: false,
            quality: 0,
            quality_kind: QualityKind::Untagged,
            implicits: smallvec![],
            prefixes: prefixes.into_iter().collect(),
            suffixes: smallvec![],
            enchantments: smallvec![],
            hidden_desecrated: None,
            sockets: smallvec![],
            hinekora_lock: None,
        }
    }

    fn roll(mod_id: &str) -> ModRoll {
        ModRoll {
            mod_id: ModId::from(mod_id),
            affix_type: AffixType::Prefix,
            kind: ModKind::Explicit,
            values: smallvec![60.0],
            is_fractured: false,
        }
    }

    #[test]
    fn empty_target_is_always_satisfied() {
        let goal = Goal::empty(DivEquiv::point(10.0));
        let item = mk_item_with_prefixes(vec![]);
        let reg = ModRegistry::from_mods(vec![]);
        assert!(is_satisfied(&goal, &item, &reg));
    }

    #[test]
    fn three_es_prefixes_satisfy_count_three() {
        let target = Target {
            prefixes: vec![TargetSpec {
                concept: Some(poc2_engine::ConceptId::from("EnergyShield")),
                concept_any: vec![],
                affix: None,
                count: 3,
                min_tier: Some(1),
                allow_hybrid: true,
            }],
            suffixes: vec![],
            constraints: vec![],
        };
        let goal = Goal::new(target, DivEquiv::point(100.0));
        let reg = ModRegistry::from_mods(vec![
            mk_es_mod("ES1", false),
            mk_es_mod("ES2", false),
            mk_es_mod("ES3", false),
        ]);
        let item = mk_item_with_prefixes(vec![roll("ES1"), roll("ES2"), roll("ES3")]);
        assert!(is_satisfied(&goal, &item, &reg));
    }

    #[test]
    fn hybrid_es_life_counts_when_allowed() {
        let target = Target {
            prefixes: vec![TargetSpec {
                concept: Some(poc2_engine::ConceptId::from("EnergyShield")),
                concept_any: vec![],
                affix: None,
                count: 1,
                min_tier: None,
                allow_hybrid: true,
            }],
            suffixes: vec![],
            constraints: vec![],
        };
        let goal = Goal::new(target, DivEquiv::point(10.0));
        let reg = ModRegistry::from_mods(vec![mk_es_mod("HYB1", true)]);
        let item = mk_item_with_prefixes(vec![roll("HYB1")]);
        assert!(is_satisfied(&goal, &item, &reg));
    }

    #[test]
    fn hybrid_excluded_when_not_allowed() {
        let target = Target {
            prefixes: vec![TargetSpec {
                concept: Some(poc2_engine::ConceptId::from("EnergyShield")),
                concept_any: vec![],
                affix: None,
                count: 1,
                min_tier: None,
                allow_hybrid: false,
            }],
            suffixes: vec![],
            constraints: vec![],
        };
        let goal = Goal::new(target, DivEquiv::point(10.0));
        let reg = ModRegistry::from_mods(vec![mk_es_mod("HYB1", true)]);
        let item = mk_item_with_prefixes(vec![roll("HYB1")]);
        assert!(!is_satisfied(&goal, &item, &reg));
    }

    #[test]
    fn should_abandon_on_corrupted() {
        let goal = Goal {
            target: Target::default(),
            abandon_criteria: vec![ItemPredicate::Corrupted(true)],
            budget: DivEquiv::point(50.0),
        };
        let mut item = mk_item_with_prefixes(vec![]);
        let reg = ModRegistry::from_mods(vec![]);
        assert!(!should_abandon(&goal, &item, &reg));
        item.corrupted = true;
        assert!(should_abandon(&goal, &item, &reg));
    }
}
