//! M14.2 — BaseRegistry integration test.
//!
//! Validates that:
//! - The `BaseRegistry` correctly resolves `BaseTypeId → ItemClassId` for
//!   real-bundle ids.
//! - `class_for_item` (via `apply_currency_with_bases`) consults the
//!   registry and produces correct mod sampling for items whose
//!   `Item.base` is a real bundle id.
//! - The legacy `Item.base = "BodyArmour"` placeholder still works through
//!   the fallback path when the registry doesn't recognize the id.
//! - Currency-internal class-aware filtering (Bone/Catalyst gating) lands
//!   in M14.5/M14.6; this file only validates the BaseRegistry plumbing.
//!
//! Reference: `docs/81-engine-training-and-rule-encoding-plan.md` §4.2
//! Tier 1.2.

use poc2_engine::base::{BaseType, InventorySize, ReleaseState};
use poc2_engine::currency::basic::OrbOfAugmentation;
use poc2_engine::ids::TagId;
use poc2_engine::item_class::AttributePool;
use poc2_engine::omen::OmenSet;
use poc2_engine::patch::PatchVersion;
use poc2_engine::weights::{Confidence, WeightObservation, WeightScope};
use poc2_engine::{
    apply_currency_with_bases, AffixType, BaseRegistry, BaseTypeId, Item, ItemClassId,
    ModDefinition, ModDomain, ModFlags, ModGroup, ModGroupId, ModId, ModKind, ModRegistry, ModRoll,
    PatchRange, QualityKind, Rarity, SpawnWeight,
};
use rand_xoshiro::rand_core::SeedableRng;
use rand_xoshiro::Xoshiro256PlusPlus;
use smallvec::smallvec;

const PATCH: PatchVersion = PatchVersion::PATCH_0_4_0;
const REAL_BASE_ID: &str = "Metadata/Items/Armours/BodyArmours/FourBodyInt3";

fn mk_real_base() -> BaseType {
    BaseType {
        id: BaseTypeId::from(REAL_BASE_ID),
        name: "Wyrmscale Coat".to_string(),
        item_class: ItemClassId::from("BodyArmour"),
        attribute_pool: AttributePool::Int,
        drop_level: 65,
        tags: smallvec![
            TagId::from("body_armour"),
            TagId::from("int_armour"),
            TagId::from("armour"),
        ],
        implicits: smallvec![],
        inventory: InventorySize {
            width: 2,
            height: 3,
        },
        release_state: ReleaseState::Released,
        patch_range: PatchRange::ALL,
    }
}

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

fn mk_suffix_filler() -> ModDefinition {
    let mut m = mk_prefix_mod("Filler", "FillerGroup");
    m.affix_type = AffixType::Suffix;
    m
}

fn mk_magic_item_with_filler_suffix(base: BaseTypeId) -> Item {
    let mut item = Item {
        base,
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
fn base_registry_resolves_class_from_real_bundle_id() {
    let base_registry = BaseRegistry::from_bases(vec![mk_real_base()]);
    assert_eq!(
        base_registry.class_of(&BaseTypeId::from(REAL_BASE_ID)),
        Some(&ItemClassId::from("BodyArmour"))
    );
    assert!(base_registry
        .tags_of(&BaseTypeId::from(REAL_BASE_ID))
        .iter()
        .any(|t| t == &TagId::from("int_armour")));
}

#[test]
fn apply_currency_with_real_base_id_routes_through_base_registry() {
    // With a real bundle BaseTypeId in `Item.base`, the registry resolves
    // its class and `sample_eligible_mod` finds the mod's allowed
    // BodyArmour entry.
    let base_registry = BaseRegistry::from_bases(vec![mk_real_base()]);
    let registry = ModRegistry::from_mods(
        vec![
            mk_prefix_mod("ModA", "GroupA"),
            mk_prefix_mod("ModB", "GroupB"),
            mk_suffix_filler(),
        ],
        // Per-base weights keyed on the real bundle id.
        vec![
            WeightObservation {
                mod_id: ModId::from("ModA"),
                scope: WeightScope::Base {
                    base: BaseTypeId::from(REAL_BASE_ID),
                },
                primary_weight: 1000.0,
                secondary_weight: None,
                confidence: Confidence::Community,
                note: None,
            },
            WeightObservation {
                mod_id: ModId::from("ModB"),
                scope: WeightScope::Base {
                    base: BaseTypeId::from(REAL_BASE_ID),
                },
                primary_weight: 100.0,
                secondary_weight: None,
                confidence: Confidence::Community,
                note: None,
            },
        ],
    );

    let mut count_a = 0usize;
    let mut count_b = 0usize;
    let trials = 5_000usize;
    for trial in 0..trials {
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0x_BA5E_5EED ^ trial as u64);
        let mut omens = OmenSet::new();
        let mut item = mk_magic_item_with_filler_suffix(BaseTypeId::from(REAL_BASE_ID));
        apply_currency_with_bases(
            &OrbOfAugmentation::new(),
            &mut item,
            &registry,
            &base_registry,
            &mut rng,
            PATCH,
            &mut omens,
        )
        .expect("aug must succeed when registry resolves the class correctly");
        match item.prefixes[0].mod_id.as_str() {
            "ModA" => count_a += 1,
            "ModB" => count_b += 1,
            other => panic!("trial {trial}: unexpected mod id {other}"),
        }
    }

    let p_b = count_b as f64 / trials as f64;
    let expected = 100.0 / 1100.0;
    let stderr = (expected * (1.0 - expected) / trials as f64).sqrt();
    let tol = 4.0 * stderr;
    assert!(
        (p_b - expected).abs() <= tol,
        "ModB sampled at {p_b:.4}; expected {expected:.4} ± {tol:.4} \
         (counts: A={count_a}, B={count_b})"
    );
}

#[test]
fn legacy_class_id_placeholder_falls_back_when_base_registry_is_empty() {
    // `Item.base = "BodyArmour"` (class-id placeholder) — the registry
    // does not recognize this id, so `class_for_item` falls back to
    // `ItemClassId::from(item.base.as_str())`. Augmentation still works.
    let base_registry = BaseRegistry::default();
    let registry = ModRegistry::from_mods(
        vec![mk_prefix_mod("ModA", "GroupA"), mk_suffix_filler()],
        vec![],
    );

    let mut rng = Xoshiro256PlusPlus::seed_from_u64(1);
    let mut omens = OmenSet::new();
    let mut item = mk_magic_item_with_filler_suffix(BaseTypeId::from("BodyArmour"));
    apply_currency_with_bases(
        &OrbOfAugmentation::new(),
        &mut item,
        &registry,
        &base_registry,
        &mut rng,
        PATCH,
        &mut omens,
    )
    .expect("legacy class-id placeholder must continue to work via fallback");
    assert_eq!(item.prefixes.len(), 1);
    assert_eq!(item.prefixes[0].mod_id.as_str(), "ModA");
}

#[test]
fn empty_base_registry_returns_none_class() {
    let r = BaseRegistry::default();
    assert!(r.class_of(&BaseTypeId::from("Anything")).is_none());
    assert_eq!(r.tags_of(&BaseTypeId::from("Anything")), &[][..]);
}
