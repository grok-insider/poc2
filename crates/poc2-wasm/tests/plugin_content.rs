//! Engine boundary test for ADR-0014 phase 1: `setPluginContent`
//! installs plugin-emitted strategy/rule TOMLs with set semantics
//! (seeds + content, idempotent) and warn-and-skips bad documents.

use poc2_engine::patch::PatchVersion;
use poc2_wasm::Engine;

const STRATEGY_TOML: &str = include_str!("../../strategies/strategies/whittling-cleanup.toml");
const RULES_TOML: &str = include_str!("../../rules/seed_rules/00_progression.toml");

fn engine() -> Engine {
    let bundle = poc2_data::Bundle::empty(PatchVersion::PATCH_0_5_0, "plugin-content-test");
    let bytes = serde_json::to_vec(&bundle).unwrap();
    Engine::new(&bytes).unwrap()
}

#[test]
fn set_plugin_content_installs_and_reports() {
    let mut e = engine();
    let strategies = serde_json::to_string(&vec![STRATEGY_TOML]).unwrap();
    let rules = serde_json::to_string(&vec![RULES_TOML]).unwrap();

    let view: serde_json::Value =
        serde_json::from_str(&e.set_plugin_content(&strategies, &rules).unwrap()).unwrap();
    assert_eq!(view["strategies_added"], 1);
    assert!(view["rules_added"].as_u64().unwrap() >= 1);
    assert_eq!(view["errors"].as_array().unwrap().len(), 0);

    // Set semantics: calling again with the same content must not grow
    // the registries (counts stay identical, no duplicate stacking).
    let view2: serde_json::Value =
        serde_json::from_str(&e.set_plugin_content(&strategies, &rules).unwrap()).unwrap();
    assert_eq!(view2["strategies_added"], 1);
    assert_eq!(view["rules_added"], view2["rules_added"]);

    // Reset: empty arrays restore pure seed registries.
    let view3: serde_json::Value =
        serde_json::from_str(&e.set_plugin_content("[]", "[]").unwrap()).unwrap();
    assert_eq!(view3["strategies_added"], 0);
    assert_eq!(view3["rules_added"], 0);
}

#[test]
fn bad_documents_are_skipped_and_reported() {
    let mut e = engine();
    let strategies = serde_json::to_string(&vec!["this is not a strategy".to_string()]).unwrap();
    let rules = serde_json::to_string(&vec![RULES_TOML.to_string()]).unwrap();

    let view: serde_json::Value =
        serde_json::from_str(&e.set_plugin_content(&strategies, &rules).unwrap()).unwrap();
    assert_eq!(view["strategies_added"], 0);
    assert!(view["rules_added"].as_u64().unwrap() >= 1);
    let errors = view["errors"].as_array().unwrap();
    assert_eq!(errors.len(), 1);
    assert!(errors[0].as_str().unwrap().starts_with("strategy[0]"));
}
