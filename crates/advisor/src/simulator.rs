//! Single-step simulation for the advisor.
//!
//! Given an item state and an [`AdvisorAction`], the simulator runs the
//! action through the engine ONCE with a deterministic RNG seed and
//! returns the post-state plus an indicator of whether the action
//! succeeded. The planner runs the simulator multiple times per
//! candidate when it wants Monte Carlo probability estimates.
//!
//! Reveal actions are simulated using [`poc2_engine::sample_reveal_options`]
//! with a tiny placeholder mod pool (TBD when poe2db data lands); for v1
//! the planner treats Reveal as a side-effect-only step (state change but
//! probability is opaque).

use poc2_engine::currency::CurrencyResolver;
use poc2_engine::error::EngineError;
use poc2_engine::item::Item;
use poc2_engine::omen::{Omen, OmenSet};
use poc2_engine::patch::PatchVersion;
use poc2_engine::registry::ModRegistry;
use rand::SeedableRng;
use rand_xoshiro::Xoshiro256PlusPlus;

use crate::action::AdvisorAction;

/// Result of one simulator step.
#[derive(Debug, Clone)]
pub struct SimulationOutcome {
    /// Resulting item state (post-apply).
    pub item: Item,
    /// True iff the engine accepted the action.
    pub success: bool,
    /// Engine error, when `success == false`.
    pub error: Option<String>,
    /// Number of mod-affecting changes the action made (0 for guidance/stop).
    pub change_count: u32,
}

/// Simulate an action against the engine. Returns a clone of the input
/// item with the action applied (or unchanged on failure).
///
/// Determinism: the RNG is seeded by `rng_seed`, so given the same input
/// state and seed, repeated calls return identical outcomes.
#[must_use]
pub fn simulate(
    item: &Item,
    action: &AdvisorAction,
    omens_in: &OmenSet,
    registry: &ModRegistry,
    resolver: &dyn CurrencyResolver,
    patch: PatchVersion,
    rng_seed: u64,
) -> SimulationOutcome {
    let mut item = item.clone();
    let mut rng = Xoshiro256PlusPlus::seed_from_u64(rng_seed);

    match action {
        AdvisorAction::ApplyCurrency { currency, omens } => {
            let mut omen_set = omens_in.clone();
            // Push any pre-activated omens. Stash-checked upstream.
            for oid in omens {
                if let Some(o) = build_omen_for_id(oid) {
                    omen_set.push(o);
                }
            }
            let Some(c) = resolver.resolve(currency) else {
                return SimulationOutcome {
                    item,
                    success: false,
                    error: Some(format!("unknown currency: {currency}")),
                    change_count: 0,
                };
            };
            let before = describe_item_state(&item);
            let result = poc2_engine::apply_currency(
                c.as_ref(),
                &mut item,
                registry,
                &mut rng,
                patch,
                &mut omen_set,
            );
            match result {
                Ok(()) => {
                    let after = describe_item_state(&item);
                    SimulationOutcome {
                        item,
                        success: true,
                        error: None,
                        change_count: state_diff_count(&before, &after),
                    }
                }
                Err(e) => SimulationOutcome {
                    item,
                    success: false,
                    error: Some(format_engine_error(&e)),
                    change_count: 0,
                },
            }
        }
        AdvisorAction::ApplyHinekorasLock => {
            let mut omen_set = omens_in.clone();
            let lock = poc2_engine::HinekorasLock::new();
            let result = poc2_engine::apply_currency(
                &lock,
                &mut item,
                registry,
                &mut rng,
                patch,
                &mut omen_set,
            );
            match result {
                Ok(()) => SimulationOutcome {
                    item,
                    success: true,
                    error: None,
                    change_count: 1,
                },
                Err(e) => SimulationOutcome {
                    item,
                    success: false,
                    error: Some(format_engine_error(&e)),
                    change_count: 0,
                },
            }
        }
        AdvisorAction::Reveal { .. } => {
            // The reveal mechanic needs a desecrated mod pool; until
            // poe2db data is integrated, we can only treat Reveal as a
            // marker step that "succeeds" if a hidden_desecrated slot
            // exists. The slot is left in place — converting it to a
            // ModRoll requires the pool. M5+ wires real reveal pools.
            if item.hidden_desecrated.is_some() {
                SimulationOutcome {
                    item,
                    success: true,
                    error: None,
                    change_count: 0,
                }
            } else {
                SimulationOutcome {
                    item,
                    success: false,
                    error: Some("no hidden desecrated mod to reveal".into()),
                    change_count: 0,
                }
            }
        }
        AdvisorAction::Stop | AdvisorAction::Abandon { .. } | AdvisorAction::Guidance { .. } => {
            SimulationOutcome {
                item,
                success: true,
                error: None,
                change_count: 0,
            }
        }
    }
}

fn format_engine_error(e: &EngineError) -> String {
    format!("{e}")
}

/// A compact fingerprint of the item's mutable state. Used to detect
/// whether an action mutated anything.
fn describe_item_state(item: &Item) -> (poc2_engine::Rarity, usize, usize, usize, bool, bool) {
    (
        item.rarity,
        item.prefixes.len(),
        item.suffixes.len(),
        item.implicits.len(),
        item.hidden_desecrated.is_some(),
        item.corrupted,
    )
}

fn state_diff_count(
    before: &(poc2_engine::Rarity, usize, usize, usize, bool, bool),
    after: &(poc2_engine::Rarity, usize, usize, usize, bool, bool),
) -> u32 {
    let mut d = 0;
    if before.0 != after.0 {
        d += 1;
    }
    if before.1 != after.1 {
        d += 1;
    }
    if before.2 != after.2 {
        d += 1;
    }
    if before.3 != after.3 {
        d += 1;
    }
    if before.4 != after.4 {
        d += 1;
    }
    if before.5 != after.5 {
        d += 1;
    }
    d
}

/// Resolve an [`OmenId`] string to the matching [`Omen`] preset.
///
/// Returns `None` for unknown omens; the simulator silently drops them.
/// Real omen resolution will move into the engine's `Omen` type with a
/// proper `from_id()` constructor in M2.6 polish.
fn build_omen_for_id(id: &poc2_engine::OmenId) -> Option<Omen> {
    let s = id.as_str();
    match s {
        "OmenOfSinistralExaltation" => Some(Omen::sinistral_exaltation()),
        "OmenOfDextralExaltation" => Some(Omen::dextral_exaltation()),
        "OmenOfGreaterExaltation" => Some(Omen::greater_exaltation()),
        "OmenOfWhittling" => Some(Omen::whittling()),
        "OmenOfCorruption" => Some(Omen::corruption()),
        "OmenOfSinistralCrystallisation" => Some(Omen::sinistral_crystallisation()),
        "OmenOfDextralCrystallisation" => Some(Omen::dextral_crystallisation()),
        "OmenOfSinistralNecromancy" => Some(Omen::sinistral_necromancy()),
        "OmenOfDextralNecromancy" => Some(Omen::dextral_necromancy()),
        "OmenOfAbyssalEchoes" => Some(Omen::abyssal_echoes()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use poc2_engine::currency::DefaultCurrencyResolver;
    use poc2_engine::ids::{CurrencyId, ItemClassId, ModGroupId, ModId, TagId};
    use poc2_engine::item::{AffixType, QualityKind, Rarity};
    use poc2_engine::mods::{ModDefinition, ModDomain, ModFlags, ModGroup, ModKind, SpawnWeight};
    use poc2_engine::patch::PatchRange;
    use smallvec::smallvec;

    fn registry() -> ModRegistry {
        ModRegistry::from_mods(vec![
            ModDefinition {
                id: ModId::from("Life1"),
                name: None,
                mod_group: ModGroup(ModGroupId::from("Life")),
                affix_type: AffixType::Prefix,
                kind: ModKind::Explicit,
                domain: ModDomain::Item,
                tags: smallvec![],
                concept_set: smallvec![],
                spawn_weights: smallvec![SpawnWeight {
                    tag: TagId::from("BodyArmour"),
                    weight: 1
                }],
                stats: smallvec![],
                required_level: 1,
                allowed_item_classes: smallvec![ItemClassId::from("BodyArmour")],
                patch_range: PatchRange::ALL,
                flags: ModFlags::empty(),
                text_template: None,
            },
            ModDefinition {
                id: ModId::from("FireRes1"),
                name: None,
                mod_group: ModGroup(ModGroupId::from("FireRes")),
                affix_type: AffixType::Suffix,
                kind: ModKind::Explicit,
                domain: ModDomain::Item,
                tags: smallvec![],
                concept_set: smallvec![],
                spawn_weights: smallvec![SpawnWeight {
                    tag: TagId::from("BodyArmour"),
                    weight: 1
                }],
                stats: smallvec![],
                required_level: 1,
                allowed_item_classes: smallvec![ItemClassId::from("BodyArmour")],
                patch_range: PatchRange::ALL,
                flags: ModFlags::empty(),
                text_template: None,
            },
        ])
    }

    fn empty_item() -> Item {
        Item {
            base: ItemClassId::from("BodyArmour").as_str().into(),
            ilvl: 82,
            rarity: Rarity::Normal,
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
    fn simulate_transmute_promotes_to_magic() {
        let reg = registry();
        let resolver = DefaultCurrencyResolver::new();
        let item = empty_item();
        let omens = OmenSet::new();
        let result = simulate(
            &item,
            &AdvisorAction::ApplyCurrency {
                currency: CurrencyId::from("OrbOfTransmutation"),
                omens: vec![],
            },
            &omens,
            &reg,
            &resolver,
            PatchVersion::PATCH_0_4_0,
            42,
        );
        assert!(result.success, "got error: {:?}", result.error);
        assert_eq!(result.item.rarity, Rarity::Magic);
        assert!(result.change_count >= 1);
    }

    #[test]
    fn simulate_unknown_currency_fails() {
        let reg = registry();
        let resolver = DefaultCurrencyResolver::new();
        let item = empty_item();
        let omens = OmenSet::new();
        let result = simulate(
            &item,
            &AdvisorAction::ApplyCurrency {
                currency: CurrencyId::from("UnknownCurrency123"),
                omens: vec![],
            },
            &omens,
            &reg,
            &resolver,
            PatchVersion::PATCH_0_4_0,
            42,
        );
        assert!(!result.success);
        assert!(result.error.is_some());
    }

    #[test]
    fn simulate_apply_lock_succeeds() {
        let reg = registry();
        let resolver = DefaultCurrencyResolver::new();
        let item = empty_item();
        let omens = OmenSet::new();
        let result = simulate(
            &item,
            &AdvisorAction::ApplyHinekorasLock,
            &omens,
            &reg,
            &resolver,
            PatchVersion::PATCH_0_4_0,
            42,
        );
        assert!(result.success);
        assert!(result.item.hinekora_lock.is_some());
    }

    #[test]
    fn simulate_terminal_actions_are_noops() {
        let reg = registry();
        let resolver = DefaultCurrencyResolver::new();
        let item = empty_item();
        let omens = OmenSet::new();
        let stop = simulate(
            &item,
            &AdvisorAction::Stop,
            &omens,
            &reg,
            &resolver,
            PatchVersion::PATCH_0_4_0,
            1,
        );
        assert!(stop.success);
        assert_eq!(stop.change_count, 0);

        let abandon = simulate(
            &item,
            &AdvisorAction::Abandon {
                reason: "test".into(),
            },
            &omens,
            &reg,
            &resolver,
            PatchVersion::PATCH_0_4_0,
            1,
        );
        assert!(abandon.success);
    }

    #[test]
    fn simulate_deterministic_for_same_seed() {
        let reg = registry();
        let resolver = DefaultCurrencyResolver::new();
        let item = empty_item();
        let omens = OmenSet::new();
        let action = AdvisorAction::ApplyCurrency {
            currency: CurrencyId::from("OrbOfTransmutation"),
            omens: vec![],
        };
        let a = simulate(
            &item,
            &action,
            &omens,
            &reg,
            &resolver,
            PatchVersion::PATCH_0_4_0,
            7,
        );
        let b = simulate(
            &item,
            &action,
            &omens,
            &reg,
            &resolver,
            PatchVersion::PATCH_0_4_0,
            7,
        );
        assert_eq!(a.item, b.item);
        assert_eq!(a.success, b.success);
    }
}
