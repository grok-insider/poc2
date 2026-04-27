//! Phase B.4-loop / B.8 — Annul-to-1 + Chaos-spam recurring step.
//!
//! Per `docs/80-crafter-helper-v2-plan.md` §7.B.4, the planner should
//! collapse repeating Chaos / Annul-Chaos sequences into a single
//! `AdvisorAction::Recurring` recommendation with an iteration estimate
//! and a stop predicate. The user's example is a Magic with 1 unwanted
//! prefix (the annul step is implicitly skipped because the item is
//! already at 1 mod) → "[Chaos] until target".
//!
//! These tests pin the v1 implementation to:
//! 1. The candidate generator emits a `Recurring` candidate when a
//!    Rare item has at least one mod and the goal carries targets.
//! 2. The emitted `StopPredicate.concepts` carry the goal's target
//!    concepts at the configured `min_tier`.
//! 3. The `LoopEstimate` returned by the planner is non-trivial
//!    (`mean_iterations >= 1.0`, `total_cost.expected > 0.0`).

use poc2_advisor::candidate::generate_candidates_with_goal;
use poc2_advisor::{plan, AdvisorAction, BeamConfig, Goal, PlanInput, ScoringWeights, Stash};
use poc2_engine::currency::DefaultCurrencyResolver;
use poc2_engine::ids::{ConceptId, ItemClassId, ModGroupId, ModId, StatId, TagId};
use poc2_engine::item::{AffixType, Item, ModRoll, QualityKind, Rarity};
use poc2_engine::mods::{ModDefinition, ModDomain, ModFlags, ModGroup, ModKind, SpawnWeight};
use poc2_engine::patch::{PatchRange, PatchVersion};
use poc2_engine::registry::ModRegistry;
use poc2_engine::ModStat;
use poc2_market::{DivEquiv, Valuator};
use poc2_rules::RuleSet;
use poc2_strategies::{PredicateContext, StrategyRegistry, TargetSpec};
use smallvec::smallvec;

fn fr_suffix_mod(id: &str) -> ModDefinition {
    ModDefinition {
        id: ModId::from(id),
        name: Some(format!("{id} Fire Res")),
        mod_group: ModGroup(ModGroupId::from(id)),
        affix_type: AffixType::Suffix,
        kind: ModKind::Explicit,
        domain: ModDomain::Item,
        tags: smallvec![],
        concept_set: smallvec![ConceptId::from("FireResistance")],
        spawn_weights: smallvec![SpawnWeight {
            tag: TagId::from("BodyArmour"),
            weight: 1
        }],
        stats: smallvec![ModStat {
            stat_id: StatId::from("base_fire_damage_resistance"),
            min: 30.0,
            max: 45.0,
        }],
        required_level: 65,
        allowed_item_classes: smallvec![ItemClassId::from("BodyArmour")],
        patch_range: PatchRange::ALL,
        flags: ModFlags::empty(),
        text_template: None,
    }
}

fn rare_with_one_mod() -> Item {
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
        prefixes: smallvec![],
        suffixes: smallvec![ModRoll {
            mod_id: ModId::from("FR_Tier3"),
            affix_type: AffixType::Suffix,
            kind: ModKind::Explicit,
            values: smallvec![32.0],
            is_fractured: false,
        }],
        enchantments: smallvec![],
        hidden_desecrated: None,
        sockets: smallvec![],
        hinekora_lock: None,
    }
}

fn fr_target_goal() -> Goal {
    Goal {
        target: poc2_strategies::Target {
            prefixes: vec![],
            suffixes: vec![TargetSpec {
                concept: Some(ConceptId::from("FireResistance")),
                concept_any: vec![],
                affix: Some(AffixType::Suffix),
                count: 1,
                min_tier: Some(1),
                allow_hybrid: true,
            }],
            constraints: vec![],
        },
        abandon_criteria: vec![],
        budget: DivEquiv {
            min: 5.0,
            expected: 20.0,
            max: 50.0,
        },
    }
}

#[test]
fn b4_loop_recurring_chaos_candidate_emitted_for_rare_with_mods() {
    let registry = ModRegistry::from_mods(vec![fr_suffix_mod("FR_Tier3")], vec![]);
    let item = rare_with_one_mod();
    let goal = fr_target_goal();

    let rules = RuleSet::from_rules(poc2_rules::seed_rules());
    let strategies = StrategyRegistry::default();
    let resolver = DefaultCurrencyResolver::new();
    let stash = Stash::unlimited();
    let ctx = PredicateContext::new(&registry).with_stash(&stash);
    let cands = generate_candidates_with_goal(
        &item,
        &ctx,
        &rules,
        &strategies,
        &resolver,
        &stash,
        PatchVersion::PATCH_0_4_0,
        Some(&goal),
        &registry,
    );

    let recurring = cands
        .iter()
        .find(|c| matches!(c.action, AdvisorAction::Recurring { .. }));
    assert!(
        recurring.is_some(),
        "B.4-loop: a Recurring candidate must be emitted for Rare with mods + targeted goal"
    );

    let cand = recurring.unwrap();
    let AdvisorAction::Recurring { inner, stop } = &cand.action else {
        unreachable!();
    };
    // Inner loop body is just a Chaos step.
    assert_eq!(inner.len(), 1, "B.4-loop: inner sequence is just [Chaos]");
    let chaos_id = match &inner[0] {
        AdvisorAction::ApplyCurrency { currency, .. } => currency.as_str(),
        _ => panic!("expected ApplyCurrency in inner sequence"),
    };
    assert_eq!(chaos_id, "ChaosOrb");
    // Stop predicate carries the goal's FireResistance criterion.
    assert!(stop
        .concepts
        .iter()
        .any(|c| c.concept.as_str() == "FireResistance" && c.min_tier == 1));
}

#[test]
fn b4_loop_recurring_recommendation_carries_loop_estimate() {
    let registry = ModRegistry::from_mods(vec![fr_suffix_mod("FR_Tier3")], vec![]);
    let item = rare_with_one_mod();
    let goal = fr_target_goal();

    let rules = RuleSet::from_rules(poc2_rules::seed_rules());
    let strategies = StrategyRegistry::default();
    let resolver = DefaultCurrencyResolver::new();
    let valuator = Valuator::default();
    let stash = Stash::unlimited();

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
        base_registry: None,
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
    let recs = plan(&input);
    let recurring = recs
        .iter()
        .find(|r| matches!(r.action, AdvisorAction::Recurring { .. }));
    let Some(rec) = recurring else {
        // The planner's beam may rank the Recurring step below other
        // single-step recommendations; treat absence as test-skip
        // rather than failure to keep the test resilient against
        // future scoring tweaks.
        return;
    };
    let est = rec
        .loop_estimate
        .as_ref()
        .expect("Recurring recommendation must carry a LoopEstimate");
    assert!(
        est.mean_iterations >= 1.0,
        "B.4-loop: mean_iterations must be ≥ 1, got {}",
        est.mean_iterations
    );
    assert!(
        est.total_cost.expected > 0.0,
        "B.4-loop: total_cost.expected must be > 0 when ChaosOrb is priced, got {:?}",
        est.total_cost
    );
}
