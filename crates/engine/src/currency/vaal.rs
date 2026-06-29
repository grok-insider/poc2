//! Vaal Orb — corruption outcomes and the corrupted-mod samplers.

use rand::Rng;
use smallvec::SmallVec;

use crate::currency::basic::{multiply_explicit_values, reroll_explicit_values};
use crate::currency::common::{
    class_for_item, collect_occupied_groups, pick_weighted, push_mod, roll_mod,
};
use crate::currency::{ApplyContext, ApplyOutcome, Currency};
use crate::error::{EngineError, EngineResult};
use crate::ids::CurrencyId;
use crate::item::{AffixType, Item, ModRoll};
use crate::mods::{ModDefinition, ModFlags, ModKind};
use crate::registry::{ModIndex, ModRegistry};

/// What kind of corruption outcome did Vaal produce?
///
/// Reported by the engine for diagnostics / advisor explanation. Mirrors
/// the outcomes documented in `/docs/11-game-mechanics.md` (lands in M2.10):
///
/// - `NoChange` — item is corrupted but otherwise unchanged. Removable by
///   Omen of Corruption (M2.6).
/// - `RerollValues` — divine-like reroll across explicit mods.
/// - `BrickMods` — strips all explicit mods and replaces them with a
///   simulated brick (here: just rerolls one prefix to a "useless" state;
///   real brick semantics land when desecrated mod data is integrated).
/// - `AddEnchantment` — adds a corrupted enchantment (placeholder for now).
/// - `AddSocket` — adds a socket beyond the normal cap.
/// - `AddQuality` — bumps quality past the cap (caps at +30).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VaalOutcome {
    NoChange,
    RerollValues,
    BrickMods,
    AddEnchantment,
    AddSocket,
    AddQuality,
}

/// Vaal Orb: corrupt the item with one of several random outcomes.
///
/// Once corrupted, the item is locked from most further crafting (only
/// Architect's Orb double-corrupt, Vaal Cultivation Orb on uniques, and a
/// handful of omens still apply).
///
/// The outcome distribution is approximated for M2.4 — refined in M2.5
/// when omen-conditioning lands. For now we use uniform 1/6 across the
/// six outcomes; Omen of Corruption (M2.6) will remove `NoChange`.
#[derive(Debug)]
pub struct VaalOrb {
    id: CurrencyId,
}

impl VaalOrb {
    pub fn new() -> Self {
        Self {
            id: CurrencyId::from("VaalOrb"),
        }
    }
}

impl Default for VaalOrb {
    fn default() -> Self {
        Self::new()
    }
}

impl Currency for VaalOrb {
    fn id(&self) -> &CurrencyId {
        &self.id
    }
    fn name(&self) -> &'static str {
        "Vaal Orb"
    }
    fn valid_rarities(&self) -> crate::currency::RaritySet {
        crate::currency::RaritySet::all()
    }

    fn apply(&self, item: &mut Item, ctx: &mut ApplyContext<'_>) -> EngineResult<ApplyOutcome> {
        if item.corrupted {
            return Err(EngineError::ItemCorrupted);
        }
        if item.sanctified {
            return Err(EngineError::ItemSanctified);
        }
        if item.mirrored {
            return Err(EngineError::InvalidApplication(
                "Vaal Orb cannot be applied to a mirrored item".into(),
            ));
        }

        // M14.4: Omen of Corruption suppresses the NoChange outcome.
        // `consume_prevent_no_change` removes the omen iff present and in
        // patch range; the boolean shifts the sampler into a 5-outcome mode.
        let prevent_no_change = ctx.omens.consume_prevent_no_change(ctx.patch, ctx.league);
        let outcome = sample_vaal_outcome(ctx.rng, prevent_no_change);
        item.corrupted = true;
        match outcome {
            VaalOutcome::NoChange => {}
            VaalOutcome::RerollValues => {
                // ≤0.4: the corruption outcome randomises mod values
                // (divine-like reroll). 0.5+: per the patch notes it now
                // "multiplies each modifier based on its current value"
                // instead — modelled as a per-mod uniform factor in
                // [0.8, 1.25] (Experimental).
                if ctx.patch >= crate::patch::PatchVersion::PATCH_0_5_0 {
                    multiply_explicit_values(item, ctx, 0.8, 1.25);
                } else {
                    reroll_explicit_values(item, ctx);
                }
            }
            VaalOutcome::BrickMods => {
                // Clear non-fractured explicit mods. For each cleared slot,
                // attempt to sample a ModKind::Corrupted mod from the
                // registry (filtered by item class and affix type). When
                // no Corrupted-explicit pool exists for the class — which
                // is the v3 starting state, since `vaal_implicits.json`
                // populates only Implicit-affix Corrupted mods — the slot
                // stays empty and the item ends up with fewer explicit
                // mods, which approximates the in-game brick.
                let kept_prefixes = item
                    .prefixes
                    .iter()
                    .filter(|m| m.is_fractured)
                    .cloned()
                    .collect::<smallvec::SmallVec<_>>();
                let kept_suffixes = item
                    .suffixes
                    .iter()
                    .filter(|m| m.is_fractured)
                    .cloned()
                    .collect::<smallvec::SmallVec<_>>();
                let prefixes_to_replace = item.prefixes.len() - kept_prefixes.len();
                let suffixes_to_replace = item.suffixes.len() - kept_suffixes.len();
                item.prefixes = kept_prefixes;
                item.suffixes = kept_suffixes;
                for _ in 0..prefixes_to_replace {
                    if let Some(m) = sample_corrupted_explicit(
                        ctx.registry,
                        ctx.base_registry,
                        item,
                        AffixType::Prefix,
                        ctx.rng,
                        ctx.patch,
                    ) {
                        push_mod(item, roll_mod(m, ctx.rng));
                    }
                }
                for _ in 0..suffixes_to_replace {
                    if let Some(m) = sample_corrupted_explicit(
                        ctx.registry,
                        ctx.base_registry,
                        item,
                        AffixType::Suffix,
                        ctx.rng,
                        ctx.patch,
                    ) {
                        push_mod(item, roll_mod(m, ctx.rng));
                    }
                }
            }
            VaalOutcome::AddEnchantment => {
                // Roll one Vaal implicit (Corrupted-kind, Implicit-affix)
                // from the registry filtered by item class. The outcome is
                // a no-op when the bundle has no Vaal-implicit data for
                // the class (older fixtures).
                if let Some(m) = sample_corrupted_implicit(
                    ctx.registry,
                    ctx.base_registry,
                    item,
                    ctx.rng,
                    ctx.patch,
                ) {
                    let values = m
                        .stats
                        .iter()
                        .map(|s| s.roll(ctx.rng.gen::<f64>()))
                        .collect();
                    item.enchantments.push(ModRoll {
                        mod_id: m.id.clone(),
                        affix_type: AffixType::Implicit,
                        kind: ModKind::Corrupted,
                        values,
                        is_fractured: false,
                    });
                }
            }
            VaalOutcome::AddSocket => {
                // Vaal can add a socket beyond the cap; we just push an empty one.
                item.sockets.push(crate::item::Socket { augment: None });
            }
            VaalOutcome::AddQuality => {
                item.quality = item.quality.saturating_add(5).min(30);
            }
        }
        Ok(())
    }
}

/// Vaal outcome categorical distribution (M14.4).
///
/// Authoritative source: poe2wiki Vaal Orb outcomes, cross-checked with
/// `docs/81-engine-training-and-rule-encoding-plan.md` §4.4 Tier 1.4.
///
/// | Outcome        | Base | Omen of Corruption |
/// |----------------|------|--------------------|
/// | NoChange       | 0.25 | 0.0                |
/// | RerollValues   | 0.20 | 0.267              |
/// | BrickMods      | 0.15 | 0.20               |
/// | AddEnchantment | 0.20 | 0.267              |
/// | AddSocket      | 0.10 | 0.133              |
/// | AddQuality     | 0.10 | 0.133              |
///
/// The omen variant simply removes the NoChange branch and renormalizes
/// the remaining five over their original 0.75 mass.
fn sample_vaal_outcome(rng: &mut dyn rand::RngCore, prevent_no_change: bool) -> VaalOutcome {
    let r: f64 = rng.gen();
    if prevent_no_change {
        // Cumulative thresholds over [RerollValues, BrickMods,
        // AddEnchantment, AddSocket, AddQuality] with mass 0.20 / 0.15 /
        // 0.20 / 0.10 / 0.10 normalized by 0.75:
        //   0.2667, 0.4667, 0.7333, 0.8667, 1.0
        if r < 0.266_666_67 {
            VaalOutcome::RerollValues
        } else if r < 0.466_666_67 {
            VaalOutcome::BrickMods
        } else if r < 0.733_333_33 {
            VaalOutcome::AddEnchantment
        } else if r < 0.866_666_67 {
            VaalOutcome::AddSocket
        } else {
            VaalOutcome::AddQuality
        }
    } else {
        // Cumulative thresholds: 0.25 / 0.45 / 0.60 / 0.80 / 0.90 / 1.00.
        if r < 0.25 {
            VaalOutcome::NoChange
        } else if r < 0.45 {
            VaalOutcome::RerollValues
        } else if r < 0.60 {
            VaalOutcome::BrickMods
        } else if r < 0.80 {
            VaalOutcome::AddEnchantment
        } else if r < 0.90 {
            VaalOutcome::AddSocket
        } else {
            VaalOutcome::AddQuality
        }
    }
}

/// Sample a Corrupted-kind explicit mod from the registry filtered by
/// item class and affix type. Used by Vaal's [`VaalOutcome::BrickMods`]
/// path to replace cleared explicit slots. Returns `None` when no
/// matching Corrupted-explicit mods exist for the class (the typical
/// case in v3 starter bundles).
fn sample_corrupted_explicit<'r>(
    registry: &'r ModRegistry,
    base_registry: &crate::base_registry::BaseRegistry,
    item: &Item,
    affix: AffixType,
    rng: &mut dyn rand::RngCore,
    patch: crate::patch::PatchVersion,
) -> Option<&'r ModDefinition> {
    let class = class_for_item(item, base_registry);
    let candidates = registry.for_class_affix(&class, affix);
    let occupied = collect_occupied_groups(registry, item);
    let mut eligible: SmallVec<[(ModIndex, f64); 16]> = SmallVec::new();
    for &idx in candidates {
        let Some(m) = registry.at(idx) else { continue };
        if m.kind != ModKind::Corrupted {
            continue;
        }
        if !m.patch_range.contains(patch) {
            continue;
        }
        if occupied.contains(&m.mod_group.0) {
            continue;
        }
        if m.required_level > item.ilvl {
            continue;
        }
        // Vaal corruption can roll essence-only-flagged Corrupted mods
        // (those don't exist) but never Desecrated-only ones.
        if m.flags
            .intersects(ModFlags::ESSENCE_ONLY | ModFlags::DESECRATED_ONLY)
        {
            continue;
        }
        // Use the registry's weight resolver; falls back to eligibility-flag
        // if no per-base/per-class observation exists.
        let w = registry.weight_for(&m.id, &item.base, item.ilvl, &class);
        if w <= 0.0 {
            continue;
        }
        eligible.push((idx, w));
    }
    pick_weighted(registry, &eligible, rng)
}

/// Sample a Vaal implicit (Corrupted-kind, Implicit-affix) from the
/// registry filtered by item class. Used by Vaal's
/// [`VaalOutcome::AddEnchantment`] path. Returns `None` when no Vaal
/// implicits exist for the class.
fn sample_corrupted_implicit<'r>(
    registry: &'r ModRegistry,
    base_registry: &crate::base_registry::BaseRegistry,
    item: &Item,
    rng: &mut dyn rand::RngCore,
    patch: crate::patch::PatchVersion,
) -> Option<&'r ModDefinition> {
    let class = class_for_item(item, base_registry);
    // The `by_class_affix` index keys on `m.affix_type`, including
    // `AffixType::Implicit`, so Vaal implicits land in the per-class
    // implicit slot of the index naturally.
    // The genuine 0.5 corrupted pools ship with `affix = Enchantment`
    // (poe2db per-class "Corrupted" sections; they land in the enchant
    // slot). Legacy fixtures used `Implicit`. Draw from both indices so
    // the sampler sees the real RePoE pool (M14 audit).
    let candidates = registry
        .for_class_affix(&class, AffixType::Implicit)
        .iter()
        .chain(
            registry
                .for_class_affix(&class, AffixType::Enchantment)
                .iter(),
        )
        .copied()
        .collect::<SmallVec<[ModIndex; 32]>>();
    let mut eligible: SmallVec<[(ModIndex, f64); 16]> = SmallVec::new();
    for idx in candidates {
        let Some(m) = registry.at(idx) else { continue };
        if m.kind != ModKind::Corrupted {
            continue;
        }
        if !m.patch_range.contains(patch) {
            continue;
        }
        if m.required_level > item.ilvl {
            continue;
        }
        // Don't double-add an existing Vaal implicit. Also respect
        // mod-group exclusivity against the existing enchantment slot.
        if item.enchantments.iter().any(|e| e.mod_id == m.id) {
            continue;
        }
        if item.enchantments.iter().any(|e| {
            registry
                .group_of(&e.mod_id)
                .is_some_and(|g| g == &m.mod_group.0)
        }) {
            continue;
        }
        // Uniform weighting for Vaal-implicit selection — the wiki
        // does not document a per-implicit weight table for PoE2.
        eligible.push((idx, 1.0));
    }
    pick_weighted(registry, &eligible, rng)
}

#[cfg(test)]
mod tests {
    use rand::SeedableRng;
    use rand_xoshiro::Xoshiro256PlusPlus;

    use super::*;
    use crate::currency::common::test_fixtures::{ctx, fixture_normal_boots, fixture_registry};
    use crate::item::Rarity;

    #[test]
    fn vaal_marks_item_corrupted() {
        let reg = fixture_registry();
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0x1);
        let mut omens = crate::omen::OmenSet::new();
        let mut item = fixture_normal_boots();
        item.rarity = Rarity::Rare;
        VaalOrb::new()
            .apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens))
            .unwrap();
        assert!(item.corrupted);
    }

    #[test]
    fn vaal_rejects_already_corrupted() {
        let reg = fixture_registry();
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0x2);
        let mut omens = crate::omen::OmenSet::new();
        let mut item = fixture_normal_boots();
        item.corrupted = true;
        let r = VaalOrb::new().apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens));
        assert!(matches!(r, Err(EngineError::ItemCorrupted)));
    }

    #[test]
    fn vaal_rejects_sanctified_or_mirrored() {
        let reg = fixture_registry();
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0x3);
        let mut omens = crate::omen::OmenSet::new();
        let mut item = fixture_normal_boots();
        item.sanctified = true;
        let r = VaalOrb::new().apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens));
        assert!(matches!(r, Err(EngineError::ItemSanctified)));

        let mut item = fixture_normal_boots();
        item.mirrored = true;
        let r = VaalOrb::new().apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens));
        assert!(matches!(r, Err(EngineError::InvalidApplication(_))));
    }

    #[test]
    fn vaal_outcome_distribution_covers_all_six_branches() {
        // Statistical: across 600 trials, every outcome variant should appear.
        use std::collections::HashSet;
        let mut seen: HashSet<u8> = HashSet::new();
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0x600);
        let mut _omens = crate::omen::OmenSet::new();
        for _ in 0..600 {
            let outcome = sample_vaal_outcome(&mut rng, false);
            seen.insert(outcome as u8);
        }
        assert_eq!(seen.len(), 6, "saw {} distinct outcomes", seen.len());
    }

    /// The genuine 0.5 corrupted pools carry `affix = Enchantment` (poe2db
    /// per-class "Corrupted" sections); the sampler must draw from that
    /// index, not just the legacy Implicit one (M14 audit regression).
    #[test]
    fn corrupted_sampler_draws_from_enchantment_affix_pool() {
        use crate::ids::{ItemClassId, ModGroupId, ModId, StatId, TagId};
        use crate::mods::{
            ModDefinition, ModDomain, ModFlags, ModGroup, ModKind, ModStat, SpawnWeight,
        };
        use crate::patch::PatchRange;
        use smallvec::smallvec;

        let corrupted_enchant = ModDefinition {
            id: ModId::from("CorruptionIncreasedLife1"),
            name: None,
            mod_group: ModGroup(ModGroupId::from("CorruptionLife")),
            affix_type: AffixType::Enchantment,
            kind: ModKind::Corrupted,
            domain: ModDomain::Item,
            tags: smallvec![],
            concept_set: smallvec![],
            spawn_weights: smallvec![SpawnWeight {
                tag: TagId::from("default"),
                weight: 1
            }],
            stats: smallvec![ModStat {
                stat_id: StatId::from("base_maximum_life"),
                min: 30.0,
                max: 40.0,
            }],
            required_level: 1,
            tier: None,
            allowed_item_classes: smallvec![ItemClassId::from("Boots")],
            patch_range: PatchRange::ALL,
            flags: ModFlags::CORRUPTED_ONLY,
            text_template: None,
        };
        let reg = ModRegistry::from_mods(vec![corrupted_enchant], vec![]);
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0x55);
        let item = {
            let mut it = fixture_normal_boots();
            it.rarity = Rarity::Rare;
            it
        };
        let m = sample_corrupted_implicit(
            &reg,
            &crate::base_registry::EMPTY,
            &item,
            &mut rng,
            crate::patch::PatchVersion::PATCH_0_5_0,
        );
        assert!(
            m.is_some_and(|m| m.id.as_str() == "CorruptionIncreasedLife1"),
            "enchantment-affix corrupted mod must be sampleable"
        );
    }
}
