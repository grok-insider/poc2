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

/// Aggregated outcome from running an action `n` times with different
/// seeds (Phase C.1 Monte Carlo aggregator).
#[derive(Debug, Clone)]
pub struct McOutcome {
    /// Mean success probability across the `n` samples in `[0, 1]`.
    pub mean_success_prob: f64,
    /// Standard error of the mean (sqrt(p*(1-p)/n)) — surfaces as the
    /// `± stderr` band the UI renders.
    pub prob_stderr: f64,
    /// Mean of `change_count` across all samples.
    pub mean_change_count: f64,
    /// Number of samples that ran (always equal to the request).
    pub n_samples: u32,
    /// One representative outcome — the deterministic-seed run, used
    /// as the planner's "what state will the user reach" proxy. Beam
    /// search needs a single canonical post-state per node; the MC
    /// aggregator only refines the *probability* of getting there.
    pub primary: SimulationOutcome,
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
    let item = item.clone();
    let mut rng = Xoshiro256PlusPlus::seed_from_u64(rng_seed);

    match action {
        AdvisorAction::ApplyCurrency { currency, omens } => simulate_apply_currency(
            item, currency, omens, omens_in, registry, resolver, patch, &mut rng,
        ),
        AdvisorAction::ApplyHinekorasLock => {
            simulate_apply_lock(item, omens_in, registry, patch, &mut rng)
        }
        AdvisorAction::Reveal { .. } => simulate_reveal(item),
        AdvisorAction::ActivateOmen { .. } => noop_success(item),
        AdvisorAction::Recombine { .. } => SimulationOutcome {
            item,
            success: false,
            error: Some("Recombine simulation requires a second item (deferred to Phase F)".into()),
            change_count: 0,
        },
        AdvisorAction::Stop | AdvisorAction::Abandon { .. } | AdvisorAction::Guidance { .. } => {
            noop_success(item)
        }
        AdvisorAction::Recurring { inner, .. } => {
            // Simulate ONE pass through the inner loop body. The planner's
            // Monte Carlo aggregator (`simulate_n`) calls this multiple
            // times; the per-iteration success rate it derives is what
            // the loop estimator (Phase B.4) uses to compute mean
            // iterations. A pass succeeds when every leaf step in
            // `inner` succeeds; the first failure short-circuits.
            simulate_recurring_pass(item, inner, omens_in, registry, resolver, patch, &mut rng)
        }
    }
}

/// One pass of a [`AdvisorAction::Recurring`] body. Each leaf step is
/// simulated in sequence with the previous outcome's item; the pass
/// fails on the first step that fails. The change count is summed
/// across leaf steps.
fn simulate_recurring_pass(
    mut item: Item,
    inner: &[AdvisorAction],
    omens_in: &OmenSet,
    registry: &ModRegistry,
    resolver: &dyn CurrencyResolver,
    patch: PatchVersion,
    rng: &mut Xoshiro256PlusPlus,
) -> SimulationOutcome {
    let mut total_changes: u32 = 0;
    for (idx, leaf) in inner.iter().enumerate() {
        // Derive a per-step seed from the rng so every leaf gets a
        // distinct deterministic stream. We can't pass a `&mut rng`
        // through `simulate` (it constructs its own from `u64`), so
        // we mint a fresh seed per leaf.
        let seed = {
            use rand::RngCore;
            rng.next_u64().wrapping_add(idx as u64)
        };
        let leaf_outcome = simulate(&item, leaf, omens_in, registry, resolver, patch, seed);
        if !leaf_outcome.success {
            return SimulationOutcome {
                item: leaf_outcome.item,
                success: false,
                error: leaf_outcome.error,
                change_count: total_changes,
            };
        }
        total_changes = total_changes.saturating_add(leaf_outcome.change_count);
        item = leaf_outcome.item;
    }
    SimulationOutcome {
        item,
        success: true,
        error: None,
        change_count: total_changes,
    }
}

#[allow(clippy::too_many_arguments)] // module-internal helper, mirrors apply_currency contract
fn simulate_apply_currency(
    mut item: Item,
    currency: &poc2_engine::ids::CurrencyId,
    omens: &[poc2_engine::ids::OmenId],
    omens_in: &OmenSet,
    registry: &ModRegistry,
    resolver: &dyn CurrencyResolver,
    patch: PatchVersion,
    rng: &mut Xoshiro256PlusPlus,
) -> SimulationOutcome {
    let mut omen_set = omens_in.clone();
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
    let result =
        poc2_engine::apply_currency(c.as_ref(), &mut item, registry, rng, patch, &mut omen_set);
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

fn simulate_apply_lock(
    mut item: Item,
    omens_in: &OmenSet,
    registry: &ModRegistry,
    patch: PatchVersion,
    rng: &mut Xoshiro256PlusPlus,
) -> SimulationOutcome {
    let mut omen_set = omens_in.clone();
    let lock = poc2_engine::HinekorasLock::new();
    let result = poc2_engine::apply_currency(&lock, &mut item, registry, rng, patch, &mut omen_set);
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

/// Reveal needs a desecrated mod pool; until poe2db data is integrated,
/// the simulator treats Reveal as a marker step that "succeeds" if a
/// hidden_desecrated slot exists. The slot is left in place — converting
/// it to a ModRoll requires the pool. Phase F+ wires real reveal pools.
fn simulate_reveal(item: Item) -> SimulationOutcome {
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

fn noop_success(item: Item) -> SimulationOutcome {
    SimulationOutcome {
        item,
        success: true,
        error: None,
        change_count: 0,
    }
}

/// Run `n` independent simulations of `action` against `item` (each
/// with a different RNG seed derived from `rng_seed_base`) and
/// aggregate into a [`McOutcome`].
///
/// `n_samples = 1` collapses to deterministic behaviour: the
/// `primary` field is the single sample, mean_success_prob is 0 or 1,
/// and prob_stderr is 0.
///
/// Per Phase C.1's perf budget: at depth-3 with 50 MC samples the
/// planner should stay under 5 ms (advisor_plan benches verify).
#[allow(clippy::too_many_arguments)] // mirrors `simulate`'s API contract
#[must_use]
pub fn simulate_n(
    item: &Item,
    action: &AdvisorAction,
    omens_in: &OmenSet,
    registry: &ModRegistry,
    resolver: &dyn CurrencyResolver,
    patch: PatchVersion,
    rng_seed_base: u64,
    n_samples: u32,
) -> McOutcome {
    let n_clamped = n_samples.max(1);
    // Run the canonical seed-0 sample first; every node in the beam
    // shares this representative state regardless of mc_samples.
    let primary = simulate(
        item,
        action,
        omens_in,
        registry,
        resolver,
        patch,
        rng_seed_base,
    );

    let mut successes: u32 = u32::from(primary.success);
    let mut sum_change_count: u32 = primary.change_count;
    for i in 1..n_clamped {
        let seed = rng_seed_base.wrapping_add(u64::from(i).wrapping_mul(0x9E37_79B9_7F4A_7C15));
        let out = simulate(item, action, omens_in, registry, resolver, patch, seed);
        if out.success {
            successes += 1;
        }
        sum_change_count = sum_change_count.saturating_add(out.change_count);
    }
    let n_f = f64::from(n_clamped);
    let p = f64::from(successes) / n_f;
    let stderr = if n_clamped <= 1 {
        0.0
    } else {
        (p * (1.0 - p) / n_f).sqrt()
    };
    let mean_change = f64::from(sum_change_count) / n_f;

    McOutcome {
        mean_success_prob: p,
        prob_stderr: stderr,
        mean_change_count: mean_change,
        n_samples: n_clamped,
        primary,
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
        "OmenOfSinistralAnnulment" => Some(Omen::sinistral_annulment()),
        "OmenOfDextralAnnulment" => Some(Omen::dextral_annulment()),
        "OmenOfSinistralErasure" => Some(Omen::sinistral_erasure()),
        "OmenOfDextralErasure" => Some(Omen::dextral_erasure()),
        "OmenOfWhittling" => Some(Omen::whittling()),
        "OmenOfLight" => Some(Omen::light()),
        "OmenOfCorruption" => Some(Omen::corruption()),
        "OmenOfSanctification" => Some(Omen::sanctification()),
        "OmenOfTheBlessed" => Some(Omen::blessed()),
        "OmenOfCatalysingExaltation" => Some(Omen::catalysing_exaltation()),
        "OmenOfSinistralCrystallisation" => Some(Omen::sinistral_crystallisation()),
        "OmenOfDextralCrystallisation" => Some(Omen::dextral_crystallisation()),
        "OmenOfSinistralNecromancy" => Some(Omen::sinistral_necromancy()),
        "OmenOfDextralNecromancy" => Some(Omen::dextral_necromancy()),
        "OmenOfAbyssalEchoes" => Some(Omen::abyssal_echoes()),
        "OmenOfTheBlackblooded" => Some(Omen::blackblooded()),
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
        ModRegistry::from_mods(
            vec![
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
                    tier: None,
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
                    tier: None,
                    allowed_item_classes: smallvec![ItemClassId::from("BodyArmour")],
                    patch_range: PatchRange::ALL,
                    flags: ModFlags::empty(),
                    text_template: None,
                },
            ],
            vec![],
        )
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
    fn simulate_n_with_one_sample_collapses_to_deterministic() {
        let reg = registry();
        let resolver = DefaultCurrencyResolver::new();
        let item = empty_item();
        let omens = OmenSet::new();
        let action = AdvisorAction::ApplyCurrency {
            currency: CurrencyId::from("OrbOfTransmutation"),
            omens: vec![],
        };
        let mc = simulate_n(
            &item,
            &action,
            &omens,
            &reg,
            &resolver,
            PatchVersion::PATCH_0_4_0,
            42,
            1,
        );
        assert_eq!(mc.n_samples, 1);
        assert!(mc.prob_stderr.abs() < 1e-12);
        assert!((mc.mean_success_prob - if mc.primary.success { 1.0 } else { 0.0 }).abs() < 1e-12);
    }

    #[test]
    fn simulate_n_50_samples_produces_stable_estimate() {
        let reg = registry();
        let resolver = DefaultCurrencyResolver::new();
        let item = empty_item();
        let omens = OmenSet::new();
        // OrbOfTransmutation always succeeds on Normal items, so the
        // MC estimate is 1.0 with stderr 0.
        let action = AdvisorAction::ApplyCurrency {
            currency: CurrencyId::from("OrbOfTransmutation"),
            omens: vec![],
        };
        let mc = simulate_n(
            &item,
            &action,
            &omens,
            &reg,
            &resolver,
            PatchVersion::PATCH_0_4_0,
            7,
            50,
        );
        assert_eq!(mc.n_samples, 50);
        assert!((mc.mean_success_prob - 1.0).abs() < 1e-9);
        assert!(mc.prob_stderr.abs() < 1e-9);
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
