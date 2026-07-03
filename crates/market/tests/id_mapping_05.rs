//! Cross-checks `default_id_mapping`'s 0.5 additions (Verisium Alloys +
//! Distilled Emotions) against the SHIPPED bundle's catalogues, so a
//! renamed/added currency upstream fails loudly here instead of silently
//! losing its price mapping.
//!
//! Skips (passing) when no bundle is present.

use poc2_market::prices::default_id_mapping;

fn bundle_path() -> Option<std::path::PathBuf> {
    if let Ok(p) = std::env::var("POC2_BUNDLE") {
        let pb = std::path::PathBuf::from(p);
        return pb.exists().then_some(pb);
    }
    let shipped = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../apps/web/public/poc2.bundle.json.gz");
    shipped.exists().then_some(shipped)
}

/// poe2scout-convention slug: kebab-case display name, apostrophes dropped.
fn slug_of(name: &str) -> String {
    name.to_lowercase()
        .replace('\'', "")
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

#[test]
fn alloys_and_emotions_have_price_mappings() {
    let Some(path) = bundle_path() else {
        eprintln!("skipping: no bundle");
        return;
    };
    let bundle = poc2_data::io::read_bundle(&path).unwrap();
    let mapping = default_id_mapping();

    let mut checked = 0;
    for section in [&bundle.alloys, &bundle.emotions] {
        for entry in &section.entries {
            let (Some(id), Some(name)) = (
                entry.get("id").and_then(|v| v.as_str()),
                entry.get("name").and_then(|v| v.as_str()),
            ) else {
                continue;
            };
            let slug = slug_of(name);
            let mapped = mapping.get(&slug).unwrap_or_else(|| {
                panic!("no default_id_mapping entry for {name:?} (expected slug {slug:?})")
            });
            assert_eq!(
                mapped.as_str(),
                id,
                "mapping for slug {slug:?} points at {mapped:?}, bundle id is {id:?}"
            );
            checked += 1;
        }
    }
    // 13 alloys + 26 emotions in the 0.5 bundle.
    assert!(
        checked >= 39,
        "expected >= 39 catalogue entries, got {checked}"
    );
}
