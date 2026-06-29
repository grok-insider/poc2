//! P1/P2 — Minimum-Modifier-Level pool correctness across currency variants.
//!
//! This is the user's worked example: "exalt, greater exalt and perfect
//! exalt will have different pools since some mods are not taken in count
//! depending on item level or because they cant roll on low tier of a
//! modifier."
//!
//! What we assert:
//! 1. **Strictly nested shrinking pools.** The set of mods an Exalted Orb can
//!    add ⊇ the set a Greater Exalted Orb can add ⊇ the set a Perfect Exalted
//!    Orb can add, on the *same* Rare item at the *same* ilvl.
//! 2. **Floor boundaries.** A tier whose `required_level` is exactly the
//!    floor is included; one below is excluded.
//! 3. **The keep-≥1-tier exception.** A mod-group whose every tier is below
//!    the floor still contributes its highest tier — no entire mod-type is
//!    deleted by a Min-Modifier-Level floor (GGG: "at least one tier of each
//!    mod type will always be eligible, respecting item level").
//!
//! Methodology: rather than reach into the private sampler, we run many
//! seeded Exalt applications on a fresh Rare item with two open prefix slots
//! and collect the *set of mods ever produced*. With enough trials the
//! observed set converges to the true eligible pool. We assert set
//! membership/exclusion, which is robust to sampling noise (a mod either can
//! or cannot appear).

use poc2_engine::currency::basic::{
    ExaltedOrb, GreaterChaosOrb, GreaterExaltedOrb, GreaterOrbOfAugmentation,
    GreaterOrbOfTransmutation, PerfectExaltedOrb, PerfectOrbOfAugmentation,
};
use poc2_engine::ids::TagId;
use poc2_engine::omen::OmenSet;
use poc2_engine::patch::PatchVersion;
use poc2_engine::{
    apply_currency, AffixType, BaseTypeId, Currency, Item, ItemClassId, ModDefinition, ModDomain,
    ModFlags, ModGroup, ModGroupId, ModId, ModKind, ModRegistry, ModRoll, PatchRange, QualityKind,
    Rarity, SpawnWeight,
};
use rand_xoshiro::rand_core::SeedableRng;
use rand_xoshiro::Xoshiro256PlusPlus;
use smallvec::smallvec;
use std::collections::BTreeSet;

const PATCH: PatchVersion = PatchVersion::PATCH_0_4_0;
const TRIALS: usize = 4_000;
const CLASS: &str = "BodyArmour";
const BASE: &str = "BodyArmour";

/// A prefix mod of a given group at a given required-level (tier proxy).
fn prefix_at(id: &str, group: &str, required_level: u32) -> ModDefinition {
    ModDefinition {
        id: ModId::from(id),
        name: None,
        mod_group: ModGroup(ModGroupId::from(group)),
        affix_type: AffixType::Prefix,
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
        patch_range: PatchRange::ALL,
        flags: ModFlags::empty(),
        text_template: None,
    }
}

fn obs(mod_id: &str, weight: f64) -> poc2_engine::weights::WeightObservation {
    poc2_engine::weights::WeightObservation {
        mod_id: ModId::from(mod_id),
        scope: poc2_engine::weights::WeightScope::Base {
            base: BaseTypeId::from(BASE),
        },
        primary_weight: weight,
        secondary_weight: None,
        confidence: poc2_engine::weights::Confidence::Community,
        note: None,
    }
}

/// A Rare item at the given ilvl with two open prefix slots (3 suffixes
/// filled with fractured fillers so Exalt always targets a prefix). Fractured
/// so they are never removed/considered for group exclusivity surprises.
fn rare_with_open_prefixes(ilvl: u32) -> Item {
    let mut item = Item {
        base: BaseTypeId::from(BASE),
        ilvl,
        rarity: Rarity::Rare,
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
    for i in 0..3 {
        item.suffixes.push(ModRoll {
            mod_id: ModId::from(format!("SufFiller{i}").as_str()),
            affix_type: AffixType::Suffix,
            kind: ModKind::Explicit,
            values: smallvec![],
            is_fractured: true,
        });
    }
    item
}

/// Collect the set of prefix mod-ids an Exalt-style currency can add to a
/// fresh Rare item at `ilvl`, over many seeded trials.
fn observed_pool(currency: &dyn Currency, registry: &ModRegistry, ilvl: u32) -> BTreeSet<String> {
    let mut seen = BTreeSet::new();
    for trial in 0..TRIALS {
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0x9E37_79B9 ^ trial as u64);
        let mut omens = OmenSet::new();
        let mut item = rare_with_open_prefixes(ilvl);
        if apply_currency(currency, &mut item, registry, &mut rng, PATCH, &mut omens).is_ok() {
            if let Some(p) = item.prefixes.first() {
                seen.insert(p.mod_id.as_str().to_string());
            }
        }
    }
    seen
}

/// Registry with a single mod-group "Phys" laddered across tiers at
/// required-levels 1 / 40 / 55 / 70, plus a low-only "LightRadius" group
/// whose only tier requires level 30 (used for the keep-≥1-tier exception).
fn laddered_registry() -> ModRegistry {
    let mods = vec![
        prefix_at("Phys_T_low", "Phys", 1),
        prefix_at("Phys_T_mid", "Phys", 40),
        prefix_at("Phys_T_hi", "Phys", 55),
        prefix_at("Phys_T_top", "Phys", 70),
        // A mod-type whose *only* tier requires level 30 — below the Perfect
        // floor of 70 and the Greater floor of 35. Used to verify the
        // keep-≥1-tier exception.
        prefix_at("LightRadius_T1", "LightRadius", 30),
        // Suffix fillers' groups must resolve for exclusivity checks.
        {
            let mut m = prefix_at("SufFiller0", "Filler0", 1);
            m.affix_type = AffixType::Suffix;
            m
        },
        {
            let mut m = prefix_at("SufFiller1", "Filler1", 1);
            m.affix_type = AffixType::Suffix;
            m
        },
        {
            let mut m = prefix_at("SufFiller2", "Filler2", 1);
            m.affix_type = AffixType::Suffix;
            m
        },
    ];
    // Per-tier numeric weights (inclusive weighting sums these up the ladder).
    let weights = vec![
        obs("Phys_T_low", 1000.0),
        obs("Phys_T_mid", 400.0),
        obs("Phys_T_hi", 150.0),
        obs("Phys_T_top", 50.0),
        obs("LightRadius_T1", 800.0),
    ];
    ModRegistry::from_mods(mods, weights)
}

#[test]
fn exalt_variants_form_strictly_nested_pools_at_high_ilvl() {
    let registry = laddered_registry();
    let ilvl = 82;

    let exalt = observed_pool(&ExaltedOrb::new(), &registry, ilvl);
    let greater = observed_pool(&GreaterExaltedOrb::new(), &registry, ilvl);
    let perfect = observed_pool(&PerfectExaltedOrb::new(), &registry, ilvl);

    // Plain Exalt: every Phys tier (≥1) is eligible at ilvl 82, plus
    // LightRadius. So the low Phys tier appears.
    assert!(
        exalt.contains("Phys_T_low"),
        "plain Exalt must be able to roll the lowest Phys tier; got {exalt:?}"
    );

    // Nesting: greater ⊆ exalt, perfect ⊆ greater.
    assert!(
        greater.is_subset(&exalt),
        "Greater pool must be a subset of Exalt pool.\n  greater={greater:?}\n  exalt={exalt:?}"
    );
    assert!(
        perfect.is_subset(&greater),
        "Perfect pool must be a subset of Greater pool.\n  perfect={perfect:?}\n  greater={greater:?}"
    );

    // The pools must actually differ (shrink), or the test proves nothing.
    assert!(
        greater.len() < exalt.len(),
        "Greater pool should be strictly smaller than Exalt; \
         exalt={exalt:?}, greater={greater:?}"
    );
}

#[test]
fn greater_floor_excludes_sub_35_phys_tier_but_keeps_higher() {
    let registry = laddered_registry();
    // Greater Exalt floor (engine const today = 50; wiki says 35). Either
    // way, the lowest Phys tier (required_level 1) is below the floor and the
    // top tier (70) is above it, so we assert the *relative* exclusion rather
    // than a hard-coded floor value.
    let greater = observed_pool(&GreaterExaltedOrb::new(), &laddered_registry(), 82);
    let _ = registry;

    assert!(
        !greater.contains("Phys_T_low"),
        "Greater Exalt must NOT roll the level-1 Phys tier (below floor); got {greater:?}"
    );
    assert!(
        greater.contains("Phys_T_top"),
        "Greater Exalt must still roll the level-70 Phys tier; got {greater:?}"
    );
}

#[test]
fn perfect_floor_keeps_one_tier_of_a_fully_subfloor_mod_type() {
    // The keep-≥1-tier exception: LightRadius's only tier requires level 30,
    // which is below the Perfect floor. It must STILL be eligible because a
    // floor never deletes an entire mod-type.
    let registry = laddered_registry();
    let perfect = observed_pool(&PerfectExaltedOrb::new(), &registry, 82);

    assert!(
        perfect.contains("LightRadius_T1"),
        "Perfect Exalt must keep the only (sub-floor) tier of LightRadius via \
         the keep-≥1-tier exception; got {perfect:?}"
    );
    // But the multi-tier Phys group's sub-floor tiers are NOT kept (the group
    // has tiers above the floor, so the exception does not fire for it).
    assert!(
        !perfect.contains("Phys_T_low"),
        "Perfect Exalt must not roll the sub-floor Phys tier when the Phys \
         group has above-floor tiers; got {perfect:?}"
    );
}

#[test]
fn ilvl_gates_the_pool_top() {
    // At low ilvl, high tiers are not yet unlocked. A level-1 item can only
    // roll the level-1 Phys tier and (no) LightRadius (requires 30).
    let registry = laddered_registry();

    let low = observed_pool(&ExaltedOrb::new(), &registry, 1);
    assert!(
        low.contains("Phys_T_low"),
        "ilvl 1 should still roll the level-1 Phys tier; got {low:?}"
    );
    assert!(
        !low.contains("Phys_T_mid") && !low.contains("Phys_T_hi") && !low.contains("Phys_T_top"),
        "ilvl 1 must not unlock any higher Phys tier; got {low:?}"
    );
    assert!(
        !low.contains("LightRadius_T1"),
        "ilvl 1 must not unlock LightRadius (requires level 30); got {low:?}"
    );

    // Boundary: ilvl exactly 30 unlocks LightRadius; ilvl 29 does not.
    let at_30 = observed_pool(&ExaltedOrb::new(), &registry, 30);
    assert!(
        at_30.contains("LightRadius_T1"),
        "ilvl == required_level (30) must include the tier; got {at_30:?}"
    );
    let at_29 = observed_pool(&ExaltedOrb::new(), &registry, 29);
    assert!(
        !at_29.contains("LightRadius_T1"),
        "ilvl one below required_level must exclude the tier; got {at_29:?}"
    );
}

// =========================================================================
// P2 — Patch-versioned Greater/Perfect floors over the public orb types.
// =========================================================================

/// A fresh **Normal** item at `ilvl` (no mods). Used for orbs that operate on
/// a Normal item (Greater/Perfect Transmutation, which promote Normal→Magic).
fn fresh_normal(ilvl: u32) -> Item {
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

/// Parametric variant of [`observed_pool`] that threads an explicit `patch`
/// and starts from a fresh **Normal** item (so Transmutation-class orbs which
/// promote Normal→Magic apply cleanly). Collects the set of mod-ids that ever
/// land on the single added prefix-or-suffix slot across many seeded trials.
fn observed_pool_patch(
    currency: &dyn Currency,
    registry: &ModRegistry,
    ilvl: u32,
    patch: PatchVersion,
) -> BTreeSet<String> {
    let mut seen = BTreeSet::new();
    for trial in 0..TRIALS {
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0x9E37_79B9 ^ trial as u64);
        let mut omens = OmenSet::new();
        let mut item = fresh_normal(ilvl);
        if apply_currency(currency, &mut item, registry, &mut rng, patch, &mut omens).is_ok() {
            let added = item.prefixes.first().or_else(|| item.suffixes.first());
            if let Some(p) = added {
                seen.insert(p.mod_id.as_str().to_string());
            }
        }
    }
    seen
}

/// Registry with a single prefix mod-group "Phys" whose tiers straddle the
/// Greater-Transmute floors of both patches: required_level 1 / 25 / 40 / 75.
/// The 0.4 floor (35) excludes the level-25 tier; the 0.5 floor (20) keeps it.
fn straddle_registry() -> ModRegistry {
    // The level-50 tier sits in the (44, 55] gap between the 0.5 Greater
    // Transmute floor (44) and the 0.4 floor (55), so it is the tier that
    // discriminates the two patches. Level 1 is below both floors; level 75
    // is above both.
    let mods = vec![
        prefix_at("Phys_T_1", "Phys", 1),
        prefix_at("Phys_T_50", "Phys", 50),
        prefix_at("Phys_T_75", "Phys", 75),
    ];
    let weights = vec![
        obs("Phys_T_1", 1000.0),
        obs("Phys_T_50", 300.0),
        obs("Phys_T_75", 50.0),
    ];
    ModRegistry::from_mods(mods, weights)
}

#[test]
fn greater_transmute_pool_grows_in_0_5_vs_0_4() {
    // Greater Orb of Transmutation works on a NORMAL item (promotes to Magic),
    // so we start from a fresh Normal at high ilvl. Its Min-Modifier-Level
    // floor dropped from 55 (0.4) to 44 (0.5) in "Return of the Ancients"
    // (0.5.0 patch notes). With Phys tiers at 1 / 50 / 75:
    //   - 0.4 floor 55: excludes tier-1 and tier-50, keeps tier-75.
    //   - 0.5 floor 44: excludes tier-1, keeps tier-50 and tier-75.
    // So the 0.5 pool is a STRICT SUPERSET, gaining exactly the level-50 tier.
    let registry = straddle_registry();
    let ilvl = 82;

    let pool_04 = observed_pool_patch(
        &GreaterOrbOfTransmutation::new(),
        &registry,
        ilvl,
        PatchVersion::PATCH_0_4_0,
    );
    let pool_05 = observed_pool_patch(
        &GreaterOrbOfTransmutation::new(),
        &registry,
        ilvl,
        PatchVersion::PATCH_0_5_0,
    );

    // 0.4 keeps the above-55 tier but not the level-50 tier.
    assert!(
        pool_04.contains("Phys_T_75"),
        "0.4 Greater Transmute must keep the above-floor tier; got {pool_04:?}"
    );
    assert!(
        !pool_04.contains("Phys_T_50"),
        "0.4 floor (55) must exclude the level-50 tier; got {pool_04:?}"
    );

    // 0.5 keeps the level-50 tier the 0.4 floor excluded.
    assert!(
        pool_05.contains("Phys_T_50"),
        "0.5 floor (44) must include the level-50 tier; got {pool_05:?}"
    );

    // Strict superset: every 0.4 mod is in 0.5, and 0.5 has at least one more.
    assert!(
        pool_04.is_subset(&pool_05),
        "0.5 pool must be a superset of the 0.4 pool.\n  0.4={pool_04:?}\n  0.5={pool_05:?}"
    );
    assert!(
        pool_05.len() > pool_04.len(),
        "0.5 pool must be STRICTLY larger than the 0.4 pool.\n  0.4={pool_04:?}\n  0.5={pool_05:?}"
    );

    // Neither floor ever lets the level-1 tier through (1 < 44 < 55).
    assert!(
        !pool_04.contains("Phys_T_1") && !pool_05.contains("Phys_T_1"),
        "the level-1 tier is below both floors and must never appear"
    );
}

/// Registry whose Phys prefix group straddles the Greater-Augment 0.4 floor
/// (55) and the Perfect-Augment floor (70): tiers at 1 / 40 / 60 / 75. The
/// item is seeded with one suffix filler so an Augment always targets the
/// open prefix slot and rolls from the Phys group.
fn augment_straddle_registry() -> ModRegistry {
    let mods = vec![
        prefix_at("Phys_T_1", "Phys", 1),
        prefix_at("Phys_T_40", "Phys", 40),
        prefix_at("Phys_T_60", "Phys", 60),
        prefix_at("Phys_T_75", "Phys", 75),
        {
            // A pre-existing Magic suffix so the lone open slot is a prefix.
            let mut m = prefix_at("SufSeed", "SufSeedGroup", 1);
            m.affix_type = AffixType::Suffix;
            m
        },
    ];
    let weights = vec![
        obs("Phys_T_1", 1000.0),
        obs("Phys_T_40", 500.0),
        obs("Phys_T_60", 200.0),
        obs("Phys_T_75", 80.0),
        obs("SufSeed", 1.0),
    ];
    ModRegistry::from_mods(mods, weights)
}

/// A Magic item carrying exactly one suffix (`SufSeed`) so an Orb of
/// Augmentation fills the single open prefix slot from the Phys group.
fn magic_with_one_suffix(ilvl: u32) -> Item {
    let mut item = fresh_normal(ilvl);
    item.rarity = Rarity::Magic;
    item.suffixes.push(ModRoll {
        mod_id: ModId::from("SufSeed"),
        affix_type: AffixType::Suffix,
        kind: ModKind::Explicit,
        values: smallvec![],
        is_fractured: false,
    });
    item
}

/// Observed prefix pool for an Augment-class orb over a Magic item that
/// already has one suffix, at the given patch.
fn observed_augment_pool(
    currency: &dyn Currency,
    registry: &ModRegistry,
    ilvl: u32,
    patch: PatchVersion,
) -> BTreeSet<String> {
    let mut seen = BTreeSet::new();
    for trial in 0..TRIALS {
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0x1234_5678 ^ trial as u64);
        let mut omens = OmenSet::new();
        let mut item = magic_with_one_suffix(ilvl);
        if apply_currency(currency, &mut item, registry, &mut rng, patch, &mut omens).is_ok() {
            if let Some(p) = item.prefixes.first() {
                seen.insert(p.mod_id.as_str().to_string());
            }
        }
    }
    seen
}

#[test]
fn greater_augment_floor_enforced() {
    // Greater Orb of Augmentation floor is 55 in 0.4. With Phys tiers at
    // 1 / 40 / 60 / 75, the sub-floor tiers (1, 40) must NEVER appear, while
    // an above-floor tier (60, 75) does. The Phys group has above-floor tiers,
    // so the keep-≥1-tier exception does not fire for it.
    let registry = augment_straddle_registry();
    let pool = observed_augment_pool(
        &GreaterOrbOfAugmentation::new(),
        &registry,
        82,
        PatchVersion::PATCH_0_4_0,
    );

    assert!(
        !pool.contains("Phys_T_1") && !pool.contains("Phys_T_40"),
        "Greater Augment (floor 55) must never roll a sub-floor tier; got {pool:?}"
    );
    assert!(
        pool.contains("Phys_T_60") || pool.contains("Phys_T_75"),
        "Greater Augment must be able to roll an above-floor tier; got {pool:?}"
    );
    // Every observed mod's required_level must be >= the floor (55).
    for id in &pool {
        let rl: u32 = id.trim_start_matches("Phys_T_").parse().unwrap();
        assert!(
            rl >= 55,
            "Greater Augment rolled {id} (req {rl}) below the 0.4 floor of 55"
        );
    }
}

#[test]
fn perfect_augment_floor_enforced() {
    // Perfect Orb of Augmentation floor is 70. Only Phys_T_75 qualifies;
    // tiers 1 / 40 / 60 are below the floor and must never appear.
    let registry = augment_straddle_registry();
    let pool = observed_augment_pool(
        &PerfectOrbOfAugmentation::new(),
        &registry,
        82,
        PatchVersion::PATCH_0_4_0,
    );

    assert!(
        pool.contains("Phys_T_75"),
        "Perfect Augment must roll the level-75 tier (above floor 70); got {pool:?}"
    );
    assert!(
        !pool.contains("Phys_T_1") && !pool.contains("Phys_T_40") && !pool.contains("Phys_T_60"),
        "Perfect Augment (floor 70) must never roll any sub-floor tier; got {pool:?}"
    );
    for id in &pool {
        let rl: u32 = id.trim_start_matches("Phys_T_").parse().unwrap();
        assert!(
            rl >= 70,
            "Perfect Augment rolled {id} (req {rl}) below the floor of 70"
        );
    }
}

/// Rare item carrying a single low (sub-floor) Phys mod plus 3 fractured
/// suffix fillers, so a Chaos Orb is forced to remove the low Phys mod and add
/// a replacement prefix from the multi-tier Phys group.
fn rare_with_low_phys(ilvl: u32) -> Item {
    let mut item = rare_with_open_prefixes(ilvl);
    item.prefixes.push(ModRoll {
        mod_id: ModId::from("Phys_T_low"),
        affix_type: AffixType::Prefix,
        kind: ModKind::Explicit,
        values: smallvec![],
        is_fractured: false,
    });
    item
}

#[test]
fn greater_chaos_replacement_respects_floor() {
    // Greater Chaos floor is 50. The Rare starts with a sub-floor Phys mod
    // (req 1); Greater Chaos removes it and adds a replacement. The Phys group
    // has above-floor tiers (55 / 70), so the keep-≥1-tier exception does NOT
    // fire — every replacement must be required_level >= 50.
    let registry = laddered_registry();
    let mut seen = BTreeSet::new();
    for trial in 0..TRIALS {
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0xC0FF_EE00 ^ trial as u64);
        let mut omens = OmenSet::new();
        let mut item = rare_with_low_phys(82);
        if apply_currency(
            &GreaterChaosOrb::new(),
            &mut item,
            &registry,
            &mut rng,
            PatchVersion::PATCH_0_4_0,
            &mut omens,
        )
        .is_ok()
        {
            // The single (replacement) prefix.
            if let Some(p) = item.prefixes.first() {
                seen.insert(p.mod_id.as_str().to_string());
            }
        }
    }

    // The sub-floor Phys tiers must never be the replacement.
    assert!(
        !seen.contains("Phys_T_low") && !seen.contains("Phys_T_mid"),
        "Greater Chaos (floor 50) must not roll sub-50 Phys tiers as replacement; got {seen:?}"
    );
    // An above-floor Phys tier must be reachable as a replacement.
    assert!(
        seen.contains("Phys_T_hi") || seen.contains("Phys_T_top"),
        "Greater Chaos must reach an above-floor Phys tier; got {seen:?}"
    );
    // Every observed replacement is at/above the floor of 50. LightRadius_T1
    // (req 30) is its OWN single-tier group; with the Phys mod removed it can
    // legitimately appear via the keep-≥1-tier exception, so we only assert
    // the Phys-group floor here.
    for id in &seen {
        if let Some(rest) = id.strip_prefix("Phys_T_") {
            let rl = match rest {
                "low" => 1,
                "mid" => 40,
                "hi" => 55,
                "top" => 70,
                _ => unreachable!("unexpected Phys tier {id}"),
            };
            assert!(
                rl >= 50,
                "Greater Chaos rolled Phys {id} (req {rl}) below floor 50"
            );
        }
    }
}

#[test]
fn keep_one_tier_exception_does_not_fire_when_above_floor_tier_exists() {
    // Deterministic restatement of the exception's *negative* case: the Phys
    // group in the laddered registry has tiers at 1 / 40 / 55 / 70. A Perfect
    // Exalted Orb (floor 50) keeps the above-floor tiers (55 / 70); because the
    // group HAS above-floor tiers, the keep-≥1-tier exception does NOT fire,
    // so the sub-floor tiers (1 / 40) must NEVER be rolled.
    let registry = laddered_registry();
    let perfect = observed_pool(&PerfectExaltedOrb::new(), &registry, 82);

    for sub in ["Phys_T_low", "Phys_T_mid"] {
        assert!(
            !perfect.contains(sub),
            "Perfect orb must never roll sub-floor Phys tier {sub} when an \
             above-floor tier exists; got {perfect:?}"
        );
    }
    // Sanity: an above-floor Phys tier IS reachable, proving the group is in
    // the pool at all (so the exclusion above is meaningful, not vacuous).
    assert!(
        perfect.contains("Phys_T_hi") || perfect.contains("Phys_T_top"),
        "Perfect orb must roll an above-floor Phys tier (55 or 70); got {perfect:?}"
    );
}
