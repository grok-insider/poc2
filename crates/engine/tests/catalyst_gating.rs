//! M14.5 — Catalyst class gating.
//!
//! Validates that catalysts:
//! - `can_apply_to` rejects items whose class string is a known
//!   non-eligible class (BodyArmour, Boots, Gloves, Helmet, Sceptres,
//!   weapons, Quiver, Focus, Talisman, Waystone, Charm, Tablet).
//! - `can_apply_to` accepts the four eligible classes (Ring, Amulet,
//!   Belt, Jewel).
//! - `apply()` errors with `InvalidApplication` when the registry-backed
//!   class lookup resolves to an ineligible class on a real-bundle item
//!   (i.e., even when `can_apply_to`'s string heuristic missed because
//!   the item's `base` is a metadata path, the apply gate catches it).
//!
//! Reference: `docs/81-engine-training-and-rule-encoding-plan.md` §4.5
//! Tier 1.5.

use poc2_engine::base::{BaseType, InventorySize, ReleaseState};
use poc2_engine::ids::TagId;
use poc2_engine::item_class::AttributePool;
use poc2_engine::omen::OmenSet;
use poc2_engine::patch::PatchVersion;
use poc2_engine::{
    BaseRegistry, BaseTypeId, CannotApply, Catalyst, Currency, EngineError, Item, ItemClassId,
    ModRegistry, PatchRange, QualityKind, Rarity,
};
use rand_xoshiro::rand_core::SeedableRng;
use rand_xoshiro::Xoshiro256PlusPlus;
use smallvec::smallvec;

const PATCH: PatchVersion = PatchVersion::PATCH_0_4_0;

fn item_with_base(base_id: &str) -> Item {
    Item {
        base: BaseTypeId::from(base_id),
        ilvl: 82,
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
    }
}

#[test]
fn can_apply_to_accepts_eligible_classes() {
    let cat = Catalyst::flesh();
    for class in ["Ring", "Amulet", "Belt", "Jewel"] {
        let item = item_with_base(class);
        assert!(
            cat.can_apply_to(&item).is_ok(),
            "expected catalyst to accept {class}"
        );
    }
}

#[test]
fn can_apply_to_rejects_non_eligible_classes() {
    let cat = Catalyst::flesh();
    for class in [
        "BodyArmour",
        "Helmet",
        "Boots",
        "Gloves",
        "OneHandSword",
        "TwoHandSword",
        "Bow",
        "Crossbow",
        "Spear",
        "Staff",
        "Sceptre",
        "Wand",
        "Quiver",
        "Focus",
        "Talisman",
        "Waystone",
    ] {
        let item = item_with_base(class);
        match cat.can_apply_to(&item) {
            Err(CannotApply::Other(msg)) => {
                assert!(
                    msg.contains("Ring"),
                    "expected helpful error mentioning Ring/Amulet/Belt/Jewel; got {msg}"
                );
            }
            Err(other) => panic!("expected CannotApply::Other, got {other:?} for {class}"),
            Ok(()) => panic!("catalyst should reject {class}"),
        }
    }
}

#[test]
fn apply_errors_when_registry_resolves_to_non_eligible_class() {
    // Real-bundle scenario: item.base is a metadata path, registry
    // resolves it to an ineligible class. `can_apply_to` would have
    // passed (no string match against KNOWN_NONELIGIBLE), but
    // `apply()` rejects via the registry-backed check.
    let real_base = "Metadata/Items/Armours/BodyArmours/FourBodyInt3";
    let base_registry = BaseRegistry::from_bases(vec![BaseType {
        id: BaseTypeId::from(real_base),
        name: "Wyrmscale Coat".into(),
        item_class: ItemClassId::from("BodyArmour"),
        attribute_pool: AttributePool::Int,
        drop_level: 65,
        tags: smallvec![TagId::from("body_armour")],
        implicits: smallvec![],
        inventory: InventorySize {
            width: 2,
            height: 3,
        },
        release_state: ReleaseState::Released,
        patch_range: PatchRange::ALL,
    }]);
    let registry = ModRegistry::from_mods(vec![], vec![]);
    let cat = Catalyst::flesh();
    let mut item = item_with_base(real_base);

    // can_apply_to passes — the metadata string doesn't match
    // KNOWN_NONELIGIBLE, so the heuristic is permissive here.
    assert!(cat.can_apply_to(&item).is_ok());

    // apply() rejects via the registry-backed gate.
    let mut rng = Xoshiro256PlusPlus::seed_from_u64(0);
    let mut omens = OmenSet::new();
    let mut ctx =
        poc2_engine::ApplyContext::new(&registry, &base_registry, &mut rng, PATCH, &mut omens);
    match cat.apply(&mut item, &mut ctx) {
        Err(EngineError::InvalidApplication(msg)) => {
            assert!(
                msg.contains("BodyArmour"),
                "expected error mentioning the resolved class; got {msg}"
            );
        }
        other => panic!("expected InvalidApplication; got {other:?}"),
    }
}

#[test]
fn apply_succeeds_on_eligible_class_with_registered_base() {
    // Forward case: registry knows the base resolves to Ring; apply succeeds.
    let real_base = "Metadata/Items/Rings/RingInt5";
    let base_registry = BaseRegistry::from_bases(vec![BaseType {
        id: BaseTypeId::from(real_base),
        name: "Lapis Amulet".into(),
        item_class: ItemClassId::from("Ring"),
        attribute_pool: AttributePool::Int,
        drop_level: 60,
        tags: smallvec![TagId::from("ring")],
        implicits: smallvec![],
        inventory: InventorySize {
            width: 1,
            height: 1,
        },
        release_state: ReleaseState::Released,
        patch_range: PatchRange::ALL,
    }]);
    let registry = ModRegistry::from_mods(vec![], vec![]);
    let cat = Catalyst::flesh();
    let mut item = item_with_base(real_base);

    let mut rng = Xoshiro256PlusPlus::seed_from_u64(0);
    let mut omens = OmenSet::new();
    let mut ctx =
        poc2_engine::ApplyContext::new(&registry, &base_registry, &mut rng, PATCH, &mut omens);
    cat.apply(&mut item, &mut ctx)
        .expect("Ring should accept catalyst");
    assert_eq!(item.quality_kind, QualityKind::Tagged(TagId::from("life")));
}

#[test]
fn can_apply_to_propagates_corrupted_and_mirrored_gates() {
    let cat = Catalyst::flesh();
    let mut item = item_with_base("Ring");
    item.corrupted = true;
    assert!(matches!(
        cat.can_apply_to(&item),
        Err(CannotApply::Corrupted)
    ));

    let mut item = item_with_base("Ring");
    item.mirrored = true;
    assert!(matches!(
        cat.can_apply_to(&item),
        Err(CannotApply::Mirrored)
    ));
}
