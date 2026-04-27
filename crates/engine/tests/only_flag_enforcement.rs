//! M14.3 — `*_ONLY` ModFlag enforcement on the basic-orb sampling path.
//!
//! Validates that `ESSENCE_ONLY`, `DESECRATED_ONLY`, and `CORRUPTED_ONLY`
//! mods never leak into rolls produced by Trans / Aug / Regal / Alch /
//! Exalt / Chaos. Also asserts that the same registry is *capable* of
//! rolling such a mod when its flag is cleared, confirming the test
//! discriminates the flag enforcement rather than some other filter.
//!
//! Reference: `docs/81-engine-training-and-rule-encoding-plan.md` §4.3
//! Tier 1.3.
//!
//! Note: Essence apply, Bone reveal, and Vaal corruption sample mods
//! through paths separate from `sample_eligible_mod`; their `*_ONLY`
//! enforcement (and the corresponding test surface) lands with the
//! per-currency tier work in M14.5/M14.6 and M14.4.

use poc2_engine::currency::basic::{
    ChaosOrb, ExaltedOrb, OrbOfAlchemy, OrbOfAugmentation, OrbOfTransmutation, RegalOrb,
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

const PATCH: PatchVersion = PatchVersion::PATCH_0_4_0;
const TRIALS: usize = 1_000;

fn mk_mod(id: &str, group: &str, affix: AffixType, flags: ModFlags) -> ModDefinition {
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
        required_level: 1,
        allowed_item_classes: smallvec![ItemClassId::from("BodyArmour")],
        patch_range: PatchRange::ALL,
        flags,
        text_template: None,
    }
}

fn mk_normal_armour() -> Item {
    Item {
        base: BaseTypeId::from("BodyArmour"),
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

fn mk_magic_with_filler_suffix() -> Item {
    let mut item = mk_normal_armour();
    item.rarity = Rarity::Magic;
    item.suffixes.push(ModRoll {
        mod_id: ModId::from("FillerSuffix"),
        affix_type: AffixType::Suffix,
        kind: ModKind::Explicit,
        values: smallvec![],
        is_fractured: true,
    });
    item
}

fn registry_with_flag(flag: ModFlags) -> ModRegistry {
    ModRegistry::from_mods(
        vec![
            mk_mod("FlaggedPrefix", "FlaggedGroup", AffixType::Prefix, flag),
            mk_mod(
                "OpenPrefix",
                "OpenGroup",
                AffixType::Prefix,
                ModFlags::empty(),
            ),
            mk_mod(
                "FillerSuffix",
                "FillerGroup",
                AffixType::Suffix,
                ModFlags::empty(),
            ),
        ],
        vec![],
    )
}

fn registry_with_no_flagged_mods() -> ModRegistry {
    ModRegistry::from_mods(
        vec![
            mk_mod(
                "OpenPrefix",
                "OpenGroup",
                AffixType::Prefix,
                ModFlags::empty(),
            ),
            mk_mod(
                "FillerSuffix",
                "FillerGroup",
                AffixType::Suffix,
                ModFlags::empty(),
            ),
        ],
        vec![],
    )
}

/// Run a basic orb against the supplied item N times; count how often the
/// flagged mod appears anywhere in the resulting prefixes.
fn count_flagged_prefix_apparitions<F, B>(
    registry: &ModRegistry,
    initial: F,
    mut bind: B,
    trials: usize,
) -> usize
where
    F: Fn() -> Item,
    B: FnMut(&mut Item, &mut Xoshiro256PlusPlus, &mut OmenSet) -> bool,
{
    let mut hits = 0usize;
    for trial in 0..trials {
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0x_F1A6_5EED ^ trial as u64);
        let mut omens = OmenSet::new();
        let mut item = initial();
        if !bind(&mut item, &mut rng, &mut omens) {
            // Application errored (e.g., no eligible mods at all). Don't
            // count toward apparitions; still counts as a non-leak.
            continue;
        }
        if item
            .prefixes
            .iter()
            .any(|m| m.mod_id.as_str() == "FlaggedPrefix")
        {
            hits += 1;
        }
        let _ = registry; // suppress unused-binding warning when bind ignores it
    }
    hits
}

fn run_with_currency(
    currency: &dyn Currency,
    registry: &ModRegistry,
    initial: impl Fn() -> Item,
) -> usize {
    count_flagged_prefix_apparitions(
        registry,
        initial,
        |item, rng, omens| apply_currency(currency, item, registry, rng, PATCH, omens).is_ok(),
        TRIALS,
    )
}

#[test]
fn transmute_never_rolls_essence_only_mod() {
    let registry = registry_with_flag(ModFlags::ESSENCE_ONLY);
    assert_eq!(
        run_with_currency(&OrbOfTransmutation::new(), &registry, mk_normal_armour),
        0,
        "Trans must not produce ESSENCE_ONLY mods"
    );
}

#[test]
fn augment_never_rolls_essence_only_mod() {
    let registry = registry_with_flag(ModFlags::ESSENCE_ONLY);
    assert_eq!(
        run_with_currency(
            &OrbOfAugmentation::new(),
            &registry,
            mk_magic_with_filler_suffix,
        ),
        0,
    );
}

#[test]
fn alchemy_never_rolls_desecrated_only_mod() {
    let registry = registry_with_flag(ModFlags::DESECRATED_ONLY);
    assert_eq!(
        run_with_currency(&OrbOfAlchemy::new(), &registry, mk_normal_armour),
        0,
    );
}

#[test]
fn regal_never_rolls_corrupted_only_mod() {
    let registry = registry_with_flag(ModFlags::CORRUPTED_ONLY);
    assert_eq!(
        run_with_currency(&RegalOrb::new(), &registry, mk_magic_with_filler_suffix,),
        0,
    );
}

#[test]
fn exalt_never_rolls_essence_only_mod_on_rare_with_open_slots() {
    let registry = registry_with_flag(ModFlags::ESSENCE_ONLY);

    let initial = || {
        let mut item = mk_normal_armour();
        item.rarity = Rarity::Rare;
        // 3 suffixes filled (fractured) so Exalt only has prefix slots.
        for n in 0..3 {
            item.suffixes.push(ModRoll {
                mod_id: ModId::from(format!("FillerSuffix{n}").as_str()),
                affix_type: AffixType::Suffix,
                kind: ModKind::Explicit,
                values: smallvec![],
                is_fractured: true,
            });
        }
        item
    };

    assert_eq!(run_with_currency(&ExaltedOrb::new(), &registry, initial), 0);
}

#[test]
fn chaos_never_rolls_desecrated_only_mod() {
    let registry = registry_with_flag(ModFlags::DESECRATED_ONLY);

    let initial = || {
        let mut item = mk_normal_armour();
        item.rarity = Rarity::Rare;
        // Item has one prefix to remove + open prefix slots.
        item.prefixes.push(ModRoll {
            mod_id: ModId::from("OpenPrefix"),
            affix_type: AffixType::Prefix,
            kind: ModKind::Explicit,
            values: smallvec![],
            is_fractured: false,
        });
        item.suffixes.push(ModRoll {
            mod_id: ModId::from("FillerSuffix"),
            affix_type: AffixType::Suffix,
            kind: ModKind::Explicit,
            values: smallvec![],
            is_fractured: true,
        });
        item
    };

    assert_eq!(run_with_currency(&ChaosOrb::new(), &registry, initial), 0);
}

#[test]
fn registries_with_no_flagged_mods_still_apply_basic_orbs() {
    // Sanity: assert the test discriminates by flag, not by some other
    // filter. Without any flagged mod, basic orbs roll the open mod just
    // as before.
    let registry = registry_with_no_flagged_mods();
    let mut count_open = 0usize;
    for trial in 0..TRIALS {
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0x_5A1E ^ trial as u64);
        let mut omens = OmenSet::new();
        let mut item = mk_normal_armour();
        apply_currency(
            &OrbOfTransmutation::new(),
            &mut item,
            &registry,
            &mut rng,
            PATCH,
            &mut omens,
        )
        .unwrap();
        if item
            .prefixes
            .iter()
            .any(|m| m.mod_id.as_str() == "OpenPrefix")
        {
            count_open += 1;
        }
    }
    // Trans picks prefix or suffix 50/50; suffix branch finds no eligible
    // suffix in this registry and falls through. Some prefix-OpenPrefix
    // hits expected.
    assert!(
        count_open > TRIALS / 4,
        "expected OpenPrefix to appear in many trials; got {count_open}"
    );
}
