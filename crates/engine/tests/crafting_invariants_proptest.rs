//! P1 — property-based invariants for weighting + sampling.
//!
//! These hold for *any* mod ladder and item level. Run via `proptest` over
//! randomized tier ladders and ilvls.
//!
//! Invariants:
//! - **Monotonic inclusive weight.** For a fixed mod, its inclusive weight is
//!   non-decreasing in ilvl (raising ilvl only ever unlocks more higher
//!   tiers; it never removes peers).
//! - **Inclusive ≤ group total.** A tier's inclusive weight never exceeds the
//!   sum of all rollable tiers of its group at that ilvl.
//! - **apply never exceeds ilvl / never dups a group.** Any mod a basic orb
//!   adds has `required_level <= ilvl` and does not duplicate an existing
//!   mod-group on the item.

use poc2_engine::currency::basic::{ExaltedOrb, OrbOfTransmutation};
use poc2_engine::ids::TagId;
use poc2_engine::omen::OmenSet;
use poc2_engine::patch::PatchVersion;
use poc2_engine::weights::{Confidence, WeightObservation, WeightScope};
use poc2_engine::{
    apply_currency, AffixType, BaseTypeId, Item, ItemClassId, ModDefinition, ModDomain, ModFlags,
    ModGroup, ModGroupId, ModId, ModKind, ModRegistry, QualityKind, Rarity, SpawnWeight,
};
use proptest::prelude::*;
use rand_xoshiro::rand_core::SeedableRng;
use rand_xoshiro::Xoshiro256PlusPlus;
use smallvec::smallvec;

const PATCH: PatchVersion = PatchVersion::PATCH_0_4_0;
const CLASS: &str = "BodyArmour";
const BASE: &str = "BodyArmour";

fn mk_mod(id: &str, group: &str, affix: AffixType, required_level: u32) -> ModDefinition {
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
            tag: TagId::from("any"),
            weight: 1
        }],
        stats: smallvec![],
        required_level,
        tier: None,
        allowed_item_classes: smallvec![ItemClassId::from(CLASS)],
        patch_range: poc2_engine::PatchRange::ALL,
        flags: ModFlags::empty(),
        text_template: None,
    }
}

fn obs(mod_id: &str, weight: f64) -> WeightObservation {
    WeightObservation {
        mod_id: ModId::from(mod_id),
        scope: WeightScope::Base {
            base: BaseTypeId::from(BASE),
        },
        primary_weight: weight,
        secondary_weight: None,
        confidence: Confidence::Community,
        note: None,
    }
}

/// Build a registry from a list of (required_level, weight) prefix tiers in
/// one mod-group "G", plus a single suffix filler group so suffix slots can
/// be filled.
fn ladder_registry(tiers: &[(u32, f64)]) -> ModRegistry {
    let mut mods = Vec::new();
    let mut weights = Vec::new();
    for (i, (rl, w)) in tiers.iter().enumerate() {
        let id = format!("G_T{i}");
        mods.push(mk_mod(&id, "G", AffixType::Prefix, *rl));
        weights.push(obs(&id, *w));
    }
    mods.push(mk_mod("SufFiller", "SufG", AffixType::Suffix, 1));
    ModRegistry::from_mods(mods, weights)
}

proptest! {
    /// Inclusive weight of the bottom tier is non-decreasing in ilvl.
    #[test]
    fn inclusive_weight_monotonic_in_ilvl(
        tiers in prop::collection::vec((1u32..=90u32, 1.0f64..2000.0), 1..6),
        ilvls in prop::collection::vec(1u32..=100u32, 2..8),
    ) {
        let r = ladder_registry(&tiers);
        let bottom = r.get(&ModId::from("G_T0")).unwrap();
        let mut sorted_ilvls = ilvls.clone();
        sorted_ilvls.sort_unstable();
        let mut prev = 0.0;
        for ilvl in sorted_ilvls {
            let w = r.inclusive_weight_for(
                bottom,
                &BaseTypeId::from(BASE),
                ilvl,
                &ItemClassId::from(CLASS),
            );
            prop_assert!(
                w + 1e-6 >= prev,
                "inclusive weight decreased: ilvl {ilvl} → {w}, prev {prev}"
            );
            prev = w;
        }
    }

    /// A tier's inclusive weight never exceeds the sum of all rollable tiers
    /// of its group at that ilvl.
    #[test]
    fn inclusive_weight_bounded_by_group_total(
        tiers in prop::collection::vec((1u32..=90u32, 1.0f64..2000.0), 1..6),
        ilvl in 1u32..=100u32,
    ) {
        let r = ladder_registry(&tiers);
        // Group total = sum of per-tier weights for tiers rollable at ilvl.
        let mut group_total = 0.0;
        for (i, (rl, w)) in tiers.iter().enumerate() {
            if *rl <= ilvl {
                let _ = i;
                group_total += *w;
            }
        }
        for i in 0..tiers.len() {
            let m = r.get(&ModId::from(format!("G_T{i}").as_str())).unwrap();
            let incl = r.inclusive_weight_for(
                m,
                &BaseTypeId::from(BASE),
                ilvl,
                &ItemClassId::from(CLASS),
            );
            prop_assert!(
                incl <= group_total + 1e-6,
                "tier {i} inclusive weight {incl} exceeds group total {group_total} at ilvl {ilvl}"
            );
        }
    }
}

fn fresh_normal_item(ilvl: u32) -> Item {
    Item {
        base: BaseTypeId::from(BASE),
        ilvl,
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

proptest! {
    /// A Transmute (Normal→Magic add) never adds a mod above the item's ilvl,
    /// across randomized ladders and ilvls. (Group-dup is trivially satisfied
    /// on a fresh Normal item, but the assertion is kept for documentation.)
    #[test]
    fn transmute_never_exceeds_ilvl(
        tiers in prop::collection::vec((1u32..=100u32, 1.0f64..2000.0), 1..6),
        ilvl in 1u32..=100u32,
        seed in any::<u64>(),
    ) {
        let r = ladder_registry(&tiers);
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(seed);
        let mut omens = OmenSet::new();
        let mut item = fresh_normal_item(ilvl);
        // Transmute may legitimately fail when no tier is rollable at ilvl.
        if apply_currency(&OrbOfTransmutation::new(), &mut item, &r, &mut rng, PATCH, &mut omens).is_ok() {
            for roll in item.prefixes.iter().chain(item.suffixes.iter()) {
                if let Some(def) = r.get(&roll.mod_id) {
                    prop_assert!(
                        def.required_level <= ilvl,
                        "added mod {} requires level {} > ilvl {}",
                        roll.mod_id.as_str(), def.required_level, ilvl
                    );
                }
            }
        }
    }

    /// Exalt never produces two mods of the same group on a Rare item that
    /// already carries the group.
    #[test]
    fn exalt_never_duplicates_group(
        tiers in prop::collection::vec((1u32..=80u32, 1.0f64..2000.0), 2..6),
        seed in any::<u64>(),
    ) {
        let r = ladder_registry(&tiers);
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(seed);
        let mut omens = OmenSet::new();
        // Rare item already carrying group "G" in a prefix slot.
        let mut item = fresh_normal_item(100);
        item.rarity = Rarity::Rare;
        item.prefixes.push(poc2_engine::ModRoll {
            mod_id: ModId::from("G_T0"),
            affix_type: AffixType::Prefix,
            kind: ModKind::Explicit,
            values: smallvec![],
            is_fractured: false,
        });
        let before = item.prefixes.len() + item.suffixes.len();
        if apply_currency(&ExaltedOrb::new(), &mut item, &r, &mut rng, PATCH, &mut omens).is_ok() {
            // Count occurrences of group "G".
            let g_count = item.prefixes.iter().chain(item.suffixes.iter())
                .filter(|roll| r.group_of(&roll.mod_id) == Some(&ModGroupId::from("G")))
                .count();
            prop_assert!(g_count <= 1, "duplicated group G: {g_count} occurrences");
        }
        let _ = before;
    }
}
