//! M16.5 — Imitation seeding from the strategy library.
//!
//! Pre-loads the [`TableModelBuilder`] with expert trajectories so the
//! standard learner converges faster on canonical chains. Per Britz's
//! "compounding error problem" — cold-start from random rollouts wastes
//! compute on states a real player would never reach. Strategies
//! encode *exactly* the trajectories the user takes.
//!
//! ## Algorithm
//!
//! For each strategy `s` whose `target` matches the training goal:
//! 1. Run `s.dry_run(initial_item, registry, max_steps)` to enumerate
//!    the strategy's intended action sequence.
//! 2. For each `dry_run` step, lift its `Action` into an
//!    [`AdvisorAction`].
//! 3. Simulate the action against the engine `n_rollouts` times,
//!    accumulating `(state, action, next_state)` observations. Each
//!    observation contributes `imitation_weight × 1` counts to the
//!    builder — i.e., the imitation prior is treated as if it were
//!    `imitation_weight` "extra" Monte Carlo samples.
//! 4. The simulated next-state becomes the input for the next step.
//!
//! After seeding, the standard [`crate::training::learn_transition_model`]
//! run fills in the off-trajectory states by random exploration.
//!
//! Reference: `docs/81-engine-training-and-rule-encoding-plan.md` §6.5
//! Tier 3.5.

use poc2_engine::base_registry::BaseRegistry;
use poc2_engine::currency::CurrencyResolver;
use poc2_engine::item::Item;
use poc2_engine::omen::OmenSet;
use poc2_engine::patch::PatchVersion;
use poc2_engine::registry::ModRegistry;
use poc2_strategies::{dry_run, Action as StrategyAction, Strategy, TerminalKind};
use rand::{RngCore, SeedableRng};
use rand_xoshiro::Xoshiro256PlusPlus;

use crate::action::AdvisorAction;
use crate::featurize::featurize;
use crate::goal::Goal;
use crate::simulator::simulate;
use crate::training::model_learner::{StateActionAlias, TableModelBuilder};

/// Configuration for imitation seeding.
#[derive(Debug, Clone, Copy)]
pub struct ImitationConfig {
    /// Per-step rollout count. Each `(state, action)` along the strategy's
    /// trajectory gets `n_rollouts` simulator runs.
    pub n_rollouts: u32,
    /// Multiplier on each observation. Defaults to `10` per Britz —
    /// imitation observations are weighted 10× heavier than random
    /// rollouts so the learner trusts expert demonstrations more.
    pub imitation_weight: u64,
    /// Strategy traversal cap. Strategies typically have 8–12 steps;
    /// cap at 32 to handle complex graphs without infinite loops.
    pub max_strategy_steps: u32,
    /// Random seed for reproducibility.
    pub seed: u64,
}

impl Default for ImitationConfig {
    fn default() -> Self {
        Self {
            n_rollouts: 1_000,
            imitation_weight: 10,
            max_strategy_steps: 32,
            seed: 0x_5EED_C0DE,
        }
    }
}

/// Seed `builder` with imitation observations drawn from `strategies`.
///
/// Returns the number of `(state, action)` pairs seeded, useful for
/// telemetry. Strategies whose `target` doesn't match `goal` are
/// skipped — comparison is conservative (full Target equality).
///
/// `lift_action` is a caller-supplied helper that converts the strategy
/// DSL's [`StrategyAction`] into the advisor's [`AdvisorAction`]. v3
/// uses [`crate::from_strategy_action`] but the function takes the
/// helper as a parameter so tests can stub.
#[allow(clippy::too_many_arguments)] // mirrors model-learner inputs + strategy slice
pub fn seed_from_strategies(
    builder: &mut TableModelBuilder,
    strategies: &[&Strategy],
    initial_item: &Item,
    goal: &Goal,
    registry: &ModRegistry,
    base_registry: &BaseRegistry,
    resolver: &dyn CurrencyResolver,
    patch: PatchVersion,
    omens: &OmenSet,
    config: ImitationConfig,
) -> u32 {
    let _ = base_registry; // reserved for future class-aware filtering
    let mut seeded_pairs = 0u32;
    let mut rng = Xoshiro256PlusPlus::seed_from_u64(config.seed);

    for strategy in strategies {
        if !strategy_target_matches_goal(strategy, goal) {
            continue;
        }
        let trace = dry_run(strategy, initial_item, registry, config.max_strategy_steps);
        let mut current_item = initial_item.clone();
        for step in &trace {
            // Skip terminal-only frames (Done / Abandon / Dangling).
            if !matches!(step.terminal, TerminalKind::None) {
                break;
            }
            let Some(strategy_action) = step.action else {
                continue;
            };
            // Lift the strategy DSL's Action into AdvisorAction.
            let Some(advisor_action) = lift_strategy_action(strategy_action) else {
                continue;
            };
            let from_features = featurize(&current_item, goal, registry);
            let alias = StateActionAlias::from(from_features, &advisor_action, true);

            // Run n_rollouts simulations to capture the next-state
            // distribution under this strategy step.
            for _ in 0..config.n_rollouts {
                let seed = rng.next_u64();
                let outcome = simulate(
                    &current_item,
                    &advisor_action,
                    omens,
                    registry,
                    resolver,
                    patch,
                    seed,
                );
                let next_features = featurize(&outcome.item, goal, registry);
                builder.add(alias.clone(), next_features, config.imitation_weight);
            }
            seeded_pairs += 1;

            // Advance the strategy by one *primary*-flavoured next-state
            // sample so subsequent iterations see a representative
            // post-step item.
            let advance_seed = rng.next_u64();
            let advance = simulate(
                &current_item,
                &advisor_action,
                omens,
                registry,
                resolver,
                patch,
                advance_seed,
            );
            current_item = advance.item;
        }
    }
    seeded_pairs
}

/// Lift a strategy DSL action into an [`AdvisorAction`]. Returns `None`
/// for terminal actions (`Done` / `Abandon`), reveal/recombine (their
/// AdvisorAction equivalents carry richer reveal-pool / stash-id data
/// than the strategy DSL emits, so the imitation seeder skips them and
/// the standard learner fills these states in via random exploration),
/// and any future strategy variants the advisor doesn't model yet.
#[must_use]
pub fn lift_strategy_action(action: &StrategyAction) -> Option<AdvisorAction> {
    match action {
        StrategyAction::ApplyCurrency { currency, omens } => Some(AdvisorAction::ApplyCurrency {
            currency: currency.clone(),
            omens: omens.clone(),
        }),
        StrategyAction::ActivateOmen { omen } => {
            Some(AdvisorAction::ActivateOmen { omen: omen.clone() })
        }
        // Reveal / Recombine / Done / Abandon: skip — the standard
        // model learner discovers these states via random exploration
        // post-imitation. Future tiers can refine the lifter to include
        // them with bridging logic for the extra fields.
        _ => None,
    }
}

/// Conservative target equality. Two targets match iff their prefixes,
/// suffixes, and constraints all match by `PartialEq`.
fn strategy_target_matches_goal(strategy: &Strategy, goal: &Goal) -> bool {
    strategy.target == goal.target
}

#[cfg(test)]
mod tests {
    use super::*;
    use poc2_engine::currency::DefaultCurrencyResolver;
    use poc2_engine::ids::{
        BaseTypeId, ConceptId, CurrencyId, ItemClassId, ModGroupId, ModId, StatId, TagId,
    };
    use poc2_engine::item::{AffixType, QualityKind, Rarity};
    use poc2_engine::mods::{
        ModDefinition, ModDomain, ModFlags, ModGroup, ModKind, ModStat, SpawnWeight,
    };
    use poc2_engine::patch::PatchRange;
    use poc2_market::DivEquiv;
    use poc2_strategies::{Action, Source, Step, StepId, Strategy, StrategyId, Target, TargetSpec};
    use smallvec::smallvec;

    fn mk_es_mod(id: &str) -> ModDefinition {
        ModDefinition {
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
                max: 80.0,
            }],
            required_level: 1,
            allowed_item_classes: smallvec![ItemClassId::from("BodyArmour")],
            patch_range: PatchRange::ALL,
            flags: ModFlags::empty(),
            text_template: None,
        }
    }

    fn es_target() -> Target {
        Target {
            prefixes: vec![TargetSpec {
                concept: Some(ConceptId::from("EnergyShield")),
                concept_any: vec![],
                affix: None,
                count: 1,
                min_tier: None,
                allow_hybrid: true,
            }],
            suffixes: vec![],
            constraints: vec![],
        }
    }

    fn mk_normal_armour() -> Item {
        Item {
            base: BaseTypeId::from("BodyArmour"),
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

    fn one_step_transmute_strategy() -> Strategy {
        Strategy {
            id: StrategyId::from("imitation-test"),
            name: "imitation-test".into(),
            source: Source::default(),
            patch_min: Some(PatchVersion::PATCH_0_4_0),
            patch_max: None,
            item_classes: vec![],
            attribute_pools: vec![],
            preconditions: vec![],
            target: es_target(),
            abandon_criteria: vec![],
            steps: vec![
                Step {
                    id: StepId::from("S1"),
                    action: Action::ApplyCurrency {
                        currency: CurrencyId::from("OrbOfTransmutation"),
                        omens: vec![],
                    },
                    target_check: None,
                    on_success: Some(StepId::from("S2")),
                    on_failure: None,
                    recovery: smallvec![],
                    note: None,
                },
                Step {
                    id: StepId::from("S2"),
                    action: Action::Done,
                    target_check: None,
                    on_success: None,
                    on_failure: None,
                    recovery: smallvec![],
                    note: None,
                },
            ],
            expected_cost_div: None,
            expected_success_prob: None,
            confidence: poc2_strategies::Confidence::Experimental,
            note: None,
        }
    }

    #[test]
    fn lift_strategy_action_passes_through_apply_currency() {
        let action = Action::ApplyCurrency {
            currency: CurrencyId::from("OrbOfTransmutation"),
            omens: vec![],
        };
        let lifted = lift_strategy_action(&action).unwrap();
        match lifted {
            AdvisorAction::ApplyCurrency { currency, .. } => {
                assert_eq!(currency.as_str(), "OrbOfTransmutation");
            }
            _ => panic!("expected ApplyCurrency"),
        }
    }

    #[test]
    fn lift_strategy_action_returns_none_on_terminal() {
        assert!(lift_strategy_action(&Action::Done).is_none());
        assert!(lift_strategy_action(&Action::Abandon {
            reason: "test".into()
        })
        .is_none());
    }

    #[test]
    fn seed_from_strategies_records_observations_for_matching_strategy() {
        let registry = ModRegistry::from_mods(vec![mk_es_mod("ES1")], vec![]);
        let base_registry = BaseRegistry::default();
        let resolver = DefaultCurrencyResolver::new();
        let goal = Goal::new(es_target(), DivEquiv::point(100.0));
        let strategy = one_step_transmute_strategy();
        let initial = mk_normal_armour();
        let mut builder = TableModelBuilder::new();
        let config = ImitationConfig {
            n_rollouts: 50,
            imitation_weight: 10,
            max_strategy_steps: 8,
            seed: 1,
        };

        let n_pairs = seed_from_strategies(
            &mut builder,
            &[&strategy],
            &initial,
            &goal,
            &registry,
            &base_registry,
            &resolver,
            PatchVersion::PATCH_0_4_0,
            &OmenSet::new(),
            config,
        );

        assert!(n_pairs >= 1, "expected at least one seeded pair");
        assert!(builder.entry_count() >= 1);
    }

    #[test]
    fn seed_from_strategies_skips_target_mismatch() {
        let registry = ModRegistry::from_mods(vec![], vec![]);
        let base_registry = BaseRegistry::default();
        let resolver = DefaultCurrencyResolver::new();
        let initial = mk_normal_armour();
        // Goal targets Life, strategy targets ES → no match.
        let goal = Goal::new(
            Target {
                prefixes: vec![TargetSpec {
                    concept: Some(ConceptId::from("Life")),
                    concept_any: vec![],
                    affix: None,
                    count: 1,
                    min_tier: None,
                    allow_hybrid: true,
                }],
                suffixes: vec![],
                constraints: vec![],
            },
            DivEquiv::point(100.0),
        );
        let strategy = one_step_transmute_strategy();
        let mut builder = TableModelBuilder::new();
        let n_pairs = seed_from_strategies(
            &mut builder,
            &[&strategy],
            &initial,
            &goal,
            &registry,
            &base_registry,
            &resolver,
            PatchVersion::PATCH_0_4_0,
            &OmenSet::new(),
            ImitationConfig::default(),
        );
        assert_eq!(n_pairs, 0, "mismatched target should produce no pairs");
        assert_eq!(builder.entry_count(), 0);
    }
}
