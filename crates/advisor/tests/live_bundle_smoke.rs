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
    // The bundle shipped with the web app (committed) — makes these smoke
    // tests run everywhere, including CI.
    let shipped = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../apps/web/public/poc2.bundle.json.gz");
    if shipped.exists() {
        return Some(shipped);
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

/// 0.5 Distilled Emotions end-to-end: a Rare Ruby jewel with a goal
/// matching an emotion's granted mod must surface that emotion as a
/// candidate recommendation (base-targeted alloys resolve through the
/// bundle's real base names — the M14 audit fix, proven on live data).
#[test]
fn live_bundle_proposes_emotion_on_matching_jewel_base() {
    use poc2_advisor::AdvisorAction;
    let Some(path) = bundle_path() else {
        eprintln!("skipping: no bundle");
        return;
    };
    let bundle = poc2_data::io::read_bundle(&path).unwrap();
    if bundle.header.game_patch < PatchVersion::PATCH_0_5_0 || bundle.emotions.entries.is_empty() {
        eprintln!("skipping: bundle has no 0.5 emotions");
        return;
    }

    // A Ruby-targeting emotion + its granted mod, straight off the bundle
    // (the emotions section stores raw JSON entries).
    let ruby_target = |e: &serde_json::Value| -> Option<(String, poc2_engine::ids::ModId)> {
        let id = e.get("id")?.as_str()?;
        let targets = e.get("targets")?.as_array()?;
        let t = targets.iter().find(|t| {
            t.get("base")
                .and_then(|b| b.as_str())
                .is_some_and(|b| b.eq_ignore_ascii_case("Ruby"))
                && t.get("engine_mod_id").and_then(|m| m.as_str()).is_some()
        })?;
        let m = t.get("engine_mod_id")?.as_str()?;
        Some((id.to_string(), poc2_engine::ids::ModId::from(m)))
    };
    let (emotion_id, target_mod) = bundle
        .emotions
        .entries
        .iter()
        .find_map(ruby_target)
        .expect("bundle should carry at least one Ruby-targeting emotion");

    let base = bundle
        .base_items
        .iter()
        .find(|b| b.name.eq_ignore_ascii_case("Ruby") && b.item_class.as_str() == "Jewel")
        .map(|b| b.id.clone())
        .expect("bundle should carry the Ruby jewel base");

    let base_registry = BaseRegistry::from_bases(bundle.base_items.clone());
    let registry = ModRegistry::from_mods(bundle.mods.clone(), bundle.weights.clone());
    // Resolver mirrors the WASM engine: emotions ride the alloy slot.
    let mut alloy_likes = bundle.alloy_catalogue();
    alloy_likes.extend(bundle.emotion_catalogue());
    let resolver = DefaultCurrencyResolver::new().with_alloys(alloy_likes);
    let rules = RuleSet::from_rules(poc2_rules::seed_rules());
    let strategies = StrategyRegistry::default();
    let valuator = Valuator::default();
    let stash = poc2_advisor::Stash::unlimited();

    // Goal concept = whatever the emotion's granted mod actually produces.
    let wanted_concept = registry
        .get(&target_mod)
        .and_then(|def| def.concept_set.first().cloned())
        .expect("emotion target mod should exist in the registry with a concept");
    let target_affix = registry.get(&target_mod).map(|d| d.affix_type);

    // A Rare Ruby with one removable junk mod (a different group so the
    // occupied-group skip doesn't fire).
    let junk = bundle
        .emotions
        .entries
        .iter()
        .filter_map(|e| e.get("targets").and_then(|t| t.as_array()))
        .flatten()
        .filter_map(|t| t.get("engine_mod_id").and_then(|m| m.as_str()))
        .map(poc2_engine::ids::ModId::from)
        .find(|m| {
            *m != target_mod
                && registry.get(m).is_some_and(|d| {
                    registry.group_of(m) != registry.group_of(&target_mod)
                        && d.affix_type == poc2_engine::AffixType::Suffix
                })
        });
    let mut item = Item {
        base,
        ilvl: 80,
        rarity: Rarity::Rare,
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
    let junk_id = junk.unwrap_or_else(|| poc2_engine::ids::ModId::from("JewelJunkPlaceholder"));
    item.suffixes.push(poc2_engine::ModRoll {
        mod_id: junk_id,
        affix_type: poc2_engine::AffixType::Suffix,
        kind: poc2_engine::ModKind::Explicit,
        values: smallvec![1.0],
        is_fractured: false,
    });

    let spec = TargetSpec {
        concept: Some(wanted_concept),
        concept_any: vec![],
        affix: None,
        count: 1,
        min_tier: None,
        allow_hybrid: true,
    };
    let (prefixes, suffixes) = match target_affix {
        Some(poc2_engine::AffixType::Suffix) => (vec![], vec![spec]),
        _ => (vec![spec], vec![]),
    };
    let goal = Goal::new(
        Target {
            prefixes,
            suffixes,
            constraints: vec![],
        },
        poc2_market::DivEquiv::point(50.0),
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
            width: 16,
            depth: 1,
            risk: 0.3,
            top_n: 16,
            seed: 7,
            mc_samples: 5,
            ..BeamConfig::default()
        },
    };

    let recs = plan(&input);
    let proposed: Vec<_> = recs
        .iter()
        .filter_map(|r| match &r.action {
            AdvisorAction::ApplyCurrency { currency, .. } => Some(currency.as_str().to_string()),
            _ => None,
        })
        .collect();
    assert!(
        proposed.contains(&emotion_id),
        "expected emotion {emotion_id:?} among recommendations for a Rare Ruby; got {proposed:?}"
    );
}
