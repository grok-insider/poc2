//! Item-class resolution for captured bundle items (M14 audit fix).
//!
//! Real captured items carry bundle `BaseTypeId` metadata paths
//! (`"Metadata/Items/Armours/BodyArmours/FourBodyInt3"`) in `Item.base`,
//! while legacy fixtures stuff the class id in directly
//! (`"BodyArmour"`). The candidate generator and class-gated predicates
//! resolve the class through `BaseRegistry::resolve_item_class`, threaded
//! once per node into the `PredicateContext` — so a captured item must
//! surface the same `ItemClass`-gated strategies as its legacy twin.

use poc2_advisor::{
    plan, BeamConfig, Goal, PlanInput, RecommendationSource, ScoringWeights, Stash,
};
use poc2_engine::base::{BaseType, InventorySize, ReleaseState};
use poc2_engine::base_registry::BaseRegistry;
use poc2_engine::currency::DefaultCurrencyResolver;
use poc2_engine::ids::{BaseTypeId, ConceptId, ItemClassId};
use poc2_engine::item::{AffixType, Item, QualityKind, Rarity};
use poc2_engine::item_class::AttributePool;
use poc2_engine::patch::{PatchRange, PatchVersion};
use poc2_engine::registry::ModRegistry;
use poc2_market::{DivEquiv, Valuator};
use poc2_rules::RuleSet;
use poc2_strategies::{load_strategy_str, PredicateContext, StrategyRegistry, TargetSpec};
use smallvec::smallvec;

const BUNDLE_BASE: &str = "Metadata/Items/Armours/BodyArmours/FourBodyInt3";

const ES_BODY_ARMOUR_TOML: &str =
    include_str!("../../../crates/strategies/strategies/3xt1-es-body-armour.toml");

fn normal_item(base: &str) -> Item {
    Item {
        base: BaseTypeId::from(base),
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

fn body_armour_base_registry() -> BaseRegistry {
    BaseRegistry::from_bases(vec![BaseType {
        id: BaseTypeId::from(BUNDLE_BASE),
        name: "Expert Hexer's Robe".into(),
        item_class: ItemClassId::from("BodyArmour"),
        attribute_pool: AttributePool::Int,
        drop_level: 65,
        tags: smallvec![],
        implicits: smallvec![],
        inventory: InventorySize {
            width: 2,
            height: 3,
        },
        release_state: ReleaseState::Released,
        patch_range: PatchRange::ALL,
    }])
}

fn es_goal() -> Goal {
    Goal {
        target: poc2_strategies::Target {
            prefixes: vec![TargetSpec {
                concept: Some(ConceptId::from("EnergyShield")),
                concept_any: vec![],
                affix: Some(AffixType::Prefix),
                count: 3,
                min_tier: Some(1),
                allow_hybrid: true,
            }],
            suffixes: vec![],
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

fn strategy_registry() -> StrategyRegistry {
    let strategy =
        load_strategy_str(ES_BODY_ARMOUR_TOML).expect("3xt1-es-body-armour-isolation must parse");
    StrategyRegistry::from_strategies(vec![strategy])
}

/// First strategy-sourced action emitted for `(item, ctx)`, if any.
fn strategy_action(
    item: &Item,
    ctx: &PredicateContext<'_>,
    strategies: &StrategyRegistry,
    registry: &ModRegistry,
) -> Option<poc2_advisor::AdvisorAction> {
    let rules = RuleSet::default();
    let resolver = DefaultCurrencyResolver::new();
    let stash = Stash::unlimited();
    poc2_advisor::candidate::generate_candidates_with_goal(
        item,
        ctx,
        &rules,
        strategies,
        &resolver,
        &stash,
        PatchVersion::PATCH_0_4_0,
        poc2_engine::patch::League::current(),
        Some(&es_goal()),
        registry,
        None,
    )
    .into_iter()
    .find(|c| matches!(&c.source, RecommendationSource::Strategy { .. }))
    .map(|c| c.action)
}

/// A captured item (metadata-path base) with the class resolved through
/// the `BaseRegistry` must match `ItemClass`-gated strategies identically
/// to the legacy class-id-placeholder form.
#[test]
fn bundle_base_item_surfaces_class_gated_strategy_like_legacy_form() {
    let strategies = strategy_registry();
    let registry = ModRegistry::from_mods(vec![], vec![]);
    let base_reg = body_armour_base_registry();

    let legacy = normal_item("BodyArmour");
    let bundled = normal_item(BUNDLE_BASE);

    let legacy_ctx = PredicateContext::new(&registry);
    let legacy_action = strategy_action(&legacy, &legacy_ctx, &strategies, &registry)
        .expect("legacy class-id base must surface the strategy");

    let bundled_ctx =
        PredicateContext::new(&registry).with_item_class(base_reg.resolve_item_class(&bundled));
    let bundled_action = strategy_action(&bundled, &bundled_ctx, &strategies, &registry)
        .expect("metadata-path base with a resolved class must surface the strategy");

    assert_eq!(legacy_action, bundled_action);

    // Without the resolved class the metadata path matches no strategy —
    // the misclassification this registry threading exists to fix.
    assert!(strategy_action(&bundled, &legacy_ctx, &strategies, &registry).is_none());
}

/// End-to-end: `plan()` threads `PlanInput.base_registry` into the
/// per-node `PredicateContext`, so a captured item recommends the same
/// strategy first-step as the legacy form.
#[test]
fn planner_resolves_bundle_base_via_base_registry() {
    let strategies = strategy_registry();
    let registry = ModRegistry::from_mods(vec![], vec![]);
    let rules = RuleSet::default();
    let resolver = DefaultCurrencyResolver::new();
    let valuator = Valuator::default();
    let stash = Stash::unlimited();
    let base_reg = body_armour_base_registry();

    let run = |item: Item, base_registry: Option<&BaseRegistry>| {
        let input = PlanInput {
            item,
            goal: es_goal(),
            rules: &rules,
            strategies: &strategies,
            registry: &registry,
            resolver: &resolver,
            valuator: &valuator,
            stash: &stash,
            patch: PatchVersion::PATCH_0_4_0,
            league: poc2_engine::patch::League::current(),
            plugin_dispatch: None,
            base_registry,
            trained_models: None,
            config: BeamConfig {
                width: 6,
                depth: 2,
                risk: 0.5,
                top_n: 8,
                seed: 0,
                mc_samples: 1,
                weights: ScoringWeights::default(),
                trained_uplift_weight: 1000.0,
            },
        };
        plan(&input)
    };

    let strategy_rec = |recs: &[poc2_advisor::Recommendation]| {
        recs.iter()
            .find(|r| {
                matches!(
                    &r.source,
                    RecommendationSource::Strategy { id, .. }
                        if id == "3xt1-es-body-armour-isolation"
                )
            })
            .map(|r| r.action.clone())
    };

    let legacy_recs = run(normal_item("BodyArmour"), None);
    let legacy_action =
        strategy_rec(&legacy_recs).expect("legacy base must recommend the strategy step");

    let bundled_recs = run(normal_item(BUNDLE_BASE), Some(&base_reg));
    let bundled_action = strategy_rec(&bundled_recs)
        .expect("captured bundle base must recommend the strategy step via the registry");

    assert_eq!(legacy_action, bundled_action);

    // Negative control: without the registry the captured base resolves
    // to nothing and the strategy never fires.
    let unresolved_recs = run(normal_item(BUNDLE_BASE), None);
    assert!(
        strategy_rec(&unresolved_recs).is_none(),
        "metadata-path base without a BaseRegistry must not match the strategy"
    );
}
