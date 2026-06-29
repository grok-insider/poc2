//! Integration test for the 0.5 Recombinator league gate, exercised
//! end-to-end through the public advisor `plan()` API.
//!
//! The Recombinator was removed in 0.5 "Return of the Ancients": it is
//! disabled in the Runes of Aldur **Challenge** league but kept in
//! **Standard** on migrated items, and remained available everywhere in
//! 0.4. The candidate generator drops `Recombine` candidates when
//! `poc2_engine::currency::recombinator_available(patch, league)` is
//! false (see `candidate::passes_engine_preconditions` /
//! `finalize_candidates`).
//!
//! We drive this gate through the real beam-search planner: a tiny
//! in-memory strategy whose first actionable step is a `Recombine` emits
//! a Recombine candidate, and we assert it survives into the
//! recommendation list for `(0.5, Standard)` and `(0.4, Challenge)` but
//! is absent for `(0.5, Challenge)`. A second test exercises the engine's
//! `recombinator_available` predicate directly across the full
//! `(patch, league)` matrix as a belt-and-suspenders integration check.

use poc2_advisor::{plan, AdvisorAction, BeamConfig, Goal, PlanInput, Stash};
use poc2_engine::currency::DefaultCurrencyResolver;
use poc2_engine::ids::{ConceptId, ItemClassId};
use poc2_engine::item::{Item, QualityKind, Rarity};
use poc2_engine::patch::{League, PatchVersion};
use poc2_engine::registry::ModRegistry;
use poc2_market::{DivEquiv, Valuator};
use poc2_rules::RuleSet;
use poc2_strategies::{
    Action, Confidence, ItemPredicate, Source, Step, StepId, Strategy, StrategyId,
    StrategyRegistry, Target, TargetSpec,
};
use smallvec::smallvec;

/// A fresh Rare ilvl 82 body armour with one rolled prefix — Recombine's
/// only legal rarity is Rare, and a non-trivial state keeps the goal
/// unsatisfied at the root so the planner actually generates candidates.
fn rare_body_armour() -> Item {
    use poc2_engine::ids::ModId;
    use poc2_engine::item::{AffixType, ModRoll};
    use poc2_engine::mods::ModKind;

    Item {
        base: ItemClassId::from("BodyArmour").as_str().into(),
        ilvl: 82,
        rarity: Rarity::Rare,
        corrupted: false,
        sanctified: false,
        mirrored: false,
        quality: 0,
        quality_kind: QualityKind::Untagged,
        implicits: smallvec![],
        prefixes: smallvec![ModRoll {
            mod_id: ModId::from("FillerPrefix"),
            affix_type: AffixType::Prefix,
            kind: ModKind::Explicit,
            values: smallvec![],
            is_fractured: false,
        }],
        suffixes: smallvec![],
        enchantments: smallvec![],
        hidden_desecrated: None,
        sockets: smallvec![],
        hinekora_lock: None,
    }
}

/// A goal the fresh Rare item does not yet satisfy (3 T1 ES prefixes), so
/// the planner is forced to expand candidates rather than short-circuit
/// to `Stop`.
fn three_es_prefix_goal() -> Goal {
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
            suffixes: vec![],
            constraints: vec![],
        },
        abandon_criteria: vec![],
        budget: DivEquiv::point(100.0),
    }
}

/// A minimal in-memory strategy whose entry step is a `Recombine` action,
/// applicable to `BodyArmour` in every patch (no patch_min / patch_max).
/// When the planner runs this strategy, the candidate generator lifts the
/// step into an `AdvisorAction::Recombine`; whether that candidate
/// survives `finalize_candidates` is exactly the league gate under test.
fn recombine_strategy() -> Strategy {
    Strategy {
        id: StrategyId::from("test-recombine-strategy"),
        name: "Test Recombine".into(),
        source: Source::default(),
        patch_min: None,
        patch_max: None,
        item_classes: vec![ItemClassId::from("BodyArmour")],
        attribute_pools: vec![],
        preconditions: vec![],
        target: Target::default(),
        abandon_criteria: vec![],
        steps: vec![Step {
            id: StepId::from("S1-recombine"),
            action: Action::Recombine {
                other_item: ItemPredicate::Always,
                omens: vec![],
            },
            target_check: None,
            on_success: None,
            on_failure: None,
            recovery: smallvec![],
            note: Some("Recombine with any owned Rare.".into()),
        }],
        expected_cost_div: None,
        expected_success_prob: None,
        confidence: Confidence::Verified,
        note: None,
    }
}

/// Run the full planner for a given `(patch, league)` with the
/// Recombine-emitting strategy loaded, and return the recommendations.
fn run_plan(patch: PatchVersion, league: League) -> Vec<poc2_advisor::Recommendation> {
    let registry = ModRegistry::from_mods(Vec::new(), Vec::new());
    // Empty rule set so the only depth-1 candidate source is our strategy
    // — this keeps the assertion focused on the Recombine gate.
    let rules = RuleSet::default();
    let strategies = StrategyRegistry::from_strategies(vec![recombine_strategy()]);
    let resolver = DefaultCurrencyResolver::new();
    let valuator = Valuator::default();
    let stash = Stash::unlimited();

    let input = PlanInput {
        item: rare_body_armour(),
        goal: three_es_prefix_goal(),
        rules: &rules,
        strategies: &strategies,
        registry: &registry,
        resolver: &resolver,
        valuator: &valuator,
        stash: &stash,
        patch,
        league,
        plugin_dispatch: None,
        base_registry: None,
        trained_models: None,
        config: BeamConfig {
            // Wide beam + high top_n + depth 1 so every depth-1 first
            // action survives into its own recommendation bucket. This
            // makes "is Recombine present?" a reliable signal that is not
            // confounded by top-N truncation.
            width: 16,
            depth: 1,
            top_n: 16,
            seed: 0,
            mc_samples: 1,
            ..BeamConfig::default()
        },
    };
    plan(&input)
}

/// True iff any recommendation's action is a `Recombine`.
fn has_recombine(recs: &[poc2_advisor::Recommendation]) -> bool {
    recs.iter()
        .any(|r| matches!(r.action, AdvisorAction::Recombine { .. }))
}

#[test]
fn recombine_dropped_in_0_5_challenge_via_plan() {
    // 0.5 Runes of Aldur (Challenge): the Recombinator is removed, so the
    // candidate must never reach the recommendation list.
    let recs = run_plan(PatchVersion::PATCH_0_5_0, League::Challenge);
    // The planner must still succeed (produce *some* recommendation —
    // even if only a fallback) rather than panic or hang.
    assert!(
        !recs.is_empty(),
        "planner should still produce a recommendation in 0.5 Challenge"
    );
    assert!(
        !has_recombine(&recs),
        "Recombine must be dropped in 0.5 Runes of Aldur (Challenge); got {:?}",
        recs.iter().map(|r| r.action.clone()).collect::<Vec<_>>()
    );
}

#[test]
fn recombine_kept_in_0_5_standard_via_plan() {
    // 0.5 Standard: the Recombinator still functions on migrated items, so
    // the candidate must reach the recommendation list.
    let recs = run_plan(PatchVersion::PATCH_0_5_0, League::Standard);
    assert!(
        has_recombine(&recs),
        "Recombine must survive in 0.5 Standard; got {:?}",
        recs.iter().map(|r| r.action.clone()).collect::<Vec<_>>()
    );
}

#[test]
fn recombine_kept_in_0_4_challenge_via_plan() {
    // 0.4 (any league): the Recombinator predates its removal and is
    // available everywhere, including the challenge league.
    let recs = run_plan(PatchVersion::PATCH_0_4_0, League::Challenge);
    assert!(
        has_recombine(&recs),
        "Recombine must survive in 0.4 Challenge (pre-removal); got {:?}",
        recs.iter().map(|r| r.action.clone()).collect::<Vec<_>>()
    );
}

#[test]
fn recombinator_available_matrix() {
    // Direct integration check on the engine predicate the advisor gate
    // consults. Mirrors the end-to-end `plan()` tests above one layer
    // down, covering the full (patch, league) matrix including the 0.3
    // pre-release and the exact 0.5 boundary.
    use poc2_engine::currency::recombinator_available;

    let p03 = PatchVersion::new(0, 3, 0);
    let p04 = PatchVersion::PATCH_0_4_0;
    let p05 = PatchVersion::PATCH_0_5_0;

    // Pre-0.5: available in every league.
    for league in [League::Standard, League::Challenge] {
        assert!(
            recombinator_available(p03, league),
            "0.3 should keep Recombinator in {league:?}"
        );
        assert!(
            recombinator_available(p04, league),
            "0.4 should keep Recombinator in {league:?}"
        );
    }

    // 0.5 boundary: Standard keeps it, Challenge drops it.
    assert!(
        recombinator_available(p05, League::Standard),
        "0.5 Standard must keep the Recombinator (migrated items)"
    );
    assert!(
        !recombinator_available(p05, League::Challenge),
        "0.5 Challenge (Runes of Aldur) must drop the Recombinator"
    );
}
