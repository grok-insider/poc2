//! End-to-end test: build a tiny bundle, serialize, validate, save, load, validate.
//!
//! Exercises the full bundle round-trip pipeline that the data crate is
//! responsible for. Real bundles built from RePoE-fork land in M2.3.

use poc2_data::concepts::{ConceptDefinition, ConceptMap, ConceptMapEntry};
use poc2_data::weights::{Confidence, WeightObservation, WeightScope};
use poc2_data::{io, Bundle};
use poc2_engine::base::{InventorySize, ReleaseState};
use poc2_engine::item::AffixType;
use poc2_engine::item_class::AttributePool;
use poc2_engine::mods::{ModDomain, ModFlags, ModKind, SpawnWeight};
use poc2_engine::tag::TagCategory;
use poc2_engine::{
    BaseType, BaseTypeId, ConceptId, ItemClass, ItemClassId, ModDefinition, ModGroup, ModGroupId,
    ModId, PatchRange, PatchVersion, StatId, Tag, TagId,
};
use smallvec::smallvec;

fn fixture_bundle() -> Bundle {
    let mut b = Bundle::empty(PatchVersion::PATCH_0_4_0, "test@deadbeef");

    // Tags
    let tag_boots = Tag {
        id: TagId::from("boots"),
        category: TagCategory::ItemClass,
        display_name: Some("Boots".into()),
    };
    let tag_int_armour = Tag {
        id: TagId::from("int_armour"),
        category: TagCategory::AttributePool,
        display_name: Some("Int Armour".into()),
    };
    let tag_es = Tag {
        id: TagId::from("energy_shield"),
        category: TagCategory::Resource,
        display_name: Some("Energy Shield".into()),
    };
    let tag_life = Tag {
        id: TagId::from("life"),
        category: TagCategory::Resource,
        display_name: Some("Life".into()),
    };
    b.tags = vec![tag_boots, tag_int_armour, tag_es, tag_life];

    // ItemClass: Boots
    b.item_classes = vec![ItemClass {
        id: ItemClassId::from("Boots"),
        name: "Boots".into(),
        max_implicits: 0,
        max_prefixes: 3,
        max_suffixes: 3,
        max_sockets: 1,
        class_tags: smallvec![TagId::from("boots")],
        patch_range: PatchRange::ALL,
    }];

    // BaseType: a generic int boot
    b.base_items = vec![BaseType {
        id: BaseTypeId::from("Metadata/Items/Armours/Boots/BootsInt5"),
        name: "Wanderer Shoes".into(),
        item_class: ItemClassId::from("Boots"),
        attribute_pool: AttributePool::Int,
        drop_level: 75,
        tags: smallvec![TagId::from("boots"), TagId::from("int_armour")],
        implicits: smallvec![],
        inventory: InventorySize {
            width: 2,
            height: 2,
        },
        release_state: ReleaseState::Released,
        patch_range: PatchRange::ALL,
    }];

    // Concepts
    b.concepts = vec![
        ConceptDefinition {
            id: ConceptId::from("EnergyShield"),
            display_name: "Energy Shield".into(),
            family: "Defence".into(),
        },
        ConceptDefinition {
            id: ConceptId::from("Life"),
            display_name: "Life".into(),
            family: "Resource".into(),
        },
    ];

    // Concept map
    b.concept_map = ConceptMap(vec![
        ConceptMapEntry {
            stat_id: StatId::from("local_energy_shield_+%"),
            concept_id: ConceptId::from("EnergyShield"),
        },
        ConceptMapEntry {
            stat_id: StatId::from("base_maximum_energy_shield"),
            concept_id: ConceptId::from("EnergyShield"),
        },
        ConceptMapEntry {
            stat_id: StatId::from("base_maximum_life"),
            concept_id: ConceptId::from("Life"),
        },
    ]);

    // ModDefinition: hybrid ES + Life prefix (the user's worked-example mod kind)
    b.mods = vec![ModDefinition {
        id: ModId::from("LocalIncreasedEnergyShieldAndLife1"),
        name: Some("Monk's".into()),
        mod_group: ModGroup(ModGroupId::from("BaseLocalDefencesAndLife")),
        affix_type: AffixType::Prefix,
        kind: ModKind::Explicit,
        domain: ModDomain::Item,
        tags: smallvec![TagId::from("energy_shield"), TagId::from("life"),],
        concept_set: smallvec![ConceptId::from("EnergyShield"), ConceptId::from("Life"),],
        spawn_weights: smallvec![
            SpawnWeight {
                tag: TagId::from("boots"),
                weight: SpawnWeight::EXCLUDED,
            },
            SpawnWeight {
                tag: TagId::from("int_armour"),
                weight: SpawnWeight::ELIGIBLE,
            },
        ],
        stats: smallvec![],
        required_level: 8,
        tier: Some(2),
        allowed_item_classes: smallvec![ItemClassId::from("Boots")],
        patch_range: PatchRange::ALL,
        flags: ModFlags::HYBRID,
        text_template: Some("(6-13)% increased Energy Shield\n+(7-10) to maximum Life".into()),
    }];

    // Weight observation
    b.weights = vec![WeightObservation {
        mod_id: ModId::from("LocalIncreasedEnergyShieldAndLife1"),
        scope: WeightScope::ItemClass {
            item_class: ItemClassId::from("Boots"),
        },
        primary_weight: 100.0,
        secondary_weight: Some(95.0),
        confidence: Confidence::Verified,
        note: Some("test fixture".into()),
    }];

    // mods_by_base
    b.mods_by_base = indexmap::IndexMap::from([(
        "Metadata/Items/Armours/Boots/BootsInt5".to_string(),
        vec!["LocalIncreasedEnergyShieldAndLife1".to_string()],
    )]);

    b
}

#[test]
fn fixture_bundle_validates() {
    let b = fixture_bundle();
    b.validate().expect("fixture should validate");
}

#[test]
fn fixture_round_trips_via_json_string() {
    let b = fixture_bundle();
    let s = serde_json::to_string(&b).unwrap();
    let back: Bundle = serde_json::from_str(&s).unwrap();
    back.validate()
        .expect("round-tripped fixture should validate");
    assert_eq!(back.mods.len(), b.mods.len());
    assert_eq!(back.base_items.len(), b.base_items.len());
}

#[test]
fn fixture_round_trips_via_plain_json_file() {
    let b = fixture_bundle();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("poc2.bundle.json");
    io::write_bundle(&b, &path, true).unwrap();
    let back = io::read_bundle(&path).unwrap();
    back.validate().unwrap();
    assert_eq!(back.mods[0].id, b.mods[0].id);
}

#[test]
#[cfg(feature = "gzip")]
fn fixture_round_trips_via_gzipped_json_file() {
    let b = fixture_bundle();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("poc2.bundle.json.gz");
    io::write_bundle(&b, &path, false).unwrap();
    let back = io::read_bundle(&path).unwrap();
    back.validate().unwrap();
    assert_eq!(back.mods[0].id, b.mods[0].id);

    // Sanity: gzipped should be smaller than plain.
    let plain_path = dir.path().join("poc2.bundle.json");
    io::write_bundle(&b, &plain_path, false).unwrap();
    let plain_size = std::fs::metadata(&plain_path).unwrap().len();
    let gz_size = std::fs::metadata(&path).unwrap().len();
    assert!(
        gz_size < plain_size,
        "expected gz < plain ({gz_size} < {plain_size})"
    );
}

#[test]
fn dangling_reference_is_caught() {
    let mut b = fixture_bundle();
    // Reference a non-existent item class on a mod.
    b.mods[0]
        .allowed_item_classes
        .push(ItemClassId::from("NonExistentClass"));
    let err = b.validate().unwrap_err();
    let s = format!("{err}");
    assert!(s.contains("ItemClass"), "got: {s}");
    assert!(s.contains("NonExistentClass"), "got: {s}");
}

#[test]
fn duplicate_mod_id_is_caught() {
    let mut b = fixture_bundle();
    let dup = b.mods[0].clone();
    b.mods.push(dup);
    let err = b.validate().unwrap_err();
    assert!(format!("{err}").contains("duplicate"));
}

#[test]
fn invalid_patch_range_is_caught() {
    let mut b = fixture_bundle();
    b.mods[0].patch_range = PatchRange {
        min: Some(PatchVersion::PATCH_0_5_0),
        max: Some(PatchVersion::PATCH_0_4_0),
    };
    let err = b.validate().unwrap_err();
    assert!(format!("{err}").contains("patch range"));
}

// -------------------------------------------------------------------------
// §5.3 Verisium Alloy wiring — `alloys` BundleSection + alloy_catalogue() +
// resolver seeding. The alloys section is `#[serde(default)]`, so pre-0.5
// bundles (no `alloys` key) must still load.
// -------------------------------------------------------------------------

use poc2_data::BUNDLE_SCHEMA_VERSION;
use poc2_engine::ids::CurrencyId;
use poc2_engine::{CurrencyResolver, DefaultCurrencyResolver};

/// A single well-formed alloy entry: `{ id, name, engine_mod_id }`.
fn runic_ward_alloy_entry() -> serde_json::Value {
    serde_json::json!({
        "id": "AlloyRunicWard",
        "name": "Verisium Alloy of Runic Ward",
        "engine_mod_id": "RunicWardCrafted",
    })
}

#[test]
fn bundle_alloys_section_round_trips() {
    let mut b = Bundle::empty(PatchVersion::PATCH_0_5_0, "test");
    b.alloys.entries.push(runic_ward_alloy_entry());

    let s = serde_json::to_string(&b).unwrap();
    let back: Bundle = serde_json::from_str(&s).unwrap();

    assert_eq!(
        back.alloys.entries.len(),
        1,
        "alloys section should survive the round-trip"
    );
    assert_eq!(back.header.schema_version, BUNDLE_SCHEMA_VERSION);
}

#[test]
fn alloy_catalogue_extracts_typed_alloys() {
    let mut b = Bundle::empty(PatchVersion::PATCH_0_5_0, "test");
    b.alloys.entries.push(runic_ward_alloy_entry());
    // A malformed entry missing both `id` and `engine_mod_id` must be skipped.
    b.alloys
        .entries
        .push(serde_json::json!({ "name": "Headless Alloy" }));

    let catalogue = b.alloy_catalogue();
    assert_eq!(
        catalogue.len(),
        1,
        "only the well-formed entry should be extracted; malformed entries are skipped"
    );
    let alloy = &catalogue[0];
    assert_eq!(alloy.id.as_str(), "AlloyRunicWard");
    assert_eq!(alloy.target_mod, ModId::from("RunicWardCrafted"));
}

#[test]
fn alloy_catalogue_seeds_resolver_and_resolves() {
    let mut b = Bundle::empty(PatchVersion::PATCH_0_5_0, "test");
    b.alloys.entries.push(runic_ward_alloy_entry());

    let catalogue = b.alloy_catalogue();
    let resolver = DefaultCurrencyResolver::new().with_alloys(catalogue);

    assert!(
        resolver
            .resolve(&CurrencyId::from("AlloyRunicWard"))
            .is_some(),
        "seeded alloy id should resolve to a currency trait object"
    );
    assert!(
        resolver
            .resolve(&CurrencyId::from("AlloyNonexistent"))
            .is_none(),
        "unknown alloy id must resolve to None"
    );
}

#[test]
fn empty_bundle_has_empty_alloys() {
    let b = Bundle::empty(PatchVersion::PATCH_0_5_0, "test");
    assert!(
        b.alloy_catalogue().is_empty(),
        "a fresh bundle carries no alloys"
    );

    // The empty bundle round-trips fine.
    let s = serde_json::to_string(&b).unwrap();
    let back: Bundle = serde_json::from_str(&s).unwrap();
    assert!(back.alloy_catalogue().is_empty());

    // Back-compat: a bundle JSON with NO `alloys` key still loads via
    // `#[serde(default)]`. Strip the key from a serialized bundle and confirm
    // it deserializes to an empty alloys section.
    let mut value: serde_json::Value = serde_json::from_str(&s).unwrap();
    value
        .as_object_mut()
        .expect("bundle serializes as a JSON object")
        .remove("alloys");
    assert!(
        value.get("alloys").is_none(),
        "alloys key should have been removed for the back-compat check"
    );
    let legacy: Bundle = serde_json::from_value(value).unwrap();
    assert!(
        legacy.alloy_catalogue().is_empty(),
        "pre-0.5 bundle with no alloys key must default to an empty section"
    );
    assert_eq!(legacy.alloys.entries.len(), 0);
}
