//! Recombinator — combine two Rare items into one (PoE2 0.4).
//!
//! ## Mechanic
//!
//! The Recombinator takes two source items of the same item-class and
//! produces a single output item drawn from the combined mod pool of
//! both inputs. Both sources are consumed.
//!
//! ## Inputs
//!
//! - Both items must be Rare.
//! - Both items must NOT be corrupted, sanctified, or mirrored.
//! - Both items must share the same `base` (the recombinator combines
//!   like-with-like — the engine enforces same `BaseTypeId`).
//! - Hidden desecrated mods are NOT carried over (they require a real
//!   reveal first).
//! - Fractured mods on either input survive into the output and remain
//!   fractured.
//!
//! ## Output
//!
//! - Same base, same ilvl as the input.
//! - Rarity = Rare.
//! - Quality is reset to 0; quality-tagged catalyst boosts are lost.
//! - Mods: from the union of both inputs' explicit mod rolls, sampled
//!   per the wiki success-chance formula (M14.4). Fractured mods always
//!   carry over (they're "preserved" in the recipe). Mod-group
//!   exclusivity is honored: if two candidates share a group, only the
//!   first sampled one is kept.
//!
//! ## Wiki success formula (M14.4)
//!
//! Per the [poe2wiki Recombinator article](https://www.poe2wiki.net/wiki/Recombinator),
//! a recombine attempt either succeeds (producing the chosen output mod
//! set) or fails (both inputs consumed, no output). Success chance is:
//!
//! ```text
//! P(success) = clamp(a × c × Π_i (Σ_{j=m_i}^{m_t0} w_j / Z), 0, 1)
//! ```
//!
//! where:
//! - `a` is the per-base coefficient (Armour=10, Weapon=16, Quiver/Focus/Belt=12,
//!   Ring/Amulet=16). See [`BASE_COEFF_*`].
//! - `c` is the mod-count coefficient driven by the chosen output mod count.
//!   See [`MOD_COUNT_COEFF`].
//! - For each chosen output mod `i`, the inner term sums weights of
//!   tier-ladder peers from the chosen tier `m_i` up to the highest tier
//!   the base/ilvl admits (`m_t0`).
//! - `Z` is the total weight of mods of the same affix type that can roll
//!   on the selected base.
//!
//! The advisor surfaces the success chance through `LoopEstimate` so the
//! UI can show "expected attempts ≈ 1 / P(success)".
//!
//! ## API surface
//!
//! - [`recombine`] — back-compat unary entry point that returns the
//!   sampled `Item` regardless of the formula's success probability.
//!   Used by existing strategy-executor and test code paths that don't
//!   yet model the success/failure outcome.
//! - [`recombine_with_chance`] — M14.4 entry point that returns a
//!   [`RecombinatorOutcome`] (`Success(Item)` or `Failure`) per the wiki
//!   formula. Used by the advisor when computing per-attempt expected cost.
//! - [`compute_recombine_success_chance`] — pure-function formula
//!   accessible to the advisor for offline planning without RNG draws.

use rand::seq::SliceRandom;
use rand::Rng;
use smallvec::SmallVec;

use crate::error::{EngineError, EngineResult};
use crate::item::{AffixType, Item, ModRoll, Rarity};
use crate::registry::ModRegistry;

/// Maximum prefixes/suffixes a recombined output retains.
///
/// PoE2 caps Rare items at 3 prefixes + 3 suffixes; the recombinator
/// uses the same bound. Future work plumbs base-class-specific caps
/// through `BaseType::max_prefixes` once the BaseRegistry lands.
const RECOMBINE_MAX_PREFIXES: u8 = 3;
const RECOMBINE_MAX_SUFFIXES: u8 = 3;

/// Per-base recombinator coefficient (the `a` term in the wiki formula).
///
/// Source: poe2wiki Recombinator article §3 "Success Chance Formula".
pub const BASE_COEFF_ARMOUR: f64 = 10.0;
pub const BASE_COEFF_WEAPON: f64 = 16.0;
pub const BASE_COEFF_QUIVER_FOCUS_BELT: f64 = 12.0;
pub const BASE_COEFF_RING_AMULET: f64 = 16.0;
/// Default coefficient for item classes the wiki doesn't enumerate
/// (jewels, charms, tablets). Conservative — keeps recombines for
/// unsupported classes from over-succeeding while a richer table lands.
pub const BASE_COEFF_OTHER: f64 = 8.0;

/// Mod-count coefficient `c[k]` for an output of `k` mods.
///
/// Source: poe2wiki Recombinator article §3 "Mod Count Coefficient" table.
/// Indexed `[1, 6]` (mod count 0 returns 1.0 as the no-op identity).
const MOD_COUNT_COEFF: [f64; 7] = [
    1.0,  // k = 0 (no mods picked — formula is the empty product = 1)
    1.0,  // k = 1
    0.85, // k = 2
    0.65, // k = 3
    0.40, // k = 4
    0.20, // k = 5
    0.10, // k = 6
];

/// Resolve the recombinator's per-base coefficient `a` for a given item
/// class id.
///
/// Item-class strings are matched against the canonical PascalCase keys.
/// Unrecognized classes fall back to [`BASE_COEFF_OTHER`].
fn base_coeff_for_class(class: &str) -> f64 {
    match class {
        // Armour-class items
        "BodyArmour" | "Helmet" | "Boots" | "Gloves" => BASE_COEFF_ARMOUR,
        // Weapons (one- and two-hand, all flavours)
        "OneHandSword" | "TwoHandSword" | "OneHandAxe" | "TwoHandAxe" | "OneHandMace"
        | "TwoHandMace" | "Bow" | "Crossbow" | "Spear" | "Staff" | "Sceptre" | "Wand"
        | "Dagger" | "Claw" => BASE_COEFF_WEAPON,
        // Quiver / Focus / Belt
        "Quiver" | "Focus" | "Belt" => BASE_COEFF_QUIVER_FOCUS_BELT,
        // Jewellery
        "Ring" | "Amulet" => BASE_COEFF_RING_AMULET,
        _ => BASE_COEFF_OTHER,
    }
}

/// Outcome of a [`recombine_with_chance`] attempt (M14.4).
///
/// On `Failure` the inputs are still consumed — the caller is responsible
/// for discarding both stash items even though no output is produced.
/// This mirrors the in-game mechanic where a failed recombine destroys
/// both sources.
///
/// The `Success` variant boxes the produced [`Item`] so that the failure
/// path doesn't pad to the full item-state size; recombines are bursty
/// (many failures per success on hard recipes) so this keeps the
/// allocator footprint reasonable.
#[derive(Debug, Clone, PartialEq)]
pub enum RecombinatorOutcome {
    /// Recombine succeeded; the wrapped `Item` is the produced output.
    Success(Box<Item>),
    /// Recombine failed; both inputs are consumed and no output exists.
    Failure,
}

/// Compute the per-attempt success chance for a recombine that would
/// produce the given output mod set on the given base.
///
/// Pure function — no RNG draws. The advisor calls this for offline
/// loop-iteration estimation; [`recombine_with_chance`] consumes the same
/// chance for runtime sampling.
///
/// Returns a probability in `[0.0, 1.0]`. Returns `1.0` when the
/// computed value exceeds 1 (which can happen for small mod counts on
/// high-coefficient bases — the formula's intent is "guaranteed
/// success" in that regime).
pub fn compute_recombine_success_chance(
    base_class: &str,
    output_mod_count: usize,
    per_mod_tier_ratios: &[f64],
) -> f64 {
    let a = base_coeff_for_class(base_class);
    let c_idx = output_mod_count.min(MOD_COUNT_COEFF.len() - 1);
    let c = MOD_COUNT_COEFF[c_idx];
    let product: f64 = per_mod_tier_ratios.iter().product();
    (a * c * product).clamp(0.0, 1.0)
}

/// Recombine two Rare items into one. Returns the new Rare item;
/// callers are responsible for discarding `a` and `b`.
pub fn recombine(
    a: &Item,
    b: &Item,
    registry: &ModRegistry,
    rng: &mut dyn rand::RngCore,
) -> EngineResult<Item> {
    validate_inputs(a, b)?;

    // Build the combined mod pool. Fractured mods are pinned (always retained).
    let mut pinned: Vec<ModRoll> = Vec::new();
    let mut pool: Vec<ModRoll> = Vec::new();
    for slot in [&a.prefixes, &b.prefixes, &a.suffixes, &b.suffixes] {
        for roll in slot {
            if roll.is_fractured {
                pinned.push(roll.clone());
            } else {
                pool.push(roll.clone());
            }
        }
    }
    pool.shuffle(rng);

    // Decide the target mod-count (1..=6). Slightly biased toward 4-5
    // mods to mimic empirical PoE2 distributions: roll a u32 in [1, 6]
    // with extra weight on the middle bucket.
    let total_target = sample_recombine_count(rng);

    // Honor mod-group exclusivity: dedupe by group_id of each candidate
    // mod. Pinned mods take precedence (their groups are claimed first).
    let mut claimed_groups: ahash::AHashSet<crate::ids::ModGroupId> = ahash::AHashSet::new();
    let mut chosen_prefixes: SmallVec<[ModRoll; 3]> = SmallVec::new();
    let mut chosen_suffixes: SmallVec<[ModRoll; 3]> = SmallVec::new();

    let try_push = |roll: ModRoll,
                    claimed: &mut ahash::AHashSet<crate::ids::ModGroupId>,
                    prefixes: &mut SmallVec<[ModRoll; 3]>,
                    suffixes: &mut SmallVec<[ModRoll; 3]>|
     -> bool {
        let prefix_full = prefixes.len() >= RECOMBINE_MAX_PREFIXES as usize;
        let suffix_full = suffixes.len() >= RECOMBINE_MAX_SUFFIXES as usize;
        let Some(group) = registry.group_of(&roll.mod_id) else {
            // Unknown mod (out-of-bundle). Be conservative — don't drop;
            // include it but skip the group-exclusivity check.
            match roll.affix_type {
                AffixType::Prefix if !prefix_full => {
                    prefixes.push(roll);
                    return true;
                }
                AffixType::Suffix if !suffix_full => {
                    suffixes.push(roll);
                    return true;
                }
                _ => return false,
            }
        };
        if claimed.contains(group) {
            return false;
        }
        match roll.affix_type {
            AffixType::Prefix => {
                if prefix_full {
                    return false;
                }
                claimed.insert(group.clone());
                prefixes.push(roll);
                true
            }
            AffixType::Suffix => {
                if suffix_full {
                    return false;
                }
                claimed.insert(group.clone());
                suffixes.push(roll);
                true
            }
            _ => false,
        }
    };

    // Pinned (fractured) mods always come first.
    for roll in pinned {
        try_push(
            roll,
            &mut claimed_groups,
            &mut chosen_prefixes,
            &mut chosen_suffixes,
        );
    }

    // Sample the rest from the shuffled pool.
    let mut count = chosen_prefixes.len() + chosen_suffixes.len();
    for roll in pool {
        if count >= total_target as usize {
            break;
        }
        if try_push(
            roll,
            &mut claimed_groups,
            &mut chosen_prefixes,
            &mut chosen_suffixes,
        ) {
            count += 1;
        }
    }

    // Even with no surviving mods, the output is still a valid Rare base.
    Ok(Item {
        base: a.base.clone(),
        ilvl: a.ilvl,
        rarity: Rarity::Rare,
        corrupted: false,
        sanctified: false,
        mirrored: false,
        quality: 0,
        quality_kind: crate::item::QualityKind::Untagged,
        implicits: a.implicits.clone(),
        prefixes: chosen_prefixes,
        suffixes: chosen_suffixes,
        enchantments: SmallVec::new(),
        hidden_desecrated: None,
        sockets: SmallVec::new(),
        hinekora_lock: None,
    })
}

/// Recombine with the M14.4 wiki success-chance formula.
///
/// Validates inputs, samples a candidate output mod set just like
/// [`recombine`], but then evaluates the wiki success-chance formula to
/// decide whether the produced output materializes ([`RecombinatorOutcome::Success`])
/// or the attempt fails ([`RecombinatorOutcome::Failure`]) — both inputs
/// are still consumed in either case.
///
/// Per-mod tier ratios `Σ_{j=m}^{m_t0} w_j / Z` are computed using
/// [`ModRegistry::weight_for`] over the source mod's tier-ladder peers.
/// When the registry has no weight observations for a mod, the term
/// degrades to `1.0 / k` where `k` is the number of mods of that affix
/// type on the base — matching the eligibility-fallback behaviour of
/// `weight_for`.
pub fn recombine_with_chance(
    a: &Item,
    b: &Item,
    registry: &ModRegistry,
    rng: &mut dyn rand::RngCore,
) -> EngineResult<RecombinatorOutcome> {
    let candidate = recombine(a, b, registry, rng)?;
    let class = a.base.as_str();
    let mod_count = candidate.prefixes.len() + candidate.suffixes.len();

    // Compute per-mod tier ratios. For each chosen output mod, sum the
    // weights of its tier-ladder peers (registry's mod-group ladder)
    // up to the mods whose `required_level <= ilvl`, then divide by the
    // total weight of mods of the same affix type the base admits.
    let mut ratios: SmallVec<[f64; 6]> = SmallVec::new();
    for roll in candidate.prefixes.iter().chain(candidate.suffixes.iter()) {
        let group = registry.group_of(&roll.mod_id);
        let Some(group) = group else {
            // Unknown mod (out-of-bundle). Use a neutral 1.0 ratio so
            // the formula doesn't penalize unrecognized mods.
            ratios.push(1.0);
            continue;
        };
        // Numerator: sum of weights of tier-ladder peers at ilvl ≤ candidate.ilvl
        // (the chosen mod's tier and lower-tier siblings — i.e., the cumulative
        // weight from m_t0 down to m_i, equivalent to the wiki's Σ_{j=m_i}^{m_t0}).
        let mut numerator = 0.0;
        for &peer_idx in registry.group_members(group) {
            let Some(peer) = registry.at(peer_idx) else {
                continue;
            };
            if peer.required_level > candidate.ilvl {
                continue;
            }
            if peer.affix_type != roll.affix_type {
                continue;
            }
            numerator += registry.weight_for(
                &peer.id,
                &candidate.base,
                candidate.ilvl,
                &crate::ids::ItemClassId::from(class),
            );
        }
        // Denominator: total weight of mods of this affix type on the base.
        let z = sum_affix_weights(registry, &candidate, roll.affix_type, class);
        let ratio = if z > 0.0 { numerator / z } else { 1.0 };
        ratios.push(ratio);
    }

    let p_success = compute_recombine_success_chance(class, mod_count, &ratios);
    let r: f64 = rng.gen();
    if r < p_success {
        Ok(RecombinatorOutcome::Success(Box::new(candidate)))
    } else {
        Ok(RecombinatorOutcome::Failure)
    }
}

/// Sum of `weight_for` across every mod of the given affix type that the
/// item's base/class admits. The denominator `Z` of the wiki formula.
fn sum_affix_weights(registry: &ModRegistry, item: &Item, affix: AffixType, class: &str) -> f64 {
    let class_id = crate::ids::ItemClassId::from(class);
    registry
        .for_class_affix(&class_id, affix)
        .iter()
        .filter_map(|&idx| registry.at(idx))
        .filter(|m| m.required_level <= item.ilvl)
        .map(|m| registry.weight_for(&m.id, &item.base, item.ilvl, &class_id))
        .sum()
}

/// Reject recombines that don't satisfy the precondition list.
fn validate_inputs(a: &Item, b: &Item) -> EngineResult<()> {
    for (label, item) in [("a", a), ("b", b)] {
        if item.rarity != Rarity::Rare {
            return Err(EngineError::InvalidApplication(format!(
                "Recombinator input {label} must be Rare; got {:?}",
                item.rarity
            )));
        }
        if item.corrupted {
            return Err(EngineError::ItemCorrupted);
        }
        if item.sanctified {
            return Err(EngineError::ItemSanctified);
        }
        if item.mirrored {
            return Err(EngineError::InvalidApplication(format!(
                "Recombinator input {label} is mirrored"
            )));
        }
    }
    if a.base != b.base {
        return Err(EngineError::InvalidApplication(format!(
            "Recombinator inputs must share a base; got {} vs {}",
            a.base, b.base
        )));
    }
    Ok(())
}

/// Draw the target mod-count from `[1, 6]` with mild peak at 4.
///
/// Approximate triangular distribution: roll 2d4-1, clamp to [1, 6].
fn sample_recombine_count(rng: &mut dyn rand::RngCore) -> u8 {
    let r1: u8 = rng.gen_range(1..=4);
    let r2: u8 = rng.gen_range(0..=3);
    (r1 + r2).clamp(1, 6)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{ItemClassId, ModGroupId, ModId, TagId};
    use crate::item::{AffixType, ModRoll, QualityKind};
    use crate::mods::{ModDefinition, ModDomain, ModFlags, ModGroup, ModKind, SpawnWeight};
    use crate::patch::PatchRange;
    use rand::SeedableRng;
    use rand_xoshiro::Xoshiro256PlusPlus;
    use smallvec::smallvec;

    fn mk_mod(id: &str, group: &str, affix: AffixType, class: &str) -> ModDefinition {
        ModDefinition {
            id: ModId::from(id),
            name: None,
            mod_group: ModGroup(ModGroupId::from(group)),
            affix_type: affix,
            kind: ModKind::Explicit,
            domain: ModDomain::Item,
            tags: smallvec![],
            concept_set: smallvec![],
            spawn_weights: smallvec![SpawnWeight {
                tag: TagId::from(class),
                weight: 1
            }],
            stats: smallvec![],
            required_level: 1,
            allowed_item_classes: smallvec![ItemClassId::from(class)],
            patch_range: PatchRange::ALL,
            flags: ModFlags::empty(),
            text_template: None,
        }
    }

    fn rare_with(prefixes: &[(&str, &str)], suffixes: &[(&str, &str)]) -> Item {
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
            prefixes: prefixes
                .iter()
                .map(|(id, _)| ModRoll {
                    mod_id: ModId::from(*id),
                    affix_type: AffixType::Prefix,
                    kind: ModKind::Explicit,
                    values: smallvec![],
                    is_fractured: false,
                })
                .collect(),
            suffixes: suffixes
                .iter()
                .map(|(id, _)| ModRoll {
                    mod_id: ModId::from(*id),
                    affix_type: AffixType::Suffix,
                    kind: ModKind::Explicit,
                    values: smallvec![],
                    is_fractured: false,
                })
                .collect(),
            enchantments: smallvec![],
            hidden_desecrated: None,
            sockets: smallvec![],
            hinekora_lock: None,
        }
    }

    fn small_registry() -> ModRegistry {
        ModRegistry::from_mods(
            vec![
                mk_mod("APrefix1", "G_AP1", AffixType::Prefix, "BodyArmour"),
                mk_mod("APrefix2", "G_AP2", AffixType::Prefix, "BodyArmour"),
                mk_mod("BPrefix1", "G_BP1", AffixType::Prefix, "BodyArmour"),
                mk_mod("ASuffix1", "G_AS1", AffixType::Suffix, "BodyArmour"),
                mk_mod("BSuffix1", "G_BS1", AffixType::Suffix, "BodyArmour"),
                mk_mod("BSuffix2", "G_BS2", AffixType::Suffix, "BodyArmour"),
                // Two mods sharing a group, to test exclusivity.
                mk_mod("CDup1", "G_DUP", AffixType::Prefix, "BodyArmour"),
                mk_mod("CDup2", "G_DUP", AffixType::Prefix, "BodyArmour"),
            ],
            vec![],
        )
    }

    #[test]
    fn recombine_outputs_rare() {
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0xfeed);
        let reg = small_registry();
        let a = rare_with(
            &[("APrefix1", "p"), ("APrefix2", "p")],
            &[("ASuffix1", "s")],
        );
        let b = rare_with(
            &[("BPrefix1", "p")],
            &[("BSuffix1", "s"), ("BSuffix2", "s")],
        );
        let out = recombine(&a, &b, &reg, &mut rng).unwrap();
        assert_eq!(out.rarity, Rarity::Rare);
        assert!(!out.corrupted);
        assert!(!out.sanctified);
        assert_eq!(out.quality, 0);
    }

    #[test]
    fn recombine_caps_at_3_per_affix() {
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0xc0de);
        let reg = small_registry();
        let a = rare_with(
            &[("APrefix1", "p"), ("APrefix2", "p"), ("BPrefix1", "p")],
            &[("ASuffix1", "s"), ("BSuffix1", "s"), ("BSuffix2", "s")],
        );
        let b = rare_with(
            &[("APrefix1", "p"), ("APrefix2", "p")],
            &[("ASuffix1", "s")],
        );
        let out = recombine(&a, &b, &reg, &mut rng).unwrap();
        assert!(out.prefixes.len() <= 3);
        assert!(out.suffixes.len() <= 3);
    }

    #[test]
    fn recombine_honors_mod_group_exclusivity() {
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0xbabe);
        let reg = small_registry();
        let a = rare_with(&[("CDup1", "p")], &[]);
        let b = rare_with(&[("CDup2", "p")], &[]);
        let out = recombine(&a, &b, &reg, &mut rng).unwrap();
        // At most one of the duplicate-group mods can survive.
        let dup_count = out
            .prefixes
            .iter()
            .filter(|m| m.mod_id.as_str() == "CDup1" || m.mod_id.as_str() == "CDup2")
            .count();
        assert!(dup_count <= 1);
    }

    #[test]
    fn recombine_preserves_fractured_mods() {
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0xfeed);
        let reg = small_registry();
        let mut a = rare_with(&[("APrefix1", "p")], &[]);
        a.prefixes[0].is_fractured = true;
        let b = rare_with(&[("BPrefix1", "p")], &[("BSuffix1", "s")]);
        let out = recombine(&a, &b, &reg, &mut rng).unwrap();
        assert!(out
            .prefixes
            .iter()
            .any(|m| m.mod_id.as_str() == "APrefix1" && m.is_fractured));
    }

    #[test]
    fn recombine_rejects_corrupted_input() {
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0);
        let reg = small_registry();
        let mut a = rare_with(&[("APrefix1", "p")], &[]);
        let b = rare_with(&[("BPrefix1", "p")], &[]);
        a.corrupted = true;
        let r = recombine(&a, &b, &reg, &mut rng);
        assert!(matches!(r, Err(EngineError::ItemCorrupted)));
    }

    #[test]
    fn recombine_rejects_sanctified_input() {
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0);
        let reg = small_registry();
        let mut a = rare_with(&[("APrefix1", "p")], &[]);
        let b = rare_with(&[("BPrefix1", "p")], &[]);
        a.sanctified = true;
        let r = recombine(&a, &b, &reg, &mut rng);
        assert!(matches!(r, Err(EngineError::ItemSanctified)));
    }

    #[test]
    fn recombine_rejects_non_rare_inputs() {
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0);
        let reg = small_registry();
        let mut a = rare_with(&[("APrefix1", "p")], &[]);
        let b = rare_with(&[("BPrefix1", "p")], &[]);
        a.rarity = Rarity::Magic;
        let r = recombine(&a, &b, &reg, &mut rng);
        assert!(matches!(r, Err(EngineError::InvalidApplication(_))));
    }

    #[test]
    fn recombine_rejects_mismatched_bases() {
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0);
        let reg = small_registry();
        let a = rare_with(&[("APrefix1", "p")], &[]);
        let mut b = rare_with(&[("BPrefix1", "p")], &[]);
        b.base = ItemClassId::from("Boots").as_str().into();
        let r = recombine(&a, &b, &reg, &mut rng);
        assert!(matches!(r, Err(EngineError::InvalidApplication(_))));
    }

    #[test]
    fn recombine_is_deterministic_for_same_seed() {
        let reg = small_registry();
        let a = rare_with(
            &[("APrefix1", "p"), ("APrefix2", "p")],
            &[("ASuffix1", "s")],
        );
        let b = rare_with(
            &[("BPrefix1", "p")],
            &[("BSuffix1", "s"), ("BSuffix2", "s")],
        );
        let mut rng_a = Xoshiro256PlusPlus::seed_from_u64(7);
        let mut rng_b = Xoshiro256PlusPlus::seed_from_u64(7);
        let out_a = recombine(&a, &b, &reg, &mut rng_a).unwrap();
        let out_b = recombine(&a, &b, &reg, &mut rng_b).unwrap();
        assert_eq!(out_a, out_b);
    }
}
