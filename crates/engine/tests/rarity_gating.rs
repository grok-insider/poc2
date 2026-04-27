//! Phase A test (Crafter helper v2 plan).
//!
//! Validates that every concrete `Currency` implementation reports the
//! correct accepted rarity set, and that `can_apply_to` rejects out-of-rarity
//! items with the expected `CannotApply::WrongRarity` reason.
//!
//! Reference: docs/80-crafter-helper-v2-plan.md §4 (Phase A).

use poc2_engine::{
    currency::basic::{
        ChaosOrb, DivineOrb, ExaltedOrb, GreaterChaosOrb, GreaterExaltedOrb,
        GreaterOrbOfAugmentation, GreaterOrbOfTransmutation, GreaterRegalOrb, OrbOfAlchemy,
        OrbOfAnnulment, OrbOfAugmentation, OrbOfTransmutation, PerfectChaosOrb, PerfectExaltedOrb,
        PerfectOrbOfAugmentation, PerfectOrbOfTransmutation, PerfectRegalOrb, RegalOrb, VaalOrb,
    },
    item::{Item, Rarity},
    CannotApply, Currency, FracturingOrb, HinekorasLock, RaritySet,
};

fn fresh(rarity: Rarity) -> Item {
    Item {
        base: poc2_engine::ids::ItemClassId::from("BodyArmour")
            .as_str()
            .into(),
        ilvl: 82,
        rarity,
        corrupted: false,
        sanctified: false,
        mirrored: false,
        quality: 0,
        quality_kind: poc2_engine::item::QualityKind::Untagged,
        implicits: smallvec::smallvec![],
        prefixes: smallvec::smallvec![],
        suffixes: smallvec::smallvec![],
        enchantments: smallvec::smallvec![],
        hidden_desecrated: None,
        sockets: smallvec::smallvec![],
        hinekora_lock: None,
    }
}

/// Each currency under test paired with the exact `RaritySet` the plan
/// (`docs/80-crafter-helper-v2-plan.md` §4) requires.
fn cases() -> Vec<(Box<dyn Currency>, RaritySet, &'static str)> {
    vec![
        (
            Box::new(OrbOfTransmutation::new()),
            RaritySet::NORMAL,
            "OrbOfTransmutation",
        ),
        (
            Box::new(GreaterOrbOfTransmutation::new()),
            RaritySet::NORMAL,
            "GreaterOrbOfTransmutation",
        ),
        (
            Box::new(PerfectOrbOfTransmutation::new()),
            RaritySet::NORMAL,
            "PerfectOrbOfTransmutation",
        ),
        (
            Box::new(OrbOfAlchemy::new()),
            RaritySet::NORMAL,
            "OrbOfAlchemy",
        ),
        (
            Box::new(OrbOfAugmentation::new()),
            RaritySet::MAGIC,
            "OrbOfAugmentation",
        ),
        (
            Box::new(GreaterOrbOfAugmentation::new()),
            RaritySet::MAGIC,
            "GreaterOrbOfAugmentation",
        ),
        (
            Box::new(PerfectOrbOfAugmentation::new()),
            RaritySet::MAGIC,
            "PerfectOrbOfAugmentation",
        ),
        (Box::new(RegalOrb::new()), RaritySet::MAGIC, "RegalOrb"),
        (
            Box::new(GreaterRegalOrb::new()),
            RaritySet::MAGIC,
            "GreaterRegalOrb",
        ),
        (
            Box::new(PerfectRegalOrb::new()),
            RaritySet::MAGIC,
            "PerfectRegalOrb",
        ),
        (Box::new(ExaltedOrb::new()), RaritySet::RARE, "ExaltedOrb"),
        (
            Box::new(GreaterExaltedOrb::new()),
            RaritySet::RARE,
            "GreaterExaltedOrb",
        ),
        (
            Box::new(PerfectExaltedOrb::new()),
            RaritySet::RARE,
            "PerfectExaltedOrb",
        ),
        (Box::new(ChaosOrb::new()), RaritySet::RARE, "ChaosOrb"),
        (
            Box::new(GreaterChaosOrb::new()),
            RaritySet::RARE,
            "GreaterChaosOrb",
        ),
        (
            Box::new(PerfectChaosOrb::new()),
            RaritySet::RARE,
            "PerfectChaosOrb",
        ),
        (
            Box::new(OrbOfAnnulment::new()),
            RaritySet::MAGIC.union(RaritySet::RARE),
            "OrbOfAnnulment",
        ),
        (
            Box::new(DivineOrb::new()),
            RaritySet::MAGIC
                .union(RaritySet::RARE)
                .union(RaritySet::UNIQUE),
            "DivineOrb",
        ),
        (Box::new(VaalOrb::new()), RaritySet::all(), "VaalOrb"),
        (
            Box::new(FracturingOrb::new()),
            RaritySet::RARE,
            "FracturingOrb",
        ),
        (
            Box::new(HinekorasLock::new()),
            RaritySet::NORMAL
                .union(RaritySet::MAGIC)
                .union(RaritySet::RARE),
            "HinekorasLock",
        ),
    ]
}

#[test]
fn currency_valid_rarities_match_plan() {
    for (currency, expected, label) in cases() {
        assert_eq!(
            currency.valid_rarities(),
            expected,
            "{label} must accept {expected:?}",
        );
    }
}

#[test]
fn can_apply_rejects_wrong_rarity_with_structured_reason() {
    for (currency, expected, label) in cases() {
        for r in [Rarity::Normal, Rarity::Magic, Rarity::Rare, Rarity::Unique] {
            let item = fresh(r);
            let result = currency.can_apply_to(&item);
            if expected.contains(r) {
                // Some currencies (Fracture) reject for non-rarity reasons even on Rare.
                if let Err(CannotApply::WrongRarity { .. }) = result {
                    panic!("{label} on {r:?} returned WrongRarity but rarity is allowed");
                }
            } else {
                match result {
                    Err(CannotApply::WrongRarity {
                        item_rarity,
                        expected: e,
                    }) => {
                        assert_eq!(item_rarity, r, "{label} echoed wrong rarity");
                        assert_eq!(e, expected, "{label} echoed wrong expected set");
                    }
                    other => panic!("{label} on {r:?}: expected WrongRarity, got {other:?}"),
                }
            }
        }
    }
}

#[test]
fn fracture_requires_four_mods_even_on_rare() {
    let frac = FracturingOrb::new();
    let item = fresh(Rarity::Rare);
    match frac.can_apply_to(&item) {
        Err(CannotApply::FractureRequiresFourMods { current }) => {
            assert_eq!(current, 0);
        }
        other => panic!("expected FractureRequiresFourMods, got {other:?}"),
    }
}

#[test]
fn rarity_set_iter_is_canonical() {
    let set = RaritySet::MAGIC.union(RaritySet::RARE);
    let collected: Vec<Rarity> = set.iter().collect();
    assert_eq!(collected, vec![Rarity::Magic, Rarity::Rare]);
}
