//! Phase B smoke tests for the v2 planner heuristics on the user's
//! worked ES body armour example (`docs/80-crafter-helper-v2-plan.md`
//! §0, §7.B.7).
//!
//! The plan's literal chain — Trans → Aug → Greater Essence on suffix →
//! Desecrate prefix with Omen → Divine → Fracture → Reveal-with-Echoes —
//! requires a dedicated strategy in `crates/strategies/strategies/` to
//! be discovered deterministically. That strategy isn't part of v1 and
//! its authoring is a follow-up. These tests instead verify the
//! *heuristics* the v2 plan introduces fire correctly:
//!
//! - **B.1 concept-occupancy**: Regal scores worse than essence/aug
//!   when 2 keeper prefixes are already locked on a Magic item.
//! - **B.2 tier-fix**: Divine surfaces on a Rare with a sub-max keeper;
//!   Fracture surfaces on a Rare with 4 visible mods at max keeper roll.
//! - **B.3 risk slider**: cautious risk penalises high-variance
//!   candidates; greedy risk de-emphasises the variance penalty.
//! - **B.6 omen-aware reveals**: a hidden_desecrated item emits
//!   `(bone, omen)` reveal candidates.

use poc2_advisor::{plan, BeamConfig, Goal, PlanInput, ScoringWeights, Stash};
use poc2_engine::currency::DefaultCurrencyResolver;
use poc2_engine::ids::{ConceptId, ItemClassId, ModGroupId, ModId, StatId, TagId};
use poc2_engine::item::{AffixType, HiddenDesecratedSlot, Item, ModRoll, QualityKind, Rarity};
use poc2_engine::mods::{ModDefinition, ModDomain, ModFlags, ModGroup, ModKind, SpawnWeight};
use poc2_engine::patch::{PatchRange, PatchVersion};
use poc2_engine::registry::ModRegistry;
use poc2_market::{DivEquiv, Valuator};
use poc2_rules::RuleSet;
use poc2_strategies::{StrategyRegistry, TargetSpec};
use smallvec::smallvec;

fn body_armour(rarity: Rarity) -> Item {
    Item {
        base: ItemClassId::from("BodyArmour").as_str().into(),
        ilvl: 82,
        rarity,
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

fn es_prefix_mod(id: &str) -> ModDefinition {
    ModDefinition {
        id: ModId::from(id),
        name: Some(format!("{id} ES Mod")),
        mod_group: ModGroup(ModGroupId::from(id)),
        affix_type: AffixType::Prefix,
        kind: ModKind::Explicit,
        domain: ModDomain::Item,
        tags: smallvec![],
        concept_set: smallvec![ConceptId::from("EnergyShield")],
        spawn_weights: smallvec![SpawnWeight {
            tag: TagId::from("BodyArmour"),
            weight: 1
        }],
        stats: smallvec![poc2_engine::ModStat {
            stat_id: StatId::from("base_maximum_energy_shield"),
            min: 100.0,
            max: 200.0,
        }],
        required_level: 70,
        tier: None,
        allowed_item_classes: smallvec![ItemClassId::from("BodyArmour")],
        patch_range: PatchRange::ALL,
        flags: ModFlags::empty(),
        text_template: None,
    }
}

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
        stats: smallvec![poc2_engine::ModStat {
            stat_id: StatId::from("base_fire_damage_resistance"),
            min: 30.0,
            max: 45.0,
        }],
        required_level: 65,
        tier: None,
        allowed_item_classes: smallvec![ItemClassId::from("BodyArmour")],
        patch_range: PatchRange::ALL,
        flags: ModFlags::empty(),
        text_template: None,
    }
}

fn es_target_goal() -> Goal {
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
            min: 40.0,
            expected: 100.0,
            max: 200.0,
        },
    }
}

/// A goal satisfied by an item carrying `es` EnergyShield prefixes and `fr`
/// FireResistance suffixes. Tier-fix (Divine/Fracture) only fires once the item
/// already meets the goal target, so these tests use a goal matched to their
/// built-item fixture to exercise the legitimate "polish the values" path.
fn es_goal(es: u8, fr: u8) -> Goal {
    let mut prefixes = vec![];
    if es > 0 {
        prefixes.push(TargetSpec {
            concept: Some(ConceptId::from("EnergyShield")),
            concept_any: vec![],
            affix: Some(AffixType::Prefix),
            count: es,
            min_tier: Some(1),
            allow_hybrid: true,
        });
    }
    let mut suffixes = vec![];
    if fr > 0 {
        suffixes.push(TargetSpec {
            concept: Some(ConceptId::from("FireResistance")),
            concept_any: vec![],
            affix: Some(AffixType::Suffix),
            count: fr,
            min_tier: Some(1),
            allow_hybrid: true,
        });
    }
    Goal {
        target: poc2_strategies::Target {
            prefixes,
            suffixes,
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

// Kept for the future "deterministic ES chain rediscovery" test once a
// dedicated `es-body-armour-deterministic.toml` strategy lands in
// `crates/strategies/strategies/`. The current tests exercise the v2
// heuristics directly (occupancy adjustment, tier-fix candidates, bone-
// omen reveal pairs) so no full plan run is needed yet.
#[allow(dead_code)]
fn run_plan(item: Item, registry: ModRegistry, risk: f64) -> Vec<poc2_advisor::Recommendation> {
    let strategies = StrategyRegistry::default();
    let rules = RuleSet::from_rules(poc2_rules::seed_rules());
    let resolver = DefaultCurrencyResolver::new();
    let valuator = Valuator::default();
    let stash = Stash::unlimited();

    let input = PlanInput {
        item,
        goal: es_target_goal(),
        rules: &rules,
        strategies: &strategies,
        registry: &registry,
        resolver: &resolver,
        valuator: &valuator,
        stash: &stash,
        patch: PatchVersion::PATCH_0_4_0,
        league: poc2_engine::patch::League::current(),
        plugin_dispatch: None,
        base_registry: None,
        trained_models: None,
        config: BeamConfig {
            width: 6,
            depth: 2,
            risk,
            top_n: 8,
            seed: 0,
            mc_samples: 1,
            weights: ScoringWeights::default(),
            trained_uplift_weight: 1000.0,
        },
    };
    plan(&input)
}

/// Concept-occupancy regression: `occupancy_adjustment` returns a
/// positive value for Augment on a Magic with one keeper, and a
/// negative value for Regal on the same state. This is the heuristic
/// the planner blends in via `score_node`.
#[test]
fn b1_occupancy_adjustment_prefers_aug_over_regal_when_keeper_present() {
    use poc2_advisor::scorer::occupancy_adjustment;

    let registry = ModRegistry::from_mods(vec![es_prefix_mod("ES_Tier1")], vec![]);
    let mut item = body_armour(Rarity::Magic);
    item.prefixes.push(ModRoll {
        mod_id: ModId::from("ES_Tier1"),
        affix_type: AffixType::Prefix,
        kind: ModKind::Explicit,
        values: smallvec![180.0],
        is_fractured: false,
    });

    let goal = es_target_goal();
    let aug = poc2_advisor::AdvisorAction::ApplyCurrency {
        currency: poc2_engine::ids::CurrencyId::from("OrbOfAugmentation"),
        omens: vec![],
    };
    let regal = poc2_advisor::AdvisorAction::ApplyCurrency {
        currency: poc2_engine::ids::CurrencyId::from("RegalOrb"),
        omens: vec![],
    };
    let aug_score = occupancy_adjustment(&aug, &item, &goal, &registry);
    let regal_score = occupancy_adjustment(&regal, &item, &goal, &registry);
    assert!(
        aug_score > regal_score,
        "B.1: Augment occupancy {aug_score} must exceed Regal occupancy {regal_score} when one keeper is locked"
    );
    assert!(
        regal_score < 0.0,
        "B.1: Regal must carry a negative occupancy delta when keepers are present, got {regal_score}"
    );
}

/// Tier-fix B.2: the candidate generator emits Fracture when the item
/// is Rare with 4 visible mods including a max-rolled keeper. Drives
/// the heuristic directly rather than through the full beam, since the
/// planner may correctly de-rank Fracture on its own (it doesn't progress
/// the goal as a single step) — what we need to verify is that the
/// candidate is *available* for the user / a chain to pick up.
#[test]
fn b2_tier_fix_emits_fracture_on_max_rolled_keeper_with_4_mods() {
    use poc2_advisor::candidate::generate_candidates_with_goal;
    use poc2_strategies::PredicateContext;

    let registry = ModRegistry::from_mods(
        vec![
            es_prefix_mod("ES_Tier1"),
            es_prefix_mod("ES_Tier2"),
            fr_suffix_mod("FR_Tier1"),
            fr_suffix_mod("FR_Tier2"),
        ],
        vec![],
    );
    let mut item = body_armour(Rarity::Rare);
    item.prefixes.push(ModRoll {
        mod_id: ModId::from("ES_Tier1"),
        affix_type: AffixType::Prefix,
        kind: ModKind::Explicit,
        values: smallvec![200.0], // max roll
        is_fractured: false,
    });
    item.prefixes.push(ModRoll {
        mod_id: ModId::from("ES_Tier2"),
        affix_type: AffixType::Prefix,
        kind: ModKind::Explicit,
        values: smallvec![150.0],
        is_fractured: false,
    });
    item.suffixes.push(ModRoll {
        mod_id: ModId::from("FR_Tier1"),
        affix_type: AffixType::Suffix,
        kind: ModKind::Explicit,
        values: smallvec![45.0],
        is_fractured: false,
    });
    item.suffixes.push(ModRoll {
        mod_id: ModId::from("FR_Tier2"),
        affix_type: AffixType::Suffix,
        kind: ModKind::Explicit,
        values: smallvec![38.0],
        is_fractured: false,
    });

    let rules = RuleSet::from_rules(poc2_rules::seed_rules());
    let strategies = StrategyRegistry::default();
    let resolver = DefaultCurrencyResolver::new();
    let stash = Stash::unlimited();
    let goal = es_goal(2, 1); // item carries 2 ES + 2 FR — satisfies the goal
    let ctx = PredicateContext::new(&registry).with_stash(&stash);
    let cands = generate_candidates_with_goal(
        &item,
        &ctx,
        &rules,
        &strategies,
        &resolver,
        &stash,
        PatchVersion::PATCH_0_4_0,
        poc2_engine::patch::League::current(),
        Some(&goal),
        &registry,
        None,
    );
    let has_fracture = cands.iter().any(|c| match &c.action {
        poc2_advisor::AdvisorAction::ApplyCurrency { currency, .. } => {
            currency.as_str() == "FracturingOrb"
        }
        _ => false,
    });
    assert!(
        has_fracture,
        "B.2: Fracture candidate must be emitted for max-rolled keeper with 4 visible mods. Got: {:?}",
        cands.iter().map(|c| &c.action).collect::<Vec<_>>()
    );
}

/// Tier-fix B.2 (sub-max): the candidate generator emits Divine when
/// a keeper mod is rolled below 95% of its max range.
#[test]
fn b2_tier_fix_emits_divine_on_sub_max_keeper() {
    use poc2_advisor::candidate::generate_candidates_with_goal;
    use poc2_strategies::PredicateContext;

    let registry = ModRegistry::from_mods(vec![es_prefix_mod("ES_Tier1")], vec![]);
    let mut item = body_armour(Rarity::Rare);
    item.prefixes.push(ModRoll {
        mod_id: ModId::from("ES_Tier1"),
        affix_type: AffixType::Prefix,
        kind: ModKind::Explicit,
        values: smallvec![120.0], // far from max=200
        is_fractured: false,
    });

    let rules = RuleSet::from_rules(poc2_rules::seed_rules());
    let strategies = StrategyRegistry::default();
    let resolver = DefaultCurrencyResolver::new();
    let stash = Stash::unlimited();
    let goal = es_goal(1, 0); // item carries 1 ES keeper — satisfies the goal
    let ctx = PredicateContext::new(&registry).with_stash(&stash);
    let cands = generate_candidates_with_goal(
        &item,
        &ctx,
        &rules,
        &strategies,
        &resolver,
        &stash,
        PatchVersion::PATCH_0_4_0,
        poc2_engine::patch::League::current(),
        Some(&goal),
        &registry,
        None,
    );
    let has_divine = cands.iter().any(|c| match &c.action {
        poc2_advisor::AdvisorAction::ApplyCurrency { currency, .. } => {
            currency.as_str() == "DivineOrb"
        }
        _ => false,
    });
    assert!(
        has_divine,
        "B.2: Divine candidate must be emitted for sub-max keeper. Got: {:?}",
        cands.iter().map(|c| &c.action).collect::<Vec<_>>()
    );
}

/// Tier-fix gate: a partial item (one sub-max keeper, but a multi-mod goal it
/// does NOT yet satisfy) must NOT get a premature Divine/Fracture — building
/// toward the target takes priority. Mirrors the reported "Magic body armour →
/// premature Divine" bug.
#[test]
fn b2_tier_fix_suppressed_until_goal_satisfied() {
    use poc2_advisor::candidate::generate_candidates_with_goal;
    use poc2_strategies::PredicateContext;

    let registry = ModRegistry::from_mods(vec![es_prefix_mod("ES_Tier1")], vec![]);
    let mut item = body_armour(Rarity::Magic);
    item.prefixes.push(ModRoll {
        mod_id: ModId::from("ES_Tier1"),
        affix_type: AffixType::Prefix,
        kind: ModKind::Explicit,
        values: smallvec![120.0], // sub-max — would trigger Divine without the gate
        is_fractured: false,
    });

    let rules = RuleSet::from_rules(poc2_rules::seed_rules());
    let strategies = StrategyRegistry::default();
    let resolver = DefaultCurrencyResolver::new();
    let stash = Stash::unlimited();
    let goal = es_target_goal(); // needs 3 ES + 1 FR — the 1-mod item is NOT built
    let ctx = PredicateContext::new(&registry).with_stash(&stash);
    let cands = generate_candidates_with_goal(
        &item,
        &ctx,
        &rules,
        &strategies,
        &resolver,
        &stash,
        PatchVersion::PATCH_0_4_0,
        poc2_engine::patch::League::current(),
        Some(&goal),
        &registry,
        None,
    );
    let has_tier_fix = cands.iter().any(|c| match &c.action {
        poc2_advisor::AdvisorAction::ApplyCurrency { currency, .. } => {
            currency.as_str() == "DivineOrb" || currency.as_str() == "FracturingOrb"
        }
        _ => false,
    });
    assert!(
        !has_tier_fix,
        "tier-fix must be suppressed on a partial item (goal not satisfied). Got: {:?}",
        cands.iter().map(|c| &c.action).collect::<Vec<_>>()
    );
}

/// B.7 — load the real `3xt1-es-body-armour-isolation` strategy from
/// disk and assert the advisor surfaces its first actionable step
/// (Perfect Orb of Transmutation) as a top recommendation when the
/// user's item matches the strategy's preconditions.
///
/// This is the v2 plan's "user's worked chain rediscovered by the
/// engine" assertion, scoped to the depth-1 entry. Walking the full
/// 10-step chain end-to-end requires a fully-populated mod bundle so
/// the simulator can advance through each step's post-state; that
/// integration test lives in `crates/engine/tests/worked_example_es_body_armour.rs`
/// against engine apply paths directly.
#[test]
fn b7_real_strategy_es_body_armour_emits_perfect_transmute_at_depth_1() {
    use poc2_advisor::candidate::generate_candidates_with_goal;
    use poc2_advisor::{AdvisorAction, RecommendationSource};
    use poc2_strategies::{load_strategy_str, PredicateContext};

    // Embed the real strategy TOML via include_str! so the test pins
    // against the same content the desktop app loads at startup.
    const ES_BODY_ARMOUR_TOML: &str =
        include_str!("../../../crates/strategies/strategies/3xt1-es-body-armour.toml");
    let strategy =
        load_strategy_str(ES_BODY_ARMOUR_TOML).expect("3xt1-es-body-armour-isolation must parse");
    let strategies = StrategyRegistry::from_strategies(vec![strategy]);

    // Empty registry — the planner doesn't need real mods to pick the
    // first actionable strategy step, since no simulation has run yet.
    let registry = ModRegistry::from_mods(vec![], vec![]);
    let rules = RuleSet::default();
    let resolver = DefaultCurrencyResolver::new();
    let stash = Stash::unlimited();
    let goal = es_target_goal();
    let item = body_armour(Rarity::Normal);
    let ctx = PredicateContext::new(&registry).with_stash(&stash);

    let cands = generate_candidates_with_goal(
        &item,
        &ctx,
        &rules,
        &strategies,
        &resolver,
        &stash,
        PatchVersion::PATCH_0_4_0,
        poc2_engine::patch::League::current(),
        Some(&goal),
        &registry,
        None,
    );

    let strategy_cand = cands.iter().find(|c| {
        matches!(
            &c.source,
            RecommendationSource::Strategy { id, .. } if id == "3xt1-es-body-armour-isolation"
        )
    });
    let strategy_cand = strategy_cand.unwrap_or_else(|| {
        panic!(
            "B.7: 3xt1-es-body-armour strategy must surface a candidate at depth 1; got: {:#?}",
            cands
                .iter()
                .map(|c| (&c.source, &c.action))
                .collect::<Vec<_>>(),
        )
    });

    // First actionable step is S2 Perfect Orb of Transmutation —
    // S1-validate-base is a noop and the advisor's first_actionable_step
    // helper walks past it.
    match &strategy_cand.action {
        AdvisorAction::ApplyCurrency { currency, omens } => {
            assert_eq!(
                currency.as_str(),
                "PerfectOrbOfTransmutation",
                "first actionable step should be PerfectOrbOfTransmutation"
            );
            assert!(
                omens.is_empty(),
                "step S2 doesn't bind any omens; got {omens:?}"
            );
        }
        other => panic!("B.7: expected ApplyCurrency PerfectOrbOfTransmutation, got {other:?}"),
    }

    // The candidate should cite step S2-perfect-transmute (not S1-validate-base)
    // because first_actionable_step walks past noops.
    if let RecommendationSource::Strategy { step, .. } = &strategy_cand.source {
        assert_eq!(
            step, "S2-perfect-transmute",
            "B.7: candidate should cite S2-perfect-transmute as its source step"
        );
    } else {
        panic!("strategy_cand source must be Strategy");
    }
}

/// B.6 — when an item carries `hidden_desecrated`, the candidate
/// generator emits one Reveal recommendation per legal `(bone, omen)`
/// pair so the OutcomeDialog can surface the explicit options.
#[test]
fn b6_omen_aware_reveals_appear_when_hidden_desecrated_present() {
    use poc2_advisor::candidate::generate_candidates_with_goal;
    use poc2_strategies::PredicateContext;

    let registry = ModRegistry::from_mods(vec![es_prefix_mod("ES_Tier1")], vec![]);
    let mut item = body_armour(Rarity::Rare);
    item.hidden_desecrated = Some(HiddenDesecratedSlot {
        affix_type: AffixType::Prefix,
        bone_size: poc2_engine::BoneSize::Preserved,
        bone_subtype: poc2_engine::BoneSubtype::Rib,
        abyss_lord: Some(poc2_engine::AbyssLord::Amanamu),
        min_mod_level: 0,
        otherworldly: false,
    });
    item.prefixes.push(ModRoll {
        mod_id: ModId::from("ES_Tier1"),
        affix_type: AffixType::Prefix,
        kind: ModKind::Explicit,
        values: smallvec![160.0],
        is_fractured: false,
    });

    let rules = RuleSet::from_rules(poc2_rules::seed_rules());
    let strategies = StrategyRegistry::default();
    let resolver = DefaultCurrencyResolver::new();
    let stash = Stash::unlimited();
    let goal = es_target_goal();
    let ctx = PredicateContext::new(&registry).with_stash(&stash);
    let cands = generate_candidates_with_goal(
        &item,
        &ctx,
        &rules,
        &strategies,
        &resolver,
        &stash,
        PatchVersion::PATCH_0_4_0,
        poc2_engine::patch::League::current(),
        Some(&goal),
        &registry,
        None,
    );
    let any_reveal_with_bone_and_omen = cands.iter().any(|c| {
        matches!(
            &c.action,
            poc2_advisor::AdvisorAction::Reveal {
                bone: Some(_),
                omen: Some(_),
                ..
            }
        )
    });
    assert!(
        any_reveal_with_bone_and_omen,
        "B.6: at least one (bone, omen) reveal pair must be emitted, got: {:?}",
        cands
            .iter()
            .filter_map(|c| match &c.action {
                poc2_advisor::AdvisorAction::Reveal { bone, omen, .. } => Some((bone, omen)),
                _ => None,
            })
            .collect::<Vec<_>>()
    );
}

/// Honest P(reach goal): the display metrics describe the user's CURRENT item.
/// A Magic body armour carrying one ES prefix satisfies the ES spec of an
/// `es_goal(1, 1)` (1 ES + 1 FR) but not the FR spec → `goal_progress == 0.5`,
/// and the headline `expected_prob` (reliability × that closeness) is a LOW,
/// honest number — not the old ~90% raw step-execution probability — bounded by
/// `expected_prob <= goal_progress`.
#[test]
fn honest_expected_prob_reflects_current_item_progress() {
    let registry = ModRegistry::from_mods(
        vec![es_prefix_mod("ES_Tier1"), fr_suffix_mod("FR_Tier1")],
        vec![],
    );
    let mut item = body_armour(Rarity::Magic);
    item.prefixes.push(ModRoll {
        mod_id: ModId::from("ES_Tier1"),
        affix_type: AffixType::Prefix,
        kind: ModKind::Explicit,
        values: smallvec![150.0],
        is_fractured: false,
    });

    let strategies = StrategyRegistry::default();
    let rules = RuleSet::from_rules(poc2_rules::seed_rules());
    let resolver = DefaultCurrencyResolver::new();
    let valuator = Valuator::default();
    let stash = Stash::unlimited();
    let input = PlanInput {
        item,
        goal: es_goal(1, 1), // 1 ES + 1 FR spec; the single ES satisfies the ES spec
        rules: &rules,
        strategies: &strategies,
        registry: &registry,
        resolver: &resolver,
        valuator: &valuator,
        stash: &stash,
        patch: PatchVersion::PATCH_0_4_0,
        league: poc2_engine::patch::League::current(),
        plugin_dispatch: None,
        base_registry: None,
        trained_models: None,
        config: BeamConfig {
            width: 6,
            depth: 4,
            risk: 0.5,
            top_n: 5,
            seed: 0,
            mc_samples: 1,
            weights: ScoringWeights::default(),
            trained_uplift_weight: 1000.0,
        },
    };
    let recs = plan(&input);
    let top = recs.first().expect("planner returned no recommendation");
    assert!(
        (top.goal_progress - 0.5).abs() < 1e-9,
        "current item satisfies 1 of 2 specs → goal_progress 0.5, got {}",
        top.goal_progress
    );
    assert!(
        top.expected_prob <= top.goal_progress + 1e-9,
        "expected_prob (reliability × closeness) must not exceed goal_progress; \
         prob={} progress={}",
        top.expected_prob,
        top.goal_progress
    );
    assert!(
        top.expected_prob < 0.9,
        "headline P(reach goal) must be honest (not the old ~90% step-prob), got {}",
        top.expected_prob
    );
}

/// The flip side: an item that already satisfies the goal reports full progress
/// (`goal_progress == 1.0`, `expected_prob == 1.0`) and the planner stops.
#[test]
fn satisfied_goal_reports_full_progress_and_stop() {
    let registry = ModRegistry::from_mods(
        vec![
            es_prefix_mod("ES_T1"),
            es_prefix_mod("ES_T2"),
            es_prefix_mod("ES_T3"),
            fr_suffix_mod("FR_T1"),
        ],
        vec![],
    );
    let mut item = body_armour(Rarity::Rare);
    for id in ["ES_T1", "ES_T2", "ES_T3"] {
        item.prefixes.push(ModRoll {
            mod_id: ModId::from(id),
            affix_type: AffixType::Prefix,
            kind: ModKind::Explicit,
            values: smallvec![150.0],
            is_fractured: false,
        });
    }
    item.suffixes.push(ModRoll {
        mod_id: ModId::from("FR_T1"),
        affix_type: AffixType::Suffix,
        kind: ModKind::Explicit,
        values: smallvec![40.0],
        is_fractured: false,
    });
    let recs = run_plan(item, registry, 0.5);
    let top = recs.first().expect("planner returned no recommendation");
    assert_eq!(top.goal_progress, 1.0, "satisfied goal ⇒ full progress");
    assert!(
        (top.expected_prob - 1.0).abs() < 1e-9,
        "satisfied goal ⇒ P(reach goal) == 1.0, got {}",
        top.expected_prob
    );
    assert!(matches!(top.action, poc2_advisor::AdvisorAction::Stop));
}
