//! M14.1 — weighted sampling integration test.
//!
//! Validates that [`poc2_engine::ModRegistry::weight_for`] is consulted by
//! [`sample_eligible_mod`] (via `apply()`) so the runtime sampler reflects
//! CoE-derived numerical weights instead of the v2 0/1 eligibility stub.
//!
//! Reference: `docs/81-engine-training-and-rule-encoding-plan.md` §4.1
//! Tier 1.1.
//!
//! Setup:
//! - Two prefix mods on the same item class but distinct mod-groups so they
//!   compete for the same affix slot.
//! - Weight observations: mod A → weight 1000, mod B → weight 100 (10:1).
//! - Magic item with one pre-placed fractured suffix so [`OrbOfAugmentation`]
//!   deterministically targets the open prefix slot, isolating the sampler
//!   from the affix-coin-flip in `pick_open_affix`.
//! - 10 000 augmentation trials with seeded RNGs (one fresh item + RNG seed
//!   per trial so trial outcomes are independent).
//!
//! Expected: roughly 91% / 9% split; tolerance is 3σ around the binomial
//! standard error so the test is unlikely to flake.

use poc2_engine::currency::basic::OrbOfAugmentation;
use poc2_engine::ids::TagId;
use poc2_engine::omen::OmenSet;
use poc2_engine::patch::PatchVersion;
use poc2_engine::weights::{Confidence, WeightObservation, WeightScope};
use poc2_engine::{
    apply_currency, AffixType, BaseTypeId, Item, ItemClassId, ModDefinition, ModDomain, ModFlags,
    ModGroup, ModGroupId, ModId, ModKind, ModRegistry, ModRoll, PatchRange, QualityKind, Rarity,
    SpawnWeight,
};
use rand_xoshiro::rand_core::SeedableRng;
use rand_xoshiro::Xoshiro256PlusPlus;
use smallvec::smallvec;

const PATCH: PatchVersion = PatchVersion::PATCH_0_4_0;
const TRIALS: usize = 10_000;

fn mk_prefix_mod(id: &str, group: &str) -> ModDefinition {
    ModDefinition {
        id: ModId::from(id),
        name: None,
        mod_group: ModGroup(ModGroupId::from(group)),
        affix_type: AffixType::Prefix,
        kind: ModKind::Explicit,
        domain: ModDomain::Item,
        tags: smallvec![],
        concept_set: smallvec![],
        // Eligibility tag-flag — non-zero so the eligibility fallback would
        // also classify the mod as eligible, but the weight tables we feed
        // in below take precedence.
        spawn_weights: smallvec![SpawnWeight {
            tag: TagId::from("any"),
            weight: 1
        }],
        stats: smallvec![],
        required_level: 1,
        allowed_item_classes: smallvec![ItemClassId::from("BodyArmour")],
        patch_range: PatchRange::ALL,
        flags: ModFlags::empty(),
        text_template: None,
    }
}

fn mk_suffix_mod_for_filler(id: &str, group: &str) -> ModDefinition {
    let mut m = mk_prefix_mod(id, group);
    m.affix_type = AffixType::Suffix;
    m
}

fn mk_filled_magic_item() -> Item {
    let mut item = Item {
        base: BaseTypeId::from("BodyArmour"),
        ilvl: 82,
        rarity: Rarity::Magic,
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
    };
    // Pre-place a fractured suffix so `pick_open_affix` deterministically
    // returns Prefix (Magic-rarity max-affix is 1 per slot, and the suffix
    // slot is full).
    item.suffixes.push(ModRoll {
        mod_id: ModId::from("Filler"),
        affix_type: AffixType::Suffix,
        kind: ModKind::Explicit,
        values: smallvec![],
        is_fractured: true,
    });
    item
}

#[test]
fn weighted_sampling_matches_observation_ratio() {
    let mods = vec![
        mk_prefix_mod("ModA", "GroupA"),
        mk_prefix_mod("ModB", "GroupB"),
        // Filler defined so the registry can resolve its mod-group during
        // mod-group exclusivity checks; the filler's weight is irrelevant
        // because Aug never targets the suffix slot in this test.
        mk_suffix_mod_for_filler("Filler", "FillerGroup"),
    ];
    let weights = vec![
        WeightObservation {
            mod_id: ModId::from("ModA"),
            scope: WeightScope::Base {
                base: BaseTypeId::from("BodyArmour"),
            },
            primary_weight: 1000.0,
            secondary_weight: None,
            confidence: Confidence::Community,
            note: None,
        },
        WeightObservation {
            mod_id: ModId::from("ModB"),
            scope: WeightScope::Base {
                base: BaseTypeId::from("BodyArmour"),
            },
            primary_weight: 100.0,
            secondary_weight: None,
            confidence: Confidence::Community,
            note: None,
        },
    ];
    let registry = ModRegistry::from_mods(mods, weights);

    let mut count_a = 0usize;
    let mut count_b = 0usize;
    for trial in 0..TRIALS {
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0xA17E_5EED ^ trial as u64);
        let mut omens = OmenSet::new();
        let mut item = mk_filled_magic_item();
        apply_currency(
            &OrbOfAugmentation::new(),
            &mut item,
            &registry,
            &mut rng,
            PATCH,
            &mut omens,
        )
        .expect("aug must succeed when prefix slot is open and at least one prefix is eligible");

        // Aug targets the open prefix slot; the new mod is the prefix added
        // by sampling. Two prefixes possible; the suffix is the locked filler.
        assert_eq!(
            item.prefixes.len(),
            1,
            "trial {trial}: aug should add 1 prefix"
        );
        match item.prefixes[0].mod_id.as_str() {
            "ModA" => count_a += 1,
            "ModB" => count_b += 1,
            other => panic!("trial {trial}: unexpected mod id {other}"),
        }
    }

    let p_b = count_b as f64 / TRIALS as f64;
    // True proportion = 100 / 1100 ≈ 0.0909.
    let expected_p_b = 100.0 / 1100.0;
    // Binomial standard error: sqrt(p (1-p) / n) ≈ 0.00287.
    // 3σ window ≈ 0.0086 — well below the 0.02 we'd need to cross either
    // side and still falsely accept. Use 4σ as an extra-paranoid floor.
    let stderr = (expected_p_b * (1.0 - expected_p_b) / TRIALS as f64).sqrt();
    let tolerance = 4.0 * stderr;
    assert!(
        (p_b - expected_p_b).abs() <= tolerance,
        "ModB sampled at {p_b:.4}; expected {expected_p_b:.4} ± {tolerance:.4} (4σ); \
         counts: A={count_a}, B={count_b}"
    );
}

#[test]
fn fallback_eligibility_is_uniform_when_no_observations_are_present() {
    // Sanity check: when bundle.weights is empty, the registry's
    // eligibility-only fallback (RePoE-fork tag flag) returns 1.0 for every
    // eligible mod, so sampling is uniform — the v2 baseline.
    let mods = vec![
        mk_prefix_mod("ModA", "GroupA"),
        mk_prefix_mod("ModB", "GroupB"),
        mk_suffix_mod_for_filler("Filler", "FillerGroup"),
    ];
    let registry = ModRegistry::from_mods(mods, vec![]);

    let mut count_a = 0usize;
    let mut count_b = 0usize;
    for trial in 0..TRIALS {
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0xFA11_BACC ^ trial as u64);
        let mut omens = OmenSet::new();
        let mut item = mk_filled_magic_item();
        apply_currency(
            &OrbOfAugmentation::new(),
            &mut item,
            &registry,
            &mut rng,
            PATCH,
            &mut omens,
        )
        .unwrap();
        match item.prefixes[0].mod_id.as_str() {
            "ModA" => count_a += 1,
            "ModB" => count_b += 1,
            other => panic!("trial {trial}: unexpected mod id {other}"),
        }
    }

    let p_b = count_b as f64 / TRIALS as f64;
    // Expected 0.5 ± a few sigma.
    let stderr = (0.5 * 0.5 / TRIALS as f64).sqrt();
    let tolerance = 4.0 * stderr;
    assert!(
        (p_b - 0.5).abs() <= tolerance,
        "uniform-eligibility fallback should split ~50/50; got {p_b:.4} \
         (counts: A={count_a}, B={count_b})"
    );
}
