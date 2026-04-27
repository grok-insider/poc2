//! M15.4 — Cross-source rule + strategy validation.
//!
//! Catches authoring drift between the strategy library, rule
//! catalogue, and engine semantics. Runs in CI; failures surface the
//! offending strategy/rule and the engine's [`CannotApply`] reason
//! (when relevant) so the author can fix the source TOML directly.
//!
//! ## Checks
//!
//! 1. **Strategy currency resolution.** For every TOML strategy, every
//!    `apply_currency` step's `currency` resolves via
//!    [`DefaultCurrencyResolver`].
//! 2. **Rule currency resolution.** Same check for every rule's
//!    `then.action` of kind `apply_currency`.
//! 3. **Currency rarity-gate sanity.** When a strategy step or rule is
//!    paired with a strategy/target rarity hint, the engine's
//!    [`Currency::can_apply_to`] is called against a synthetic
//!    matching item; assert it returns `Ok` (i.e., the rarity gate
//!    accepts the synthesized item).
//! 4. **Currency id presence.** Every `apply_currency` action's
//!    currency is recognized by `DefaultCurrencyResolver` regardless
//!    of whether it carries omens.
//!
//! Reference: `docs/81-engine-training-and-rule-encoding-plan.md` §5.4
//! Tier 2.4.

use std::path::PathBuf;

use poc2_engine::currency::{CurrencyResolver, DefaultCurrencyResolver};
use poc2_engine::ids::{BaseTypeId, CurrencyId, OmenId};
use poc2_engine::item::{Item, QualityKind, Rarity};
use poc2_rules::seed_rules;
use poc2_rules::SuggestionAction;
use poc2_strategies::{load_strategy_toml, Action, Strategy};
use smallvec::smallvec;

/// Build a synthetic item with the given rarity + class. The placeholder
/// `Item.base = class_id` mirrors the v3 transitional convention used
/// by every fixture in the workspace.
fn synth_item(rarity: Rarity, class: &str) -> Item {
    Item {
        base: BaseTypeId::from(class),
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

/// Classes the can_apply_to probe sweeps. Covers the dominant gear
/// categories so class-gated currencies (Catalyst, Bone) find at least
/// one accepting class.
const CLASS_PROBE: &[&str] = &[
    "BodyArmour",
    "Helmet",
    "Boots",
    "Gloves",
    "Ring",
    "Amulet",
    "Belt",
    "Jewel",
    "Talisman",
    "OneHandSword",
    "Bow",
    "Quiver",
    "Focus",
];

/// Currencies with strong item-state preconditions that the synthetic
/// item can't satisfy (e.g., FracturingOrb requires ≥4 visible mods).
/// The can_apply_to probe skips these — their preconditions are
/// validated by dedicated tests in `engine/tests/`.
const SKIP_CAN_APPLY_PROBE: &[&str] = &[
    "FracturingOrb", // requires ≥4 visible mods
    "HinekorasLock", // already-locked check; synth item state-dependent
    "ArchitectsOrb", // double-corruption helper not yet implemented
];

/// Best-effort: pick the strategy's primary item class for the
/// can_apply_to probe. Falls back to `BodyArmour` when no class hint
/// is declared.
fn class_hint_for_strategy(strategy: &Strategy) -> Vec<String> {
    if strategy.item_classes.is_empty() {
        CLASS_PROBE.iter().map(|s| (*s).to_string()).collect()
    } else {
        strategy
            .item_classes
            .iter()
            .map(|c| c.as_str().to_string())
            .collect()
    }
}

fn load_all_seed_strategies() -> Vec<Strategy> {
    // CARGO_MANIFEST_DIR resolves to `crates/advisor` for this test.
    // Strategies live at `crates/strategies/strategies/*.toml`; the
    // canonical relative path from the advisor crate root is one level
    // up + into the strategies crate's strategies/ subdir.
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let dir = manifest.join("..").join("strategies").join("strategies");
    let mut out = Vec::new();
    let entries = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("read_dir({}) failed: {e}", dir.display()));
    for entry in entries {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("toml") {
            continue;
        }
        let strategy = load_strategy_toml(&path)
            .unwrap_or_else(|e| panic!("strategy {} failed to load: {e}", path.display()));
        out.push(strategy);
    }
    out
}

fn unresolved_currency(resolver: &DefaultCurrencyResolver, currency: &CurrencyId) -> bool {
    // Catalog-derived currencies (essences, certain catalysts) may be
    // unresolved without a populated bundle. Filter those out so the
    // CI test focuses on engine-defined ids that are *expected* to
    // resolve out of the box.
    //
    // Currencies on the explicit deferred-implementation list (e.g.,
    // ArchitectsOrb for double corruption) are also skipped — they're
    // referenced in aspirational strategies whose ship-prep is
    // tracked separately.
    let s = currency.as_str();
    let is_essence = s.contains("Essence");
    let is_deferred_impl = SKIP_CAN_APPLY_PROBE.contains(&s);
    let is_resolved = resolver.resolve(currency).is_some();
    !is_resolved && !is_essence && !is_deferred_impl
}

#[test]
fn every_seed_strategy_step_resolves_via_default_resolver() {
    let strategies = load_all_seed_strategies();
    let resolver = DefaultCurrencyResolver::new();
    let mut failures: Vec<String> = Vec::new();
    for strategy in &strategies {
        for step in &strategy.steps {
            if let Action::ApplyCurrency { currency, .. } = &step.action {
                if unresolved_currency(&resolver, currency) {
                    failures.push(format!(
                        "strategy `{}` step `{}` references unknown currency `{}`",
                        strategy.id.0, step.id.0, currency
                    ));
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "cross-source validation failures:\n{}",
        failures.join("\n")
    );
}

#[test]
fn every_seed_rule_action_resolves_via_default_resolver() {
    let rules = seed_rules();
    let resolver = DefaultCurrencyResolver::new();
    let mut failures: Vec<String> = Vec::new();
    for rule in &rules {
        for suggestion in &rule.then {
            if let SuggestionAction::ApplyCurrency { currency, .. } = &suggestion.action {
                if unresolved_currency(&resolver, currency) {
                    failures.push(format!(
                        "rule `{}` references unknown currency `{}`",
                        rule.id.0, currency
                    ));
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "cross-source validation failures:\n{}",
        failures.join("\n")
    );
}

#[test]
fn every_strategy_step_currency_can_apply_to_at_some_rarity_and_class() {
    // For each apply_currency step, `Currency::can_apply_to` should
    // return Ok for at least ONE (rarity × class) combination drawn
    // from the strategy's `item_classes` hint (or the full CLASS_PROBE
    // when no hint is declared). Currencies on the SKIP_CAN_APPLY_PROBE
    // list are exempt — their preconditions need richer item state
    // than this synthetic check supplies.
    let strategies = load_all_seed_strategies();
    let resolver = DefaultCurrencyResolver::new();
    let mut failures: Vec<String> = Vec::new();
    for strategy in &strategies {
        let classes = class_hint_for_strategy(strategy);
        for step in &strategy.steps {
            let Action::ApplyCurrency { currency, .. } = &step.action else {
                continue;
            };
            if SKIP_CAN_APPLY_PROBE.contains(&currency.as_str()) {
                continue;
            }
            let Some(c) = resolver.resolve(currency) else {
                continue;
            };
            let accepted_any = classes.iter().any(|class| {
                [Rarity::Normal, Rarity::Magic, Rarity::Rare, Rarity::Unique]
                    .into_iter()
                    .any(|r| c.can_apply_to(&synth_item(r, class)).is_ok())
            });
            if !accepted_any {
                failures.push(format!(
                    "strategy `{}` step `{}`: currency `{}` rejects every \
                     (rarity × class) combination across {classes:?}",
                    strategy.id.0, step.id.0, currency
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "cross-source validation failures:\n{}",
        failures.join("\n")
    );
}

#[test]
fn rule_apply_currency_actions_can_apply_to_at_some_rarity_and_class() {
    let rules = seed_rules();
    let resolver = DefaultCurrencyResolver::new();
    let mut failures: Vec<String> = Vec::new();
    for rule in &rules {
        for suggestion in &rule.then {
            let SuggestionAction::ApplyCurrency { currency, .. } = &suggestion.action else {
                continue;
            };
            if SKIP_CAN_APPLY_PROBE.contains(&currency.as_str()) {
                continue;
            }
            let Some(c) = resolver.resolve(currency) else {
                continue;
            };
            let accepted_any = CLASS_PROBE.iter().any(|class| {
                [Rarity::Normal, Rarity::Magic, Rarity::Rare, Rarity::Unique]
                    .into_iter()
                    .any(|r| c.can_apply_to(&synth_item(r, class)).is_ok())
            });
            if !accepted_any {
                failures.push(format!(
                    "rule `{}`: currency `{}` rejects every (rarity × class)",
                    rule.id.0, currency
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "cross-source validation failures:\n{}",
        failures.join("\n")
    );
}

#[test]
fn strategy_omen_ids_have_canonical_form() {
    // Sanity: every omen id in a strategy step looks like
    // `Omen…` — the canonical PoE2 omen-name prefix. Catches typos
    // before they become silent no-ops at apply time.
    let strategies = load_all_seed_strategies();
    let mut failures: Vec<String> = Vec::new();
    for strategy in &strategies {
        for step in &strategy.steps {
            let omens: &[OmenId] = match &step.action {
                Action::ApplyCurrency { omens, .. } => omens.as_slice(),
                Action::ActivateOmen { omen } => std::slice::from_ref(omen),
                _ => continue,
            };
            for omen in omens {
                let s = omen.as_str();
                if !s.starts_with("Omen") {
                    failures.push(format!(
                        "strategy `{}` step `{}`: omen id `{s}` should start with \"Omen\"",
                        strategy.id.0, step.id.0
                    ));
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "cross-source validation failures:\n{}",
        failures.join("\n")
    );
}

#[test]
fn strategy_step_ids_referenced_by_branches_exist() {
    // Sanity: every `on_success` / `on_failure` step id points to an
    // actual step in the same strategy. Dangling references would
    // crash the executor at runtime; this test catches them at CI.
    let strategies = load_all_seed_strategies();
    let mut failures: Vec<String> = Vec::new();
    for strategy in &strategies {
        let known_ids: std::collections::HashSet<&str> =
            strategy.steps.iter().map(|s| s.id.0.as_str()).collect();
        for step in &strategy.steps {
            for branch in [&step.on_success, &step.on_failure].into_iter().flatten() {
                if !known_ids.contains(branch.0.as_str()) {
                    failures.push(format!(
                        "strategy `{}` step `{}` references unknown step `{}`",
                        strategy.id.0, step.id.0, branch.0
                    ));
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "cross-source validation failures:\n{}",
        failures.join("\n")
    );
}

#[test]
fn class_hint_helper_is_self_consistent() {
    let strategies = load_all_seed_strategies();
    for strategy in &strategies {
        let classes = class_hint_for_strategy(strategy);
        assert!(
            !classes.is_empty(),
            "strategy `{}`: class hint helper returned empty list",
            strategy.id.0
        );
    }
}
