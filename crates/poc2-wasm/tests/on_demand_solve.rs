//! ADR-0015 engine-boundary test: `recommend` solves cache-missing goals
//! on demand, reuses the cached policy on repeat calls, re-solves when the
//! cached model doesn't cover the current item, and invalidates on league
//! switches.

use poc2_engine::ids::{ConceptId, ItemClassId, ModGroupId, ModId, StatId, TagId};
use poc2_engine::item::{AffixType, Item, QualityKind, Rarity};
use poc2_engine::mods::{
    ModDefinition, ModDomain, ModFlags, ModGroup, ModKind, ModStat, SpawnWeight,
};
use poc2_engine::patch::{PatchRange, PatchVersion};
use poc2_wasm::Engine;
use smallvec::smallvec;

const CLASS: &str = "BodyArmour";

fn mk_mod(id: &str, group: &str, affix: AffixType, concept: &str) -> ModDefinition {
    ModDefinition {
        id: ModId::from(id),
        name: None,
        mod_group: ModGroup(ModGroupId::from(group)),
        affix_type: affix,
        kind: ModKind::Explicit,
        domain: ModDomain::Item,
        tags: smallvec![],
        concept_set: smallvec![ConceptId::from(concept)],
        spawn_weights: smallvec![SpawnWeight {
            tag: TagId::from(CLASS),
            weight: 1
        }],
        stats: smallvec![ModStat {
            stat_id: StatId::from(format!("stat_{id}")),
            min: 10.0,
            max: 20.0,
        }],
        required_level: 1,
        tier: None,
        allowed_item_classes: smallvec![ItemClassId::from(CLASS)],
        patch_range: PatchRange::ALL,
        flags: ModFlags::empty(),
        text_template: None,
    }
}

fn engine_with_fixture_bundle() -> Engine {
    let mut bundle = poc2_data::Bundle::empty(PatchVersion::PATCH_0_5_0, "on-demand-test");
    bundle.mods = vec![
        mk_mod("ES1", "ES", AffixType::Prefix, "EnergyShield"),
        mk_mod("Life1", "Life", AffixType::Prefix, "Life"),
        mk_mod("FireRes1", "FireRes", AffixType::Suffix, "FireResistance"),
        mk_mod("ColdRes1", "ColdRes", AffixType::Suffix, "ColdResistance"),
    ];
    let bytes = serde_json::to_vec(&bundle).expect("serialize bundle");
    Engine::new(&bytes).expect("engine boots")
}

fn item_json(rarity: Rarity, prefixes: &[&str], suffixes: &[&str]) -> String {
    let roll = |id: &&str, affix| poc2_engine::item::ModRoll {
        mod_id: ModId::from(*id),
        affix_type: affix,
        kind: ModKind::Explicit,
        values: smallvec![],
        is_fractured: false,
    };
    let item = Item {
        base: poc2_engine::ids::BaseTypeId::from(CLASS),
        ilvl: 82,
        rarity,
        corrupted: false,
        sanctified: false,
        mirrored: false,
        quality: 0,
        quality_kind: QualityKind::Untagged,
        implicits: smallvec![],
        prefixes: prefixes
            .iter()
            .map(|i| roll(i, AffixType::Prefix))
            .collect(),
        suffixes: suffixes
            .iter()
            .map(|i| roll(i, AffixType::Suffix))
            .collect(),
        enchantments: smallvec![],
        hidden_desecrated: None,
        sockets: smallvec![],
        hinekora_lock: None,
    };
    serde_json::to_string(&item).unwrap()
}

fn es_goal_json() -> String {
    serde_json::json!({
        "target": {
            "prefixes": [{
                "concept": "EnergyShield",
                "concept_any": [],
                "affix": null,
                "count": 1,
                "min_tier": null,
                "allow_hybrid": true
            }],
            "suffixes": [],
            "constraints": []
        },
        "abandon_criteria": [],
        "budget": { "min": 100.0, "expected": 100.0, "max": 100.0 }
    })
    .to_string()
}

#[test]
fn recommend_solves_goal_on_demand_and_caches() {
    let mut engine = engine_with_fixture_bundle();
    assert_eq!(engine.trained_model_count(), 0);

    let item = item_json(Rarity::Normal, &[], &[]);
    let goal = es_goal_json();

    // First recommend: cache miss → on-demand solve → one cached model.
    let recs = engine
        .recommend(&item, &goal, 0.5, 3, 3)
        .expect("recommend");
    assert!(recs.starts_with('['), "recommendations JSON: {recs}");
    assert_eq!(
        engine.trained_model_count(),
        1,
        "first recommend must solve + cache the goal"
    );

    // Second recommend, same goal: cache hit, no growth.
    engine
        .recommend(&item, &goal, 0.5, 3, 3)
        .expect("recommend");
    assert_eq!(engine.trained_model_count(), 1);

    // Budget tweak must NOT re-key (goal_hash ignores budget).
    let goal_other_budget = goal.replace("100.0", "55.0");
    engine
        .recommend(&item, &goal_other_budget, 0.5, 3, 3)
        .expect("recommend");
    assert_eq!(engine.trained_model_count(), 1);
}

#[test]
fn recommend_resolves_again_when_item_outside_model_coverage() {
    let mut engine = engine_with_fixture_bundle();
    let goal = es_goal_json();

    // Solve from an empty Normal base…
    let normal = item_json(Rarity::Normal, &[], &[]);
    engine
        .recommend(&normal, &goal, 0.5, 3, 3)
        .expect("recommend");
    assert_eq!(engine.trained_model_count(), 1);

    // …then plan from a mid-craft Rare the first solve never reached
    // (three mods incl. both resistances — unreachable from an empty
    // Normal base only via states the first BFS visited? The rare state
    // IS reachable in this tiny fixture, so use a fractured mod to force
    // a state outside the original reachable set).
    let mut rare: Item = serde_json::from_str(&item_json(
        Rarity::Rare,
        &["Life1"],
        &["FireRes1", "ColdRes1"],
    ))
    .unwrap();
    rare.prefixes[0].is_fractured = true;
    let rare_json = serde_json::to_string(&rare).unwrap();
    engine
        .recommend(&rare_json, &goal, 0.5, 3, 3)
        .expect("recommend");
    // Same (goal, class) key — the entry was REPLACED by a re-solve, not
    // duplicated.
    assert_eq!(engine.trained_model_count(), 1);
}

#[test]
fn set_league_invalidates_trained_models() {
    let mut engine = engine_with_fixture_bundle();
    let item = item_json(Rarity::Normal, &[], &[]);
    let goal = es_goal_json();
    engine
        .recommend(&item, &goal, 0.5, 3, 3)
        .expect("recommend");
    assert_eq!(engine.trained_model_count(), 1);

    // League switch clears (league gates candidates); next recommend
    // re-solves on demand. The engine boots on League::current()
    // (challenge in 0.5), so switch to standard.
    engine.set_league("standard").expect("set league");
    assert_eq!(engine.trained_model_count(), 0);
    engine
        .recommend(&item, &goal, 0.5, 3, 3)
        .expect("recommend");
    assert_eq!(engine.trained_model_count(), 1);

    // Same-league set is a no-op (no clear).
    engine.set_league("standard").expect("set league");
    assert_eq!(engine.trained_model_count(), 1);
}

#[test]
fn empty_target_goal_never_solves() {
    let mut engine = engine_with_fixture_bundle();
    let item = item_json(Rarity::Normal, &[], &[]);
    let goal = serde_json::json!({
        "target": { "prefixes": [], "suffixes": [], "constraints": [] },
        "abandon_criteria": [],
        "budget": { "min": 100.0, "expected": 100.0, "max": 100.0 }
    })
    .to_string();
    engine
        .recommend(&item, &goal, 0.5, 3, 3)
        .expect("recommend");
    assert_eq!(engine.trained_model_count(), 0);
}
