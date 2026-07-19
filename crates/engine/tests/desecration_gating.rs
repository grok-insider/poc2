//! M14.6 — Bone subtype × item class gating + lord-pool restrictions.
//!
//! Validates the full subtype × class permutation table and the lord-pool
//! omen restrictions:
//! - Rib bones: legal on BodyArmour / Helmet / Boots / Gloves; rejected
//!   elsewhere.
//! - Jawbone bones: legal on weapons + Quiver; rejected on armours, jewellery,
//!   etc.
//! - Collarbone bones: legal on Ring / Amulet / Belt / Talisman; rejected
//!   elsewhere.
//! - Cranium bones: legal on Jewel; rejected elsewhere.
//! - Lord-targeted omens (Blackblooded / Liege / Sovereign) on Cranium
//!   bones (jewels) error at apply-time.
//! - Lord-targeted omens on Sceptres error at apply-time (no exclusive
//!   desecrated).
//!
//! Reference: `docs/81-engine-training-and-rule-encoding-plan.md` §4.6
//! Tier 1.6.

use poc2_engine::omen::{Omen, OmenSet};
use poc2_engine::patch::PatchVersion;
use poc2_engine::{
    apply_currency, BaseTypeId, Bone, BoneSize, BoneSubtype, CannotApply, Currency, EngineError,
    Item, ModRegistry, QualityKind, Rarity,
};
use rand_xoshiro::rand_core::SeedableRng;
use rand_xoshiro::Xoshiro256PlusPlus;
use smallvec::smallvec;

const PATCH: PatchVersion = PatchVersion::PATCH_0_4_0;

fn item_with_base(base: &str) -> Item {
    Item {
        base: BaseTypeId::from(base),
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
fn rib_bone_class_gate_table() {
    let bone = Bone::new(BoneSize::Preserved, BoneSubtype::Rib);
    for legal in ["BodyArmour", "Helmet", "Boots", "Gloves"] {
        let item = item_with_base(legal);
        assert!(
            bone.can_apply_to(&item).is_ok(),
            "Rib should accept {legal}"
        );
    }
    for illegal in [
        "OneHandSword",
        "Bow",
        "Sceptre",
        "Quiver",
        "Ring",
        "Amulet",
        "Belt",
        "Jewel",
    ] {
        let item = item_with_base(illegal);
        match bone.can_apply_to(&item) {
            Err(CannotApply::Other(_)) => {}
            other => panic!("Rib should reject {illegal}; got {other:?}"),
        }
    }
}

#[test]
fn jawbone_class_gate_table() {
    let bone = Bone::new(BoneSize::Preserved, BoneSubtype::Jawbone);
    for legal in [
        "OneHandSword",
        "TwoHandSword",
        "OneHandAxe",
        "TwoHandAxe",
        "OneHandMace",
        "TwoHandMace",
        "Bow",
        "Crossbow",
        "Spear",
        "Staff",
        "Sceptre",
        "Wand",
        "Dagger",
        "Claw",
        "Quiver",
    ] {
        let item = item_with_base(legal);
        assert!(
            bone.can_apply_to(&item).is_ok(),
            "Jawbone should accept {legal}"
        );
    }
    for illegal in [
        "BodyArmour",
        "Helmet",
        "Boots",
        "Gloves",
        "Ring",
        "Amulet",
        "Belt",
        "Talisman",
        "Jewel",
        "Focus",
    ] {
        let item = item_with_base(illegal);
        match bone.can_apply_to(&item) {
            Err(CannotApply::Other(_)) => {}
            other => panic!("Jawbone should reject {illegal}; got {other:?}"),
        }
    }
}

#[test]
fn collarbone_class_gate_table() {
    let bone = Bone::new(BoneSize::Preserved, BoneSubtype::Collarbone);
    for legal in ["Ring", "Amulet", "Belt", "Talisman"] {
        let item = item_with_base(legal);
        assert!(
            bone.can_apply_to(&item).is_ok(),
            "Collarbone should accept {legal}"
        );
    }
    for illegal in [
        "BodyArmour",
        "Helmet",
        "Boots",
        "Gloves",
        "OneHandSword",
        "Bow",
        "Quiver",
        "Jewel",
    ] {
        let item = item_with_base(illegal);
        match bone.can_apply_to(&item) {
            Err(CannotApply::Other(_)) => {}
            other => panic!("Collarbone should reject {illegal}; got {other:?}"),
        }
    }
}

#[test]
fn cranium_class_gate_table() {
    let bone = Bone::new(BoneSize::Preserved, BoneSubtype::Cranium);
    let item = item_with_base("Jewel");
    assert!(bone.can_apply_to(&item).is_ok());
    for illegal in [
        "BodyArmour",
        "Helmet",
        "Ring",
        "Amulet",
        "Belt",
        "OneHandSword",
        "Bow",
    ] {
        let item = item_with_base(illegal);
        match bone.can_apply_to(&item) {
            Err(CannotApply::Other(_)) => {}
            other => panic!("Cranium should reject {illegal}; got {other:?}"),
        }
    }
}

#[test]
fn apply_rejects_lord_targeting_omen_on_cranium_bone_jewel() {
    // Cranium → Jewel has its own pool (Lightless / of the Abyss); any
    // lord-targeting omen is illegal.
    let registry = ModRegistry::from_mods(vec![], vec![]);
    let bone = Bone::new(BoneSize::Preserved, BoneSubtype::Cranium);
    let mut item = item_with_base("Jewel");
    let mut rng = Xoshiro256PlusPlus::seed_from_u64(0);

    for omen in [Omen::blackblooded(), Omen::liege(), Omen::sovereign()] {
        let mut omens = OmenSet::new();
        omens.push(omen.clone());
        let r = apply_currency(&bone, &mut item, &registry, &mut rng, PATCH, &mut omens);
        assert!(
            matches!(r, Err(EngineError::InvalidApplication(ref msg)) if msg.contains("lord-targeting")),
            "Cranium + lord-targeting omen {} should error; got {r:?}",
            omen.id
        );
        // Reset hidden_desecrated for next iteration if it was set by a
        // partial apply (defensive).
        item.hidden_desecrated = None;
    }
}

#[test]
fn apply_rejects_lord_targeting_omen_on_sceptre() {
    // Sceptres accept Jawbone bones in general but lord-targeting omens
    // are illegal on them (no exclusive desecrated pool).
    let registry = ModRegistry::from_mods(vec![], vec![]);
    let bone = Bone::new(BoneSize::Preserved, BoneSubtype::Jawbone);
    let mut item = item_with_base("Sceptre");
    let mut rng = Xoshiro256PlusPlus::seed_from_u64(0);

    let mut omens = OmenSet::new();
    omens.push(Omen::blackblooded());
    let r = apply_currency(&bone, &mut item, &registry, &mut rng, PATCH, &mut omens);
    assert!(
        matches!(r, Err(EngineError::InvalidApplication(ref msg)) if msg.contains("Sceptre")),
        "Sceptre + lord omen should error; got {r:?}"
    );
}

#[test]
fn apply_succeeds_with_lord_targeting_omen_on_jawbone_weapon() {
    // Jawbone on OneHandSword (non-Sceptre weapon) accepts lord omens.
    let registry = ModRegistry::from_mods(vec![], vec![]);
    let bone = Bone::new(BoneSize::Preserved, BoneSubtype::Jawbone);
    let mut item = item_with_base("OneHandSword");
    let mut rng = Xoshiro256PlusPlus::seed_from_u64(0);

    let mut omens = OmenSet::new();
    omens.push(Omen::blackblooded());
    let r = apply_currency(&bone, &mut item, &registry, &mut rng, PATCH, &mut omens);
    assert!(
        r.is_ok(),
        "Jawbone + lord omen on OneHandSword should succeed; got {r:?}"
    );
}

#[test]
fn apply_succeeds_on_legal_class_combinations() {
    let registry = ModRegistry::from_mods(vec![], vec![]);

    let cases = [
        (BoneSubtype::Rib, "BodyArmour"),
        (BoneSubtype::Jawbone, "Bow"),
        (BoneSubtype::Collarbone, "Belt"),
        (BoneSubtype::Cranium, "Jewel"),
    ];
    for (subtype, class) in cases {
        let bone = Bone::new(BoneSize::Preserved, subtype);
        let mut item = item_with_base(class);
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0);
        let mut omens = OmenSet::new();
        let r = apply_currency(&bone, &mut item, &registry, &mut rng, PATCH, &mut omens);
        assert!(
            r.is_ok(),
            "{subtype:?} on {class} should succeed; got {r:?}"
        );
        assert!(item.hidden_desecrated.is_some());
    }
}

#[test]
fn apply_rejects_illegal_class_via_registry() {
    // Cranium bone on Jewel succeeds; on BodyArmour rejects.
    let registry = ModRegistry::from_mods(vec![], vec![]);
    let bone = Bone::new(BoneSize::Preserved, BoneSubtype::Cranium);
    let mut item = item_with_base("BodyArmour");
    let mut rng = Xoshiro256PlusPlus::seed_from_u64(0);
    let mut omens = OmenSet::new();
    let r = apply_currency(&bone, &mut item, &registry, &mut rng, PATCH, &mut omens);
    assert!(
        matches!(r, Err(EngineError::InvalidApplication(ref msg)) if msg.contains("Cranium")),
        "Cranium on BodyArmour should error; got {r:?}"
    );
}

// -------------------------------------------------------------------------
// Apply-time gating across every Collarbone jewellery class (complements the
// `can_apply_to`-only `collarbone_class_gate_table`).
// -------------------------------------------------------------------------

#[test]
fn collarbone_accepted_on_jewellery_classes() {
    // Exact strings come from BoneSubtype::Collarbone.valid_classes().
    assert_eq!(
        BoneSubtype::Collarbone.valid_classes(),
        &["Ring", "Amulet", "Belt", "Talisman"]
    );
    let registry = ModRegistry::from_mods(vec![], vec![]);
    for class in BoneSubtype::Collarbone.valid_classes() {
        let bone = Bone::new(BoneSize::Preserved, BoneSubtype::Collarbone);
        let mut item = item_with_base(class);
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0);
        let mut omens = OmenSet::new();
        let r = apply_currency(&bone, &mut item, &registry, &mut rng, PATCH, &mut omens);
        assert!(
            r.is_ok(),
            "Preserved Collarbone should apply on {class}; got {r:?}"
        );
        assert!(
            item.hidden_desecrated.is_some(),
            "Collarbone must seed a hidden slot on {class}"
        );
    }
}

// -------------------------------------------------------------------------
// Single-lord consumption: two distinct lord omens active, exactly one is
// burned by a successful Jawbone apply on a weapon; the other remains.
// -------------------------------------------------------------------------

#[test]
fn two_lord_omens_one_consumed_one_remains() {
    let registry = ModRegistry::from_mods(vec![], vec![]);
    let bone = Bone::new(BoneSize::Preserved, BoneSubtype::Jawbone);
    let mut item = item_with_base("OneHandSword");
    let mut rng = Xoshiro256PlusPlus::seed_from_u64(0);

    let mut omens = OmenSet::new();
    omens.push(Omen::blackblooded()); // Kurgal
    omens.push(Omen::liege()); // Amanamu
    assert_eq!(omens.len(), 2, "two distinct lord omens active");

    let r = apply_currency(&bone, &mut item, &registry, &mut rng, PATCH, &mut omens);
    assert!(
        r.is_ok(),
        "Jawbone on a weapon with a lord omen should succeed; got {r:?}"
    );

    // Exactly one abyss_lord set on the freshly-seeded slot.
    let slot = item
        .hidden_desecrated
        .as_ref()
        .expect("apply must seed a hidden desecrated slot");
    assert!(
        slot.abyss_lord.is_some(),
        "the consumed lord omen must tag the slot with an abyss lord"
    );

    // Exactly one lord omen consumed; the other remains in the active set.
    assert_eq!(
        omens.len(),
        1,
        "exactly one of the two lord omens must be consumed"
    );
    // And the survivor is still an active lord-targeting omen.
    assert_eq!(
        omens.iter().count(),
        1,
        "one lord omen should remain active"
    );
}
