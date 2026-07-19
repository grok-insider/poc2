//! 0.5 data-gap pool regression (Waystones / Precursor Tablets / Relics /
//! Life+Mana Flasks / Charms / Inscribed Ultimatum): the shipped bundle
//! must carry each surface's craftable mod pool, pinned to its own
//! classes — and the gear pools must stay leak-free (the `default`-tag
//! cross-domain leak this pass fixed put 85 surface mods on Rings).
//!
//! Skips (passing) when no bundle is present.

use poc2_engine::mods::ModDomain;

fn bundle_path() -> Option<std::path::PathBuf> {
    if let Ok(p) = std::env::var("POC2_BUNDLE") {
        let pb = std::path::PathBuf::from(p);
        return pb.exists().then_some(pb);
    }
    let shipped = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../apps/web/public/poc2.bundle.json.gz");
    shipped.exists().then_some(shipped)
}

/// Positive-weight prefix/suffix mods allowed on `class`.
fn pool_count(bundle: &poc2_data::Bundle, class: &str) -> usize {
    bundle
        .mods
        .iter()
        .filter(|m| {
            matches!(
                m.affix_type,
                poc2_engine::AffixType::Prefix | poc2_engine::AffixType::Suffix
            ) && m.allowed_item_classes.iter().any(|c| c.as_str() == class)
                && m.spawn_weights.iter().any(|sw| sw.weight > 0)
        })
        .count()
}

#[test]
fn data_gap_pools_present_and_isolated() {
    let Some(path) = bundle_path() else {
        eprintln!("skipping: no bundle");
        return;
    };
    let bundle = poc2_data::io::read_bundle(&path).unwrap();
    if pool_count(&bundle, "Map") == 0 {
        eprintln!("skipping: bundle predates the data-gap pass");
        return;
    }

    // Expected sizes cross-checked against poe2db (ranges absorb small
    // upstream drift; a big move should be reviewed, not auto-accepted).
    let expect: &[(&str, usize, usize)] = &[
        ("Map", 90, 140),               // poe2db Waystones ~109
        ("TowerAugmentation", 70, 100), // poe2db Precursor Tablets 83
        ("Relic", 110, 170),            // poe2db Relics ~139
        ("LifeFlask", 40, 80),          // poe2db Life Flasks ~57
        ("ManaFlask", 40, 80),
        ("UtilityFlask", 35, 70), // poe2db Charms ~51
        ("UltimatumKey", 25, 40), // poe2db Inscribed Ultimatum 31
    ];
    for (class, lo, hi) in expect {
        let n = pool_count(&bundle, class);
        assert!(
            n >= *lo && n <= *hi,
            "{class} pool = {n}, expected {lo}..={hi}"
        );
    }

    // Waystone bases shipped (16 tiers).
    let waystones = bundle
        .base_items
        .iter()
        .filter(|b| b.item_class.as_str() == "Map")
        .count();
    assert!(
        waystones >= 16,
        "expected >= 16 waystone bases, got {waystones}"
    );

    // Isolation both ways: no surface-domain mod may be allowed on a gear
    // class, and no gear pool may contain surface-domain mods.
    for gear in ["Ring", "BodyArmour", "Boots", "Wand"] {
        let leaked: Vec<_> = bundle
            .mods
            .iter()
            .filter(|m| {
                matches!(m.domain, ModDomain::Map | ModDomain::Misc)
                    && m.allowed_item_classes.iter().any(|c| c.as_str() == gear)
            })
            .map(|m| m.id.as_str().to_string())
            .collect();
        assert!(
            leaked.is_empty(),
            "surface-domain mods leaked into the {gear} pool: {leaked:?}"
        );
    }

    // And the surface pools contain ONLY their own domains (no gear mods
    // riding in via stray tags).
    for (class, dom) in [
        ("Map", ModDomain::Map),
        ("TowerAugmentation", ModDomain::Misc),
    ] {
        let foreign: Vec<_> = bundle
            .mods
            .iter()
            .filter(|m| {
                m.allowed_item_classes.iter().any(|c| c.as_str() == class)
                    && m.spawn_weights.iter().any(|sw| sw.weight > 0)
                    && m.domain != dom
            })
            .map(|m| m.id.as_str().to_string())
            .collect();
        assert!(
            foreign.is_empty(),
            "foreign-domain mods in the {class} pool: {foreign:?}"
        );
    }
}
