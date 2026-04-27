//! Critical "rediscovery" test (per docs/70-roadmap.md M4):
//! given the user's worked-example item state, the advisor must produce
//! either the user's encoded strategy step OR a strictly-better one.
//!
//! For v1, we accept "matches the strategy step OR the seed rule that
//! covers the same step". The user's worked example is encoded in
//! crates/strategies/strategies/3xt1-es-body-armour.toml; the advisor
//! should reach that strategy's first step (Perfect Transmutation) on a
//! fresh Normal ilvl 82 BodyArmour.
//!
//! These tests exercise the full advisor stack end-to-end:
//!
//! - Stash::unlimited (no affordability filtering)
//! - Real rule catalogue (15 seed rules)
//! - Real strategy registry (canonical 3xT1 ES strategy loaded from TOML)
//! - DefaultCurrencyResolver
//! - Valuator with conservative defaults

use poc2_advisor::{plan, AdvisorAction, BeamConfig, Goal, PlanInput, RecommendationSource, Stash};
use poc2_engine::currency::DefaultCurrencyResolver;
use poc2_engine::ids::{ConceptId, ItemClassId};
use poc2_engine::item::{Item, QualityKind, Rarity};
use poc2_engine::patch::PatchVersion;
use poc2_engine::registry::ModRegistry;
use poc2_market::{DivEquiv, Valuator};
use poc2_rules::RuleSet;
use poc2_strategies::{load_strategy_toml, ItemPredicate, StrategyRegistry, Target, TargetSpec};
use smallvec::smallvec;

/// Build the user's worked-example goal: 3 T1 ES prefixes + 2 T1 res
/// suffixes, on an uncorrupted, non-mirrored body armour, with a
/// reasonable budget.
fn worked_example_goal() -> Goal {
    Goal {
        target: Target {
            prefixes: vec![TargetSpec {
                concept: Some(ConceptId::from("EnergyShield")),
                concept_any: vec![],
                affix: None,
                count: 3,
                min_tier: Some(1),
                allow_hybrid: true,
            }],
            suffixes: vec![TargetSpec {
                concept: None,
                concept_any: vec![
                    ConceptId::from("FireResistance"),
                    ConceptId::from("ColdResistance"),
                    ConceptId::from("LightningResistance"),
                    ConceptId::from("AllResistances"),
                ],
                affix: None,
                count: 2,
                min_tier: Some(1),
                allow_hybrid: true,
            }],
            constraints: vec![],
        },
        abandon_criteria: vec![
            ItemPredicate::Corrupted(true),
            ItemPredicate::Sanctified(true),
        ],
        budget: DivEquiv {
            min: 40.0,
            expected: 100.0,
            max: 200.0,
        },
    }
}

/// A fresh Normal ilvl 82 body armour, the starting state of the user's
/// worked example.
fn fresh_body_armour() -> Item {
    Item {
        base: ItemClassId::from("BodyArmour").as_str().into(),
        ilvl: 82,
        rarity: Rarity::Normal,
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
    }
}

/// Locate the canonical strategy fixture relative to this crate.
fn canonical_strategy_path() -> std::path::PathBuf {
    let here = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    here.parent()
        .expect("crates dir")
        .join("strategies")
        .join("strategies")
        .join("3xt1-es-body-armour.toml")
}

#[test]
fn rediscovery_top_recommendation_is_perfect_transmute() {
    // Setup the full advisor stack.
    let registry = ModRegistry::from_mods(vec![], vec![]);
    let resolver = DefaultCurrencyResolver::new();
    let rules = RuleSet::from_rules(poc2_rules::seed_rules());

    let canonical =
        load_strategy_toml(canonical_strategy_path()).expect("canonical strategy loads cleanly");
    let strategies = StrategyRegistry::from_strategies(vec![canonical]);

    let valuator = Valuator::default();
    let stash = Stash::unlimited();
    let goal = worked_example_goal();
    let item = fresh_body_armour();

    let input = PlanInput {
        item,
        goal,
        rules: &rules,
        strategies: &strategies,
        registry: &registry,
        resolver: &resolver,
        valuator: &valuator,
        stash: &stash,
        patch: PatchVersion::PATCH_0_4_0,
        plugin_dispatch: None,
        config: BeamConfig {
            width: 5,
            depth: 1,
            top_n: 5,
            ..BeamConfig::default()
        },
    };

    let recs = plan(&input);
    assert!(!recs.is_empty(), "advisor must produce at least one rec");

    // Per A.5, the rule catalogue grew from ~46 to ~100 rules. Many
    // higher-priority guidance / warning rules can now sit above the
    // R001 Perfect-Transmute action in the rankings (e.g. R304 Tarke
    // bankroll rule fires `Always`). Assert Perfect Transmute appears
    // in the top-N (top_n = 5 here) rather than strictly at index 0.
    let has_transmute = recs.iter().any(|r| {
        matches!(
            &r.action,
            AdvisorAction::ApplyCurrency { currency, .. }
                if currency.as_str() == "PerfectOrbOfTransmutation"
        )
    });
    assert!(
        has_transmute,
        "expected Perfect Transmute to appear in the top recs (matches user's worked example step S2 + rule R001); got {:?}",
        recs.iter()
            .map(|r| (r.action.clone(), r.source.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn rediscovery_recommendation_is_traceable_to_source() {
    let registry = ModRegistry::from_mods(vec![], vec![]);
    let resolver = DefaultCurrencyResolver::new();
    let rules = RuleSet::from_rules(poc2_rules::seed_rules());

    let canonical =
        load_strategy_toml(canonical_strategy_path()).expect("canonical strategy loads");
    let strategies = StrategyRegistry::from_strategies(vec![canonical]);

    let valuator = Valuator::default();
    let stash = Stash::unlimited();

    let input = PlanInput {
        item: fresh_body_armour(),
        goal: worked_example_goal(),
        rules: &rules,
        strategies: &strategies,
        registry: &registry,
        resolver: &resolver,
        valuator: &valuator,
        stash: &stash,
        patch: PatchVersion::PATCH_0_4_0,
        plugin_dispatch: None,
        config: BeamConfig::default(),
    };
    let recs = plan(&input);

    // Every recommendation must cite a source: rule, strategy, or heuristic.
    for r in &recs {
        match &r.source {
            RecommendationSource::Rule { id, .. } => {
                assert!(!id.is_empty(), "rule id must be non-empty");
            }
            RecommendationSource::Strategy { id, step } => {
                assert!(!id.is_empty(), "strategy id must be non-empty");
                assert!(!step.is_empty(), "strategy step must be non-empty");
            }
            RecommendationSource::Heuristic { name } => {
                assert!(!name.is_empty(), "heuristic name must be non-empty");
            }
        }
        assert!(!r.rationale.is_empty(), "rationale must be non-empty");
    }
}

#[test]
fn rediscovery_top_3_includes_strategy_and_rule_sources() {
    let registry = ModRegistry::from_mods(vec![], vec![]);
    let resolver = DefaultCurrencyResolver::new();
    let rules = RuleSet::from_rules(poc2_rules::seed_rules());

    let canonical = load_strategy_toml(canonical_strategy_path()).unwrap();
    let strategies = StrategyRegistry::from_strategies(vec![canonical]);

    let valuator = Valuator::default();
    let stash = Stash::unlimited();

    let input = PlanInput {
        item: fresh_body_armour(),
        goal: worked_example_goal(),
        rules: &rules,
        strategies: &strategies,
        registry: &registry,
        resolver: &resolver,
        valuator: &valuator,
        stash: &stash,
        patch: PatchVersion::PATCH_0_4_0,
        plugin_dispatch: None,
        config: BeamConfig {
            width: 8,
            depth: 1,
            top_n: 3,
            ..BeamConfig::default()
        },
    };
    let recs = plan(&input);
    // We expect at most 3 recommendations.
    assert!(recs.len() <= 3);
    // At least one recommendation should cite the canonical strategy or
    // R001 — the worked-example seed rule.
    let cites_known_source = recs.iter().any(|r| match &r.source {
        RecommendationSource::Strategy { id, .. } => id == "3xt1-es-body-armour-isolation",
        RecommendationSource::Rule { id, .. } => id.starts_with("R001"),
        RecommendationSource::Heuristic { .. } => false,
    });
    assert!(
        cites_known_source,
        "expected recs to cite canonical strategy or R001"
    );
}

#[test]
fn risk_slider_changes_recommendation_score_ordering() {
    // High-budget greedy plan should score higher (cheaper effective cost
    // when risk=1) than the same plan under a cautious risk=0.
    let registry = ModRegistry::from_mods(vec![], vec![]);
    let resolver = DefaultCurrencyResolver::new();
    let rules = RuleSet::from_rules(poc2_rules::seed_rules());

    let canonical = load_strategy_toml(canonical_strategy_path()).unwrap();
    let strategies = StrategyRegistry::from_strategies(vec![canonical]);

    let valuator = Valuator::default();
    let stash = Stash::unlimited();
    let goal = worked_example_goal();
    let item = fresh_body_armour();

    let mk = |risk: f64| PlanInput {
        item: item.clone(),
        goal: goal.clone(),
        rules: &rules,
        strategies: &strategies,
        registry: &registry,
        resolver: &resolver,
        valuator: &valuator,
        stash: &stash,
        patch: PatchVersion::PATCH_0_4_0,
        plugin_dispatch: None,
        config: BeamConfig {
            width: 3,
            depth: 1,
            top_n: 1,
            risk,
            ..BeamConfig::default()
        },
    };

    let cautious = plan(&mk(0.0));
    let greedy = plan(&mk(1.0));
    assert!(!cautious.is_empty());
    assert!(!greedy.is_empty());
    // Risk=1 should produce >= score (cost band collapses to the
    // expected case rather than max).
    assert!(
        greedy[0].score >= cautious[0].score - 1e-9,
        "risk=1 score ({}) should be >= risk=0 score ({})",
        greedy[0].score,
        cautious[0].score
    );
}

#[test]
fn rare_with_3_t1_es_already_satisfies_goal() {
    use poc2_engine::ids::{ModGroupId, ModId, StatId, TagId};
    use poc2_engine::item::{AffixType, ModRoll};
    use poc2_engine::mods::{
        ModDefinition, ModDomain, ModFlags, ModGroup, ModKind, ModStat, SpawnWeight,
    };
    use poc2_engine::patch::PatchRange;

    let mk_es_mod = |id: &str| ModDefinition {
        id: ModId::from(id),
        name: None,
        mod_group: ModGroup(ModGroupId::from(format!("ES-{id}"))),
        affix_type: AffixType::Prefix,
        kind: ModKind::Explicit,
        domain: ModDomain::Item,
        tags: smallvec![],
        concept_set: smallvec![ConceptId::from("EnergyShield")],
        spawn_weights: smallvec![SpawnWeight {
            tag: TagId::from("BodyArmour"),
            weight: 1
        }],
        stats: smallvec![ModStat {
            stat_id: StatId::from("local_energy_shield"),
            min: 50.0,
            max: 80.0
        }],
        required_level: 75,
        allowed_item_classes: smallvec![ItemClassId::from("BodyArmour")],
        patch_range: PatchRange::ALL,
        flags: ModFlags::empty(),
        text_template: None,
    };
    let mk_res_mod = |id: &str, concept: &str| ModDefinition {
        id: ModId::from(id),
        name: None,
        mod_group: ModGroup(ModGroupId::from(format!("Res-{id}"))),
        affix_type: AffixType::Suffix,
        kind: ModKind::Explicit,
        domain: ModDomain::Item,
        tags: smallvec![],
        concept_set: smallvec![ConceptId::from(concept)],
        spawn_weights: smallvec![SpawnWeight {
            tag: TagId::from("BodyArmour"),
            weight: 1
        }],
        stats: smallvec![],
        required_level: 75,
        allowed_item_classes: smallvec![ItemClassId::from("BodyArmour")],
        patch_range: PatchRange::ALL,
        flags: ModFlags::empty(),
        text_template: None,
    };

    let registry = ModRegistry::from_mods(
        vec![
            mk_es_mod("ES1"),
            mk_es_mod("ES2"),
            mk_es_mod("ES3"),
            mk_res_mod("FireRes1", "FireRes"),
            mk_res_mod("ColdRes1", "ColdRes"),
        ],
        vec![],
    );
    let resolver = DefaultCurrencyResolver::new();
    let rules = RuleSet::from_rules(poc2_rules::seed_rules());
    let strategies = StrategyRegistry::default();
    let valuator = Valuator::default();
    let stash = Stash::unlimited();

    // Build the goal — note we look for ConceptId "FireResistance" but
    // the registry uses "FireRes" so the suffix won't match. To keep
    // this simple we use the same concept names registered above.
    let goal = Goal {
        target: Target {
            prefixes: vec![TargetSpec {
                concept: Some(ConceptId::from("EnergyShield")),
                concept_any: vec![],
                affix: None,
                count: 3,
                min_tier: None,
                allow_hybrid: true,
            }],
            suffixes: vec![TargetSpec {
                concept: None,
                concept_any: vec![ConceptId::from("FireRes"), ConceptId::from("ColdRes")],
                affix: None,
                count: 2,
                min_tier: None,
                allow_hybrid: true,
            }],
            constraints: vec![],
        },
        abandon_criteria: vec![],
        budget: DivEquiv::point(100.0),
    };

    let item = Item {
        base: ItemClassId::from("BodyArmour").as_str().into(),
        ilvl: 82,
        rarity: Rarity::Rare,
        corrupted: false,
        sanctified: false,
        mirrored: false,
        quality: 0,
        quality_kind: QualityKind::Untagged,
        implicits: smallvec![],
        prefixes: smallvec![
            ModRoll {
                mod_id: ModId::from("ES1"),
                affix_type: AffixType::Prefix,
                kind: ModKind::Explicit,
                values: smallvec![60.0],
                is_fractured: false,
            },
            ModRoll {
                mod_id: ModId::from("ES2"),
                affix_type: AffixType::Prefix,
                kind: ModKind::Explicit,
                values: smallvec![60.0],
                is_fractured: false,
            },
            ModRoll {
                mod_id: ModId::from("ES3"),
                affix_type: AffixType::Prefix,
                kind: ModKind::Explicit,
                values: smallvec![60.0],
                is_fractured: false,
            },
        ],
        suffixes: smallvec![
            ModRoll {
                mod_id: ModId::from("FireRes1"),
                affix_type: AffixType::Suffix,
                kind: ModKind::Explicit,
                values: smallvec![],
                is_fractured: false,
            },
            ModRoll {
                mod_id: ModId::from("ColdRes1"),
                affix_type: AffixType::Suffix,
                kind: ModKind::Explicit,
                values: smallvec![],
                is_fractured: false,
            },
        ],
        enchantments: smallvec![],
        hidden_desecrated: None,
        sockets: smallvec![],
        hinekora_lock: None,
    };

    let input = PlanInput {
        item,
        goal,
        rules: &rules,
        strategies: &strategies,
        registry: &registry,
        resolver: &resolver,
        valuator: &valuator,
        stash: &stash,
        patch: PatchVersion::PATCH_0_4_0,
        plugin_dispatch: None,
        config: BeamConfig::default(),
    };
    let recs = plan(&input);
    assert_eq!(recs.len(), 1);
    assert!(matches!(recs[0].action, AdvisorAction::Stop));
}
