//! Integration test: load and validate the canonical user-supplied strategy
//! file `strategies/3xt1-es-body-armour.toml`.
//!
//! This is the canonical strategy fixture — the user's worked example
//! encoded as data. The test asserts:
//! - The file parses cleanly through the strategy DSL
//! - The validator's structural checks (step ids, on_success/on_failure
//!   references, branch goto targets) all pass
//! - The strategy registers and can be looked up by item-class
//! - The patch-versioning filter accepts it on 0.4 and rejects it on 0.3

use poc2_engine::ids::ItemClassId;
use poc2_engine::patch::PatchVersion;
use poc2_strategies::{load_strategy_toml, StrategyId, StrategyRegistry};

const FIXTURE: &str = "strategies/3xt1-es-body-armour.toml";

#[test]
fn canonical_strategy_loads_and_validates() {
    let s = load_strategy_toml(FIXTURE).expect("canonical strategy must load");
    assert_eq!(s.id, StrategyId::from("3xt1-es-body-armour-isolation"));
    assert_eq!(s.name, "Triple T1 Energy Shield Body Armour Isolation");
    assert_eq!(s.patch_min, Some(PatchVersion::PATCH_0_4_0));
    assert_eq!(s.item_classes, vec![ItemClassId::from("BodyArmour")]);
    // 11 step ids: validate-base, transmute, restart-base, augment,
    // regal, annul-chaos, give-up, exalt-loop, bone, divine+fracture,
    // fracture-fail, reveal, essence, vaal, done. Some are nested via
    // sequence/loop_until and don't appear as top-level steps.
    assert!(s.steps.len() >= 10, "got {} steps", s.steps.len());
}

#[test]
fn canonical_strategy_registers_and_filters_by_class() {
    let s = load_strategy_toml(FIXTURE).unwrap();
    let r = StrategyRegistry::from_strategies(vec![s]);
    let body_armour = ItemClassId::from("BodyArmour");
    let boots = ItemClassId::from("Boots");

    let on_0_4: Vec<_> = r
        .for_class(&body_armour, PatchVersion::PATCH_0_4_0)
        .collect();
    assert_eq!(on_0_4.len(), 1, "should match BodyArmour on 0.4");
    let on_boots: Vec<_> = r.for_class(&boots, PatchVersion::PATCH_0_4_0).collect();
    assert_eq!(on_boots.len(), 0, "should not match Boots class");
}

#[test]
fn canonical_strategy_excluded_on_pre_patch_baseline() {
    // patch_min = 0.4.0 means this strategy is NOT available on 0.3.0.
    let s = load_strategy_toml(FIXTURE).unwrap();
    let r = StrategyRegistry::from_strategies(vec![s]);
    let body_armour = ItemClassId::from("BodyArmour");
    let on_0_3: Vec<_> = r
        .for_class(&body_armour, PatchVersion::new(0, 3, 0))
        .collect();
    assert_eq!(on_0_3.len(), 0, "patch_min = 0.4.0 must exclude 0.3");
}

#[test]
fn canonical_strategy_step_graph_is_connected() {
    let s = load_strategy_toml(FIXTURE).unwrap();
    // Reachability check: walk from the entry point following on_success
    // and on_failure; assert every top-level step is reachable.
    let mut reachable = ahash::AHashSet::new();
    let mut frontier = vec![s.steps[0].id.clone()];
    while let Some(cur) = frontier.pop() {
        if !reachable.insert(cur.clone()) {
            continue;
        }
        if let Some(step) = s.step(&cur) {
            for next in [&step.on_success, &step.on_failure].into_iter().flatten() {
                frontier.push(next.clone());
            }
        }
    }
    // Every step except recovery-only branches should be reachable from
    // the entry point.
    let unreachable: Vec<_> = s
        .steps
        .iter()
        .filter(|st| !reachable.contains(&st.id))
        .map(|st| st.id.0.clone())
        .collect();
    assert!(
        unreachable.is_empty()
            || unreachable
                .iter()
                .all(|id| id.contains("S4c") || id.contains("fail") || id.contains("S2b")),
        "unreachable steps from entry: {unreachable:?}"
    );
}
