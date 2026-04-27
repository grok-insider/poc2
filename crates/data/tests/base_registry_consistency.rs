//! M14.2 — `BaseRegistry` ↔ bundle consistency test.
//!
//! Asserts that bundles satisfy the cross-references the engine's
//! `BaseRegistry` depends on:
//! - Every `BaseType.item_class` resolves to an entry in
//!   `bundle.item_classes`.
//! - Every `BaseType.id` is unique within `bundle.base_items`.
//! - The constructed `BaseRegistry`'s class lookup matches the
//!   `BaseType`'s `item_class` field for every base.
//!
//! Runs against a synthetic minimal bundle. The pipeline-built bundle
//! validation lands in `pipeline/tests/registry_coverage.rs` (M14.7).
//!
//! Reference: `docs/81-engine-training-and-rule-encoding-plan.md` §4.2
//! Tier 1.2.

use ahash::AHashSet;
use poc2_data::concepts::ConceptMap;
use poc2_data::Bundle;
use poc2_engine::base::{BaseType, InventorySize, ReleaseState};
use poc2_engine::item_class::AttributePool;
use poc2_engine::tag::TagCategory;
use poc2_engine::{
    BaseRegistry, BaseTypeId, ItemClass, ItemClassId, PatchRange, PatchVersion, Tag, TagId,
};
use smallvec::smallvec;

fn mk_bundle_with_bases() -> Bundle {
    let mut b = Bundle::empty(PatchVersion::PATCH_0_4_0, "test@base-registry-consistency");

    let tag_boots = Tag {
        id: TagId::from("boots"),
        category: TagCategory::ItemClass,
        display_name: Some("Boots".into()),
    };
    let tag_belt = Tag {
        id: TagId::from("belt"),
        category: TagCategory::ItemClass,
        display_name: Some("Belt".into()),
    };
    b.tags = vec![tag_boots, tag_belt];

    b.item_classes = vec![
        ItemClass {
            id: ItemClassId::from("Boots"),
            name: "Boots".into(),
            max_implicits: 0,
            max_prefixes: 3,
            max_suffixes: 3,
            max_sockets: 1,
            class_tags: smallvec![TagId::from("boots")],
            patch_range: PatchRange::ALL,
        },
        ItemClass {
            id: ItemClassId::from("Belt"),
            name: "Belt".into(),
            max_implicits: 0,
            max_prefixes: 3,
            max_suffixes: 3,
            max_sockets: 0,
            class_tags: smallvec![TagId::from("belt")],
            patch_range: PatchRange::ALL,
        },
    ];

    b.base_items = vec![
        BaseType {
            id: BaseTypeId::from("Metadata/Items/Armours/Boots/BootsInt5"),
            name: "Wanderer Shoes".into(),
            item_class: ItemClassId::from("Boots"),
            attribute_pool: AttributePool::Int,
            drop_level: 65,
            tags: smallvec![TagId::from("boots")],
            implicits: smallvec![],
            inventory: InventorySize {
                width: 2,
                height: 2,
            },
            release_state: ReleaseState::Released,
            patch_range: PatchRange::ALL,
        },
        BaseType {
            id: BaseTypeId::from("Metadata/Items/Belts/Belt6"),
            name: "Heavy Belt".into(),
            item_class: ItemClassId::from("Belt"),
            attribute_pool: AttributePool::Str,
            drop_level: 60,
            tags: smallvec![TagId::from("belt")],
            implicits: smallvec![],
            inventory: InventorySize {
                width: 2,
                height: 1,
            },
            release_state: ReleaseState::Released,
            patch_range: PatchRange::ALL,
        },
    ];

    b.concept_map = ConceptMap::default();
    b
}

#[test]
fn every_base_item_class_exists_in_bundle_item_classes() {
    let bundle = mk_bundle_with_bases();
    let class_ids: AHashSet<&ItemClassId> = bundle.item_classes.iter().map(|c| &c.id).collect();
    for base in &bundle.base_items {
        assert!(
            class_ids.contains(&base.item_class),
            "base {} references unknown item_class {}",
            base.id,
            base.item_class
        );
    }
}

#[test]
fn base_ids_are_unique_within_bundle() {
    let bundle = mk_bundle_with_bases();
    let mut seen = AHashSet::new();
    for base in &bundle.base_items {
        assert!(
            seen.insert(base.id.clone()),
            "duplicate base id in bundle: {}",
            base.id
        );
    }
}

#[test]
fn base_registry_class_lookup_matches_bundle() {
    let bundle = mk_bundle_with_bases();
    let registry = BaseRegistry::from_bases(bundle.base_items.clone());
    for base in &bundle.base_items {
        assert_eq!(
            registry.class_of(&base.id),
            Some(&base.item_class),
            "registry returned a different class for base {}",
            base.id
        );
    }
}

#[test]
fn base_registry_for_class_groups_bases_correctly() {
    let bundle = mk_bundle_with_bases();
    let registry = BaseRegistry::from_bases(bundle.base_items.clone());
    let boots = registry.for_class(&ItemClassId::from("Boots"));
    assert_eq!(boots.len(), 1);
    assert_eq!(
        boots[0],
        BaseTypeId::from("Metadata/Items/Armours/Boots/BootsInt5")
    );
    let belt = registry.for_class(&ItemClassId::from("Belt"));
    assert_eq!(belt.len(), 1);
    let none = registry.for_class(&ItemClassId::from("Helmet"));
    assert!(none.is_empty());
}
