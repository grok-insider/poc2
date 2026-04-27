//! Advisor `plan()` performance baseline (M2.9).
//!
//! Per ADR-0007 the advisor must produce a first result in ~200ms and a
//! depth-3 beam in ~2s. We bench the planner's depth-1 fast path and a
//! depth-3 beam search to track regressions. The bench setup uses the
//! seed rule catalogue + the canonical strategy + the conservative
//! valuator defaults — exactly what production runs.
//!
//! Run with `cargo bench --bench advisor_plan -p poc2-advisor`.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use poc2_advisor::{plan, BeamConfig, Goal, PlanInput, ScoringWeights, Stash};
use poc2_engine::currency::DefaultCurrencyResolver;
use poc2_engine::ids::{ConceptId, ItemClassId};
use poc2_engine::item::{Item, QualityKind, Rarity};
use poc2_engine::patch::PatchVersion;
use poc2_engine::registry::ModRegistry;
use poc2_market::{DivEquiv, Valuator};
use poc2_rules::RuleSet;
use poc2_strategies::{load_strategy_str, StrategyRegistry, Target, TargetSpec};
use smallvec::smallvec;

const CANONICAL_STRATEGY: &str =
    include_str!("../../strategies/strategies/3xt1-es-body-armour.toml");

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
                ],
                affix: None,
                count: 2,
                min_tier: Some(1),
                allow_hybrid: true,
            }],
            constraints: vec![],
        },
        abandon_criteria: vec![],
        budget: DivEquiv {
            min: 40.0,
            expected: 100.0,
            max: 200.0,
        },
    }
}

fn bench_plan(c: &mut Criterion) {
    let registry = ModRegistry::from_mods(vec![], vec![]);
    let resolver = DefaultCurrencyResolver::new();
    let rules = RuleSet::from_rules(poc2_rules::seed_rules());
    let strategy = load_strategy_str(CANONICAL_STRATEGY).expect("canonical strategy loads");
    let strategies = StrategyRegistry::from_strategies(vec![strategy]);
    let valuator = Valuator::default();
    let stash = Stash::unlimited();
    let goal = worked_example_goal();
    let item = fresh_body_armour();

    // Depth-1 fast path: rules + strategies, no lookahead, no MC.
    // Target: <1ms.
    c.bench_function("plan_depth_1_top_3", |b| {
        b.iter(|| {
            let input = PlanInput {
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
                    width: 5,
                    depth: 1,
                    risk: 0.5,
                    top_n: 3,
                    seed: 0,
                    mc_samples: 1,
                    weights: ScoringWeights::default(),
                },
            };
            black_box(plan(&input));
        });
    });

    // Depth-3 beam: full lookahead, no MC. Pre-Phase-C.1 baseline.
    c.bench_function("plan_depth_3_top_3", |b| {
        b.iter(|| {
            let input = PlanInput {
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
                    width: 5,
                    depth: 3,
                    risk: 0.5,
                    top_n: 3,
                    seed: 0,
                    mc_samples: 1,
                    weights: ScoringWeights::default(),
                },
            };
            black_box(plan(&input));
        });
    });

    // Depth-3 beam with 50 MC samples per candidate (Phase C.1 default).
    // Target: <5ms per /docs/72-v1-execution-plan.md C.1 budget.
    c.bench_function("plan_depth_3_top_3_mc50", |b| {
        b.iter(|| {
            let input = PlanInput {
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
                    width: 5,
                    depth: 3,
                    risk: 0.5,
                    top_n: 3,
                    seed: 0,
                    mc_samples: 50,
                    weights: ScoringWeights::default(),
                },
            };
            black_box(plan(&input));
        });
    });

    // Depth-5 beam at width 8: stress test. Target: <500ms.
    c.bench_function("plan_depth_5_width_8", |b| {
        b.iter(|| {
            let input = PlanInput {
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
                    width: 8,
                    depth: 5,
                    risk: 0.5,
                    top_n: 5,
                    seed: 0,
                    mc_samples: 1,
                    weights: ScoringWeights::default(),
                },
            };
            black_box(plan(&input));
        });
    });
}

criterion_group!(advisor_plan, bench_plan);
criterion_main!(advisor_plan);
