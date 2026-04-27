//! Phase A regression: the planner must never recommend a currency that
//! the engine would reject because of rarity/state preconditions, and
//! it must never recommend `Orb of Alchemy` (per docs/80-crafter-helper-v2-plan.md
//! §4 — Alchemy generates uncontrolled randomness and the controlled
//! Trans → Aug → Regal/Essence chain is strictly preferred).

use poc2_advisor::{plan, BeamConfig, Goal, PlanInput, ScoringWeights, Stash};
use poc2_engine::currency::DefaultCurrencyResolver;
use poc2_engine::ids::{ConceptId, ItemClassId};
use poc2_engine::item::{Item, QualityKind, Rarity};
use poc2_engine::patch::PatchVersion;
use poc2_engine::registry::ModRegistry;
use poc2_market::{DivEquiv, Valuator};
use poc2_rules::RuleSet;
use poc2_strategies::StrategyRegistry;
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

fn worked_example_goal() -> Goal {
    use poc2_strategies::TargetSpec;
    Goal {
        target: poc2_strategies::Target {
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

fn run_plan(item: Item) -> Vec<poc2_advisor::Recommendation> {
    let registry = ModRegistry::from_mods(Vec::new());
    let strategies = StrategyRegistry::default();
    let rules = RuleSet::from_rules(poc2_rules::seed_rules());
    let resolver = DefaultCurrencyResolver::new();
    let valuator = Valuator::default();
    let stash = Stash::unlimited();

    let input = PlanInput {
        item,
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
            width: 5,
            depth: 3,
            risk: 0.5,
            top_n: 5,
            seed: 0,
            mc_samples: 1,
            weights: ScoringWeights::default(),
        },
    };
    plan(&input)
}

fn currency_id(rec: &poc2_advisor::Recommendation) -> Option<&str> {
    if let poc2_advisor::AdvisorAction::ApplyCurrency { currency, .. } = &rec.action {
        Some(currency.as_str())
    } else {
        None
    }
}

#[test]
fn normal_item_never_sees_exalt_chaos_aug_regal_annul() {
    let recs = run_plan(body_armour(Rarity::Normal));
    for rec in &recs {
        let Some(c) = currency_id(rec) else { continue };
        let illegal = matches!(
            c,
            "ExaltedOrb"
                | "GreaterExaltedOrb"
                | "PerfectExaltedOrb"
                | "ChaosOrb"
                | "GreaterChaosOrb"
                | "PerfectChaosOrb"
                | "OrbOfAugmentation"
                | "GreaterOrbOfAugmentation"
                | "PerfectOrbOfAugmentation"
                | "RegalOrb"
                | "GreaterRegalOrb"
                | "PerfectRegalOrb"
                | "OrbOfAnnulment"
        );
        assert!(!illegal, "Normal item must not recommend {c}");
    }
}

#[test]
fn magic_item_never_sees_transmute_or_exalt_or_chaos() {
    let recs = run_plan(body_armour(Rarity::Magic));
    for rec in &recs {
        let Some(c) = currency_id(rec) else { continue };
        let illegal = matches!(
            c,
            "OrbOfTransmutation"
                | "GreaterOrbOfTransmutation"
                | "PerfectOrbOfTransmutation"
                | "ExaltedOrb"
                | "GreaterExaltedOrb"
                | "PerfectExaltedOrb"
                | "ChaosOrb"
                | "GreaterChaosOrb"
                | "PerfectChaosOrb"
        );
        assert!(!illegal, "Magic item must not recommend {c}");
    }
}

#[test]
fn rare_item_never_sees_transmute_or_aug_or_regal() {
    let recs = run_plan(body_armour(Rarity::Rare));
    for rec in &recs {
        let Some(c) = currency_id(rec) else { continue };
        let illegal = matches!(
            c,
            "OrbOfTransmutation"
                | "GreaterOrbOfTransmutation"
                | "PerfectOrbOfTransmutation"
                | "OrbOfAugmentation"
                | "GreaterOrbOfAugmentation"
                | "PerfectOrbOfAugmentation"
                | "RegalOrb"
                | "GreaterRegalOrb"
                | "PerfectRegalOrb"
        );
        assert!(!illegal, "Rare item must not recommend {c}");
    }
}

#[test]
fn alchemy_is_never_recommended_at_any_rarity() {
    for rarity in [Rarity::Normal, Rarity::Magic, Rarity::Rare] {
        let recs = run_plan(body_armour(rarity));
        for rec in &recs {
            let Some(c) = currency_id(rec) else { continue };
            assert_ne!(
                c, "OrbOfAlchemy",
                "Alchemy must never be recommended ({rarity:?})",
            );
        }
    }
}
