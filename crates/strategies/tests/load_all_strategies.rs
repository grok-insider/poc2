//! Integration test: every TOML file in `strategies/` parses, validates,
//! and registers cleanly. Catches typos and structural bugs in
//! community-authored strategy fixtures before they ship in the bundle.

use poc2_strategies::{load_strategy_toml, StrategyRegistry};

#[test]
fn all_seed_strategies_load() {
    let dir = std::path::Path::new("strategies");
    let entries = std::fs::read_dir(dir).expect("strategies/ exists");
    let mut count = 0_usize;
    let mut loaded = Vec::new();
    for entry in entries {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("toml") {
            continue;
        }
        let s = load_strategy_toml(&path)
            .unwrap_or_else(|e| panic!("strategy {} failed to load: {e}", path.display()));
        assert!(
            !s.id.0.is_empty(),
            "strategy at {} has empty id",
            path.display()
        );
        assert!(
            !s.name.is_empty(),
            "strategy at {} has empty name",
            path.display()
        );
        loaded.push(s);
        count += 1;
    }
    assert!(count >= 3, "expected >= 3 seed strategies, got {count}");

    // Build a registry over all loaded strategies; ensure ids are unique.
    let r = StrategyRegistry::from_strategies(loaded);
    assert_eq!(r.len(), count);
}
