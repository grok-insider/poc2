//! Live-bundle end-to-end smoke test.
//!
//! Loads a real on-disk bundle (the one the desktop app would load),
//! builds the engine + base registries from it, constructs a realistic
//! Normal int-armour Body Armour at ilvl 82 with a 3×T1 Energy Shield goal,
//! and runs the advisor planner. Asserts the whole stack works against real
//! data: the bundle loads under the current schema, the planner returns at
//! least one recommendation, and every recommended currency is legal for the
//! item's rarity (no illegal candidate leaks).
//!
//! Skips gracefully (passing) when no bundle is present, so CI without a
//! built bundle stays green. Point it at a specific bundle via the
//! `POC2_BUNDLE` env var; otherwise it tries
//! `~/.config/poc2/bundles/poc2.bundle.json.gz`.

use poc2_advisor::{plan, BeamConfig, Goal, PlanInput};
use poc2_engine::currency::DefaultCurrencyResolver;
use poc2_engine::ids::{BaseTypeId, ConceptId};
use poc2_engine::item::{Item, QualityKind, Rarity};
use poc2_engine::patch::{League, PatchVersion};
use poc2_engine::{BaseRegistry, ModRegistry};
use poc2_market::Valuator;
use poc2_rules::RuleSet;
use poc2_strategies::{StrategyRegistry, Target, TargetSpec};
use smallvec::smallvec;

fn bundle_path() -> Option<std::path::PathBuf> {
    if let Ok(p) = std::env::var("POC2_BUNDLE") {
        let pb = std::path::PathBuf::from(p);
        return pb.exists().then_some(pb);
    }
    let home = std::env::var("HOME").ok()?;
    let pb = std::path::PathBuf::from(home).join(".config/poc2/bundles/poc2.bundle.json.gz");
    pb.exists().then_some(pb)
}

/// Find an int-armour Body Armour base id in the bundle (for an ES craft).
fn pick_int_body_armour(bundle: &poc2_data::Bundle) -> Option<BaseTypeId> {
    bundle
        .base_items
        .iter()
        .find(|b| {
            b.item_class.as_str() == "BodyArmour"
                && b.tags.iter().any(|t| t.as_str() == "int_armour")
        })
        .map(|b| b.id.clone())
}

fn es_prefix_spec() -> TargetSpec {
    TargetSpec {
        concept: Some(ConceptId::from("EnergyShield")),
        concept_any: vec![],
        affix: Some(poc2_engine::item::AffixType::Prefix),
        count: 3,
        min_tier: None,
        allow_hybrid: true,
    }
}

#[test]
fn live_bundle_plans_es_body_armour() {
    let Some(path) = bundle_path() else {
        eprintln!("live_bundle_smoke: no bundle on disk; skipping (set POC2_BUNDLE to run).");
        return;
    };

    let bundle = poc2_data::io::read_bundle(&path)
        .unwrap_or_else(|e| panic!("bundle at {} failed to load: {e}", path.display()));

    // Sanity: this must be a current-schema, real-content bundle.
    assert!(
        bundle.mods.len() > 100,
        "bundle should carry a real mod set; got {}",
        bundle.mods.len()
    );
    let patch = bundle.header.game_patch;

    let base = pick_int_body_armour(&bundle)
        .expect("bundle should contain at least one int-armour Body Armour base");

    // Build registries exactly as the desktop loader does.
    let base_registry = BaseRegistry::from_bases(bundle.base_items.clone());
    let registry = ModRegistry::from_mods(bundle.mods.clone(), bundle.weights.clone());
    let resolver = DefaultCurrencyResolver::new();
    let rules = RuleSet::from_rules(poc2_rules::seed_rules());
    let strategies = StrategyRegistry::default();
    let valuator = Valuator::default();
    let stash = poc2_advisor::Stash::unlimited();

    let item = Item {
        base,
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
    };

    let goal = Goal::new(
        Target {
            prefixes: vec![es_prefix_spec()],
            suffixes: vec![],
            constraints: vec![],
        },
        poc2_market::DivEquiv::point(100.0),
    );

    let input = PlanInput {
        item,
        goal,
        rules: &rules,
        strategies: &strategies,
        registry: &registry,
        resolver: &resolver,
        valuator: &valuator,
        stash: &stash,
        patch,
        league: League::current(),
        plugin_dispatch: None,
        base_registry: Some(&base_registry),
        trained_models: None,
        config: BeamConfig {
            width: 8,
            depth: 3,
            risk: 0.3,
            top_n: 5,
            seed: 0xC0FFEE,
            mc_samples: 20,
            ..BeamConfig::default()
        },
    };

    let recs = plan(&input);
    assert!(
        !recs.is_empty(),
        "planner returned no recommendations for a Normal ilvl-82 ES body-armour goal"
    );

    // Every recommended currency must be legal on a Normal item — the first
    // legal step for a Normal item is Transmutation (no Exalt/Regal/Chaos on
    // Normal). This guards against illegal-candidate leakage end-to-end.
    use poc2_advisor::AdvisorAction;
    for r in &recs {
        if let AdvisorAction::ApplyCurrency { currency, .. } = &r.action {
            let illegal_on_normal = matches!(
                currency.as_str(),
                "ExaltedOrb"
                    | "GreaterExaltedOrb"
                    | "PerfectExaltedOrb"
                    | "RegalOrb"
                    | "GreaterRegalOrb"
                    | "PerfectRegalOrb"
                    | "ChaosOrb"
                    | "GreaterChaosOrb"
                    | "PerfectChaosOrb"
                    | "OrbOfAnnulment"
                    | "OrbOfAugmentation"
            );
            assert!(
                !illegal_on_normal,
                "planner recommended {currency} which is illegal on a Normal item"
            );
        }
    }

    // The Recombinator must never be recommended in the 0.5 challenge league.
    if patch >= PatchVersion::PATCH_0_5_0 {
        for r in &recs {
            assert!(
                !matches!(r.action, AdvisorAction::Recombine { .. }),
                "Recombinator recommended in 0.5 Runes of Aldur (Challenge)"
            );
        }
    }

    eprintln!(
        "live_bundle_smoke: patch {patch}, {} mods, {} weights → {} recommendations. Top: {:?}",
        bundle.mods.len(),
        bundle.weights.len(),
        recs.len(),
        recs.first().map(|r| &r.action),
    );
}

/// Asserts the advisor's top recommendation on a fresh Normal item is a
/// concrete crafting step (an Orb of Transmutation variant), never empty
/// advisory guidance — the bug found during the 0.5 bring-up.
#[test]
fn live_bundle_top_step_on_normal_is_concrete() {
    use poc2_advisor::AdvisorAction;
    let Some(path) = bundle_path() else {
        eprintln!("skipping: no bundle");
        return;
    };
    let bundle = poc2_data::io::read_bundle(&path).unwrap();
    let base = pick_int_body_armour(&bundle).unwrap();
    let base_registry = BaseRegistry::from_bases(bundle.base_items.clone());
    let registry = ModRegistry::from_mods(bundle.mods.clone(), bundle.weights.clone());
    let resolver = DefaultCurrencyResolver::new();
    let rules = RuleSet::from_rules(poc2_rules::seed_rules());
    let strategies = StrategyRegistry::default();
    let valuator = Valuator::default();
    let stash = poc2_advisor::Stash::unlimited();
    let item = Item {
        base,
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
    };
    let goal = Goal::new(
        Target {
            prefixes: vec![es_prefix_spec()],
            suffixes: vec![],
            constraints: vec![],
        },
        poc2_market::DivEquiv::point(100.0),
    );
    let input = PlanInput {
        item,
        goal,
        rules: &rules,
        strategies: &strategies,
        registry: &registry,
        resolver: &resolver,
        valuator: &valuator,
        stash: &stash,
        patch: bundle.header.game_patch,
        league: League::current(),
        plugin_dispatch: None,
        base_registry: Some(&base_registry),
        trained_models: None,
        config: BeamConfig {
            width: 8,
            depth: 3,
            risk: 0.3,
            top_n: 8,
            seed: 1,
            mc_samples: 20,
            ..BeamConfig::default()
        },
    };
    let recs = plan(&input);
    let top = recs.first().expect("expected at least one recommendation");
    match &top.action {
        AdvisorAction::ApplyCurrency { currency, .. } => {
            assert!(
                currency.as_str().contains("Transmutation"),
                "top step on a Normal item should be a Transmutation, got {currency}"
            );
        }
        other => panic!("top recommendation should be a concrete currency step, got {other:?}"),
    }
}
