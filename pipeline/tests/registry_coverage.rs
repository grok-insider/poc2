//! Phase E coverage regression for the bundle's mod registry.
//!
//! These tests assert the *minimum* number of desecrated mods that land in
//! `bundle.mods` for each gear class (floors regenerated from poe2db's 0.5
//! per-class tables), and that the retired Vaal-implicit fixture stays
//! empty. The advisor relies on the desecrated pools when planning
//! omen-aware desecrate flows; a regression that drops a pool would
//! silently shrink the recommendation set.
//!
//! ## Why this lives in `pipeline/tests/` (not `crates/data/tests/`)
//!
//! The plan originally placed this test under `crates/data/tests/`. That
//! creates a dep cycle (`poc2-data` → `poc2-pipeline` → `poc2-data`), since
//! the fixture loader lives in the pipeline crate. Hosting it here keeps
//! the same regression coverage without restructuring crate boundaries.

use poc2_data::Bundle;
use poc2_engine::ids::ItemClassId;
use poc2_engine::mods::{ModFlags, ModKind};
use poc2_engine::PatchVersion;

/// Build a bundle that contains only the curated fixtures. RePoE-fork and
/// CoE are skipped — they don't carry desecrated/Vaal data anyway.
fn fixture_only_bundle() -> Bundle {
    let snap = poc2_pipeline::sources::fixtures::load().expect("embedded fixtures must parse");
    let mut bundle = Bundle::empty(PatchVersion::PATCH_0_4_0, "test@registry-coverage");
    poc2_pipeline::normalize::fixtures_to_bundle::normalize_fixtures(&snap, &mut bundle)
        .expect("fixture normalization is infallible");
    bundle
}

/// Sentinel: confirm the bundle still declares the documented schema
/// version after Phase E. Phase E was deliberately additive — no schema
/// bump — so existing v1 bundles on disk continue to load. If a future
/// pass changes the on-disk shape this test will trip and force a
/// migration plan.
#[test]
fn schema_version_matches_loader() {
    let bundle = fixture_only_bundle();
    assert_eq!(
        bundle.header.schema_version,
        poc2_data::BUNDLE_SCHEMA_VERSION
    );
}

#[test]
fn body_armour_has_minimum_desecrated_coverage() {
    let bundle = fixture_only_bundle();
    let class = ItemClassId::from("BodyArmour");
    let count = bundle.count_mods_by_kind_for_class(&class, ModKind::Desecrated);
    assert!(
        count >= 11,
        "BodyArmour desecrated coverage too low: got {count}, want ≥ 11"
    );
}

/// The Vaal-implicit fixture is retired (0.5 "Return of the Ancients"):
/// its 13 VaalImplicit_* entries were fabricated. The real corruption
/// pool (`Corruption*` mods) ships via RePoE-fork, so a fixture-only
/// bundle must contain zero Corrupted-kind mods — anything else means a
/// fabricated implicit re-entered through the fixture path.
#[test]
fn vaal_implicit_fixture_is_retired() {
    let bundle = fixture_only_bundle();
    let count = bundle
        .mods
        .iter()
        .filter(|m| m.kind == ModKind::Corrupted)
        .count();
    assert_eq!(
        count, 0,
        "fixture-only bundle must carry no Corrupted mods, got {count}"
    );
}

/// Coverage floor per gear class, set to the exact distinct-mod counts
/// regenerated from poe2db's 0.5 per-class Desecrated Modifiers tables
/// (equipment pool = the global "Desecrated Mods /196" page; Jewel = the
/// separate ilvl-1 "Lightless" pool). A drop below any floor means the
/// fixture lost real pool entries.
#[test]
fn gear_class_desecrated_coverage_meets_minimums() {
    let bundle = fixture_only_bundle();
    let cases: &[(&str, usize)] = &[
        ("Amulet", 31),
        ("Belt", 21),
        ("BodyArmour", 13),
        ("Boots", 16),
        ("Bow", 17),
        ("Buckler", 14),
        ("Crossbow", 15),
        ("Focus", 18),
        ("Gloves", 18),
        ("Helmet", 17),
        ("Jewel", 44),
        ("OneHandMace", 16),
        ("Quiver", 8),
        ("Ring", 22),
        ("Shield", 20),
        ("Spear", 17),
        ("Staff", 12),
        ("Talisman", 18),
        ("TwoHandMace", 16),
        ("Wand", 15),
        ("Warstaff", 12),
    ];
    for (class_name, expected) in cases {
        let class = ItemClassId::from(*class_name);
        let count = bundle.count_mods_by_kind_for_class(&class, ModKind::Desecrated);
        assert!(
            count >= *expected,
            "{class_name} desecrated coverage too low: got {count}, want ≥ {expected}"
        );
    }
}

/// Every desecrated mod must carry the `DESECRATED_ONLY` flag so the
/// outcome dialog correctly greys them out under non-bone-reveal actions.
#[test]
fn all_desecrated_mods_carry_desecrated_only_flag() {
    let bundle = fixture_only_bundle();
    for m in &bundle.mods {
        if m.kind == ModKind::Desecrated {
            assert!(
                m.flags.contains(ModFlags::DESECRATED_ONLY),
                "{} (Desecrated) missing DESECRATED_ONLY flag",
                m.id.as_str()
            );
        }
    }
}

/// Every Vaal-corruption implicit must carry the `CORRUPTED_ONLY` flag.
#[test]
fn all_vaal_implicits_carry_corrupted_only_flag() {
    let bundle = fixture_only_bundle();
    for m in &bundle.mods {
        if m.kind == ModKind::Corrupted {
            assert!(
                m.flags.contains(ModFlags::CORRUPTED_ONLY),
                "{} (Corrupted) missing CORRUPTED_ONLY flag",
                m.id.as_str()
            );
        }
    }
}

/// Catches the "bundle reuses the same ModId twice" regression the v1
/// validator already enforces, scoped to fixture entries to provide a
/// localized failure message during Phase E development.
#[test]
fn fixture_mod_ids_are_unique() {
    let snap = poc2_pipeline::sources::fixtures::load().expect("embedded fixtures must parse");
    let mut seen: ahash::AHashSet<&str> = ahash::AHashSet::new();
    for entry in &snap.desecrated {
        assert!(
            seen.insert(entry.id.as_str()),
            "duplicate desecrated mod id: {}",
            entry.id
        );
    }
    for entry in &snap.vaal_implicits {
        assert!(
            seen.insert(entry.id.as_str()),
            "duplicate vaal implicit mod id: {}",
            entry.id
        );
    }
}
