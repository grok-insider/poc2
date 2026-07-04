//! Goal solving — the one-call "build the exact transition model, run
//! value iteration twice, package a [`TrainedModel`] pair" recipe.
//!
//! Extracted from the `train-advisor` binary (ADR-0015) so the SAME
//! solver runs in two places:
//!
//! - **Offline** (`train-advisor`): the corpus precompute that ships
//!   `/trained-models.json` as a warm-start artefact.
//! - **On demand** (the WASM engine): when a `recommend` call misses the
//!   trained-model cache for the user's `(goal, item-class)`, the engine
//!   solves the goal right there (hundreds of milliseconds at the
//!   [`SolveProfile::on_demand`] budget) and caches the result — every
//!   user goal gets an exact policy, not just the curated corpus.
//!
//! The solve is deterministic (fixed seed; the analytic transition
//! builder is seed-independent outside the MC fallback) and pure CPU.

use poc2_engine::currency::Essence;
use poc2_engine::ids::{CurrencyId, ItemClassId};
use poc2_engine::registry::ModRegistry;

use crate::action::AdvisorAction;
use crate::featurize::{featurize, FeatureVec};
use crate::goal::Goal;
use crate::training::analytic_model::{learn_transition_model_analytic, AnalyticConfig};
use crate::training::hybrid::{goal_hash, trained_model_from, RewardKind, TrainedModel};
use crate::training::model_learner::CraftingTask;
use crate::training::value_iteration::{value_iteration, ValueIterationConfig};

/// Canonical training seed (mirrors the historical `train-advisor`
/// constant; only the MC fallback consumes it).
pub const SOLVE_SEED: u64 = 0x_5EED_C0DE_C0DE_5EED;

/// Budget knobs for one [`solve_goal`] run.
#[derive(Debug, Clone, Copy)]
pub struct SolveProfile {
    /// BFS state cap (truncation beyond it).
    pub max_states: u32,
    /// Per-state action-list cap.
    pub max_actions_per_state: u32,
    /// Monte Carlo samples for actions without a closed form (essences,
    /// omen-conditioned applies).
    pub mc_fallback_samples: u32,
    /// Afterstate aliasing (see `StateActionAlias`).
    pub afterstate_aliasing: bool,
}

impl SolveProfile {
    /// The offline corpus-precompute budget (`train-advisor` defaults).
    #[must_use]
    pub fn offline(max_states: u32, mc_fallback_samples: u32) -> Self {
        Self {
            max_states,
            max_actions_per_state: 32,
            mc_fallback_samples,
            afterstate_aliasing: true,
        }
    }

    /// The plan-time budget: smaller BFS reach + fallback sampling so a
    /// cache-miss solve stays in the hundreds-of-milliseconds range
    /// inside the engine worker (never on the UI thread).
    #[must_use]
    pub fn on_demand() -> Self {
        Self {
            max_states: 2_000,
            max_actions_per_state: 24,
            mc_fallback_samples: 2_000,
            afterstate_aliasing: true,
        }
    }
}

/// Output of [`solve_goal`]: the two reward models plus diagnostics.
#[derive(Debug, Clone)]
pub struct SolvedGoal {
    /// Path-length-reward model (the canonical cache entry).
    pub path: TrainedModel,
    /// Cost-reward model (the risk-slider blend twin).
    pub cost: TrainedModel,
    pub metrics: SolveMetrics,
}

/// Diagnostics from one solve.
#[derive(Debug, Clone, Copy)]
pub struct SolveMetrics {
    pub states_visited: usize,
    pub vi_iterations_path: u32,
    pub vi_iterations_cost: u32,
    /// `V_path(s0)` — the expected steps-to-goal from the task's initial
    /// item (negative; `-1000` = terminal unreachable at this budget).
    pub v_path_s0: f64,
    pub v_cost_s0: f64,
}

/// Solve one crafting goal: analytic transition model → two Bellman
/// solves → packaged [`TrainedModel`] pair keyed on
/// `(goal_hash(task.goal), item_class)`.
#[must_use]
pub fn solve_goal(
    task: &CraftingTask<'_>,
    item_class: &ItemClassId,
    actions: &[AdvisorAction],
    profile: SolveProfile,
) -> SolvedGoal {
    let config = AnalyticConfig {
        afterstate_aliasing: profile.afterstate_aliasing,
        seed: SOLVE_SEED,
        max_states: profile.max_states,
        max_actions_per_state: profile.max_actions_per_state,
        mc_fallback_samples: profile.mc_fallback_samples,
    };
    let actions_vec = actions.to_vec();
    let model =
        learn_transition_model_analytic(task, config, move |_item, _goal| actions_vec.clone());

    let terminal = terminal_predicate(&task.goal);
    let vi_config = ValueIterationConfig::default();
    let path_result = value_iteration(
        &model,
        actions,
        profile.afterstate_aliasing,
        &terminal,
        |_s, _a| -1.0,
        vi_config,
    );
    let cost_result = value_iteration(
        &model,
        actions,
        profile.afterstate_aliasing,
        &terminal,
        |_s, a| -synthetic_cost_for_action(a),
        vi_config,
    );

    let goal_h = goal_hash(&task.goal);
    let path = trained_model_from(
        goal_h,
        item_class.clone(),
        poc2_data::BUNDLE_SCHEMA_VERSION,
        poc2_engine::ENGINE_SCHEMA_VERSION,
        RewardKind::PathLength,
        &path_result,
        Some(&cost_result),
    );
    let cost = trained_model_from(
        goal_h,
        item_class.clone(),
        poc2_data::BUNDLE_SCHEMA_VERSION,
        poc2_engine::ENGINE_SCHEMA_VERSION,
        RewardKind::Cost,
        &cost_result,
        Some(&cost_result),
    );

    let initial_features = featurize(&task.initial_item, &task.goal, task.registry);
    let metrics = SolveMetrics {
        states_visited: model.entry_count(),
        vi_iterations_path: path_result.iterations,
        vi_iterations_cost: cost_result.iterations,
        v_path_s0: path_result
            .value
            .get(&initial_features)
            .copied()
            .unwrap_or(0.0),
        v_cost_s0: cost_result
            .value
            .get(&initial_features)
            .copied()
            .unwrap_or(0.0),
    };

    SolvedGoal {
        path,
        cost,
        metrics,
    }
}

/// The basic-orb action set (Perfect tiers + Annul + Divine) every solve
/// explores.
#[must_use]
pub fn basic_solver_actions() -> Vec<AdvisorAction> {
    [
        "PerfectOrbOfTransmutation",
        "PerfectOrbOfAugmentation",
        "PerfectRegalOrb",
        "PerfectExaltedOrb",
        "PerfectChaosOrb",
        "OrbOfAnnulment",
        "DivineOrb",
    ]
    .into_iter()
    .map(|id| AdvisorAction::ApplyCurrency {
        currency: CurrencyId::from(id),
        omens: vec![],
    })
    .collect()
}

/// Full solver action set: basic orbs plus every goal-relevant essence —
/// Greater (Magic → Rare promote) and Perfect (Rare remove-add) tiers
/// whose granted mod for `item_class` carries a goal-wanted concept.
/// Lesser/Normal are mechanically weaker Greater duplicates; Corrupted
/// needs Vaal state the solver doesn't model.
#[must_use]
pub fn enumerate_solver_actions(
    goal: &Goal,
    item_class: &ItemClassId,
    essences: &[Essence],
    registry: &ModRegistry,
) -> Vec<AdvisorAction> {
    let mut actions = basic_solver_actions();

    let wanted: Vec<&poc2_engine::ids::ConceptId> = goal
        .target
        .prefixes
        .iter()
        .chain(goal.target.suffixes.iter())
        .flat_map(|s| s.concept.iter().chain(s.concept_any.iter()))
        .collect();
    if wanted.is_empty() {
        return actions;
    }

    for essence in essences {
        if !matches!(
            essence.quality,
            poc2_engine::EssenceQuality::Greater | poc2_engine::EssenceQuality::Perfect
        ) {
            continue;
        }
        let granted: Vec<&poc2_engine::ids::ModId> = if essence.class_targets.is_empty() {
            vec![&essence.target_mod]
        } else {
            let targets: Vec<_> = essence
                .class_targets
                .iter()
                .filter(|t| &t.class == item_class)
                .map(|t| &t.mod_id)
                .collect();
            if targets.is_empty() {
                // Class-targeted essence with no entry for this class —
                // illegal on the class.
                continue;
            }
            targets
        };
        let relevant = granted.iter().any(|mod_id| {
            registry
                .get(mod_id)
                .is_some_and(|def| def.concept_set.iter().any(|c| wanted.contains(&c)))
        });
        if relevant {
            actions.push(AdvisorAction::ApplyCurrency {
                currency: essence.id.clone(),
                omens: vec![],
            });
        }
    }
    actions
}

/// Bitmap-full terminal predicate for `goal` (artefact schema v2: the
/// bitmap is count-aware, so this agrees with `is_satisfied` modulo
/// `target.constraints` and `min_tier`). Never fires for empty targets.
pub fn terminal_predicate(goal: &Goal) -> impl Fn(&FeatureVec) -> bool {
    let n_specs = (goal.target.prefixes.len() + goal.target.suffixes.len()).min(16);
    let mask: u16 = if n_specs == 16 {
        u16::MAX
    } else if n_specs == 0 {
        0
    } else {
        (1u16 << n_specs) - 1
    };
    move |state: &FeatureVec| mask != 0 && (state.target_match & mask) == mask
}

/// Synthetic per-action cost weights for the cost reward. Stable across
/// runs so trained models are reproducible; a live-`Valuator` cost feed
/// remains future work (docs/81 §6.3 note).
#[must_use]
pub fn synthetic_cost_for_action(action: &AdvisorAction) -> f64 {
    let AdvisorAction::ApplyCurrency { currency, .. } = action else {
        return 0.05;
    };
    // Several currencies intentionally share a weight; keep the arms
    // distinct so future price updates can diverge them independently.
    #[allow(clippy::match_same_arms)]
    match currency.as_str() {
        "PerfectOrbOfTransmutation" => 0.5,
        "PerfectOrbOfAugmentation" => 0.4,
        "PerfectRegalOrb" => 0.6,
        "PerfectExaltedOrb" => 1.5,
        "PerfectChaosOrb" => 0.8,
        "OrbOfAnnulment" => 0.3,
        "DivineOrb" => 0.5,
        // Catalogue essences (goal-relevant, added per-goal by
        // `enumerate_solver_actions`). Perfect = the expensive
        // deterministic finisher; Greater = the mid-tier promoter.
        s if s.starts_with("PerfectEssence") => 2.0,
        s if s.contains("Essence") => 0.7,
        _ => 0.05,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use poc2_engine::base_registry::BaseRegistry;
    use poc2_engine::currency::DefaultCurrencyResolver;
    use poc2_engine::ids::{BaseTypeId, ConceptId, ModGroupId, ModId, StatId, TagId};
    use poc2_engine::item::{AffixType, Item, QualityKind, Rarity};
    use poc2_engine::mods::{
        ModDefinition, ModDomain, ModFlags, ModGroup, ModKind, ModStat, SpawnWeight,
    };
    use poc2_engine::patch::{PatchRange, PatchVersion};
    use poc2_market::DivEquiv;
    use poc2_strategies::{Target, TargetSpec};
    use smallvec::smallvec;

    const CLASS: &str = "BodyArmour";

    fn mk_mod(id: &str, group: &str, affix: AffixType, concept: &str) -> ModDefinition {
        ModDefinition {
            id: ModId::from(id),
            name: None,
            mod_group: ModGroup(ModGroupId::from(group)),
            affix_type: affix,
            kind: ModKind::Explicit,
            domain: ModDomain::Item,
            tags: smallvec![],
            concept_set: smallvec![ConceptId::from(concept)],
            spawn_weights: smallvec![SpawnWeight {
                tag: TagId::from(CLASS),
                weight: 1
            }],
            stats: smallvec![ModStat {
                stat_id: StatId::from(format!("stat_{id}")),
                min: 10.0,
                max: 20.0,
            }],
            required_level: 1,
            tier: None,
            allowed_item_classes: smallvec![ItemClassId::from(CLASS)],
            patch_range: PatchRange::ALL,
            flags: ModFlags::empty(),
            text_template: None,
        }
    }

    fn fixture_registry() -> ModRegistry {
        ModRegistry::from_mods(
            vec![
                mk_mod("ES1", "ES", AffixType::Prefix, "EnergyShield"),
                mk_mod("Life1", "Life", AffixType::Prefix, "Life"),
                mk_mod("FireRes1", "FireRes", AffixType::Suffix, "FireResistance"),
                mk_mod("ColdRes1", "ColdRes", AffixType::Suffix, "ColdResistance"),
            ],
            vec![],
        )
    }

    fn normal_item() -> Item {
        Item {
            base: BaseTypeId::from(CLASS),
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

    fn es_fire_goal() -> Goal {
        Goal::new(
            Target {
                prefixes: vec![TargetSpec {
                    concept: Some(ConceptId::from("EnergyShield")),
                    concept_any: vec![],
                    affix: None,
                    count: 1,
                    min_tier: None,
                    allow_hybrid: true,
                }],
                suffixes: vec![TargetSpec {
                    concept: Some(ConceptId::from("FireResistance")),
                    concept_any: vec![],
                    affix: None,
                    count: 1,
                    min_tier: None,
                    allow_hybrid: true,
                }],
                constraints: vec![],
            },
            DivEquiv::point(100.0),
        )
    }

    #[test]
    fn solve_goal_produces_cache_ready_pair() {
        let registry = fixture_registry();
        let base_registry = BaseRegistry::default();
        let resolver = DefaultCurrencyResolver::new();
        let goal = es_fire_goal();
        let task = CraftingTask {
            initial_item: normal_item(),
            goal: goal.clone(),
            registry: &registry,
            base_registry: &base_registry,
            resolver: &resolver,
            patch: PatchVersion::PATCH_0_5_0,
            omens: poc2_engine::omen::OmenSet::new(),
        };
        let class = ItemClassId::from(CLASS);
        let actions = enumerate_solver_actions(&goal, &class, &[], &registry);
        let solved = solve_goal(&task, &class, &actions, SolveProfile::on_demand());

        assert_eq!(solved.path.goal_hash, goal_hash(&goal));
        assert_eq!(solved.path.item_class, class);
        assert_eq!(solved.path.reward_kind, RewardKind::PathLength);
        assert_eq!(solved.cost.reward_kind, RewardKind::Cost);
        assert!(
            solved.metrics.v_path_s0 < 0.0 && solved.metrics.v_path_s0 > -100.0,
            "goal should be reachable: V_path(s0) = {}",
            solved.metrics.v_path_s0
        );
        assert!(solved.metrics.states_visited > 0);

        // The pair drops straight into a cache and answers Q lookups.
        let mut cache = crate::training::TrainedModelCache::new();
        cache.insert_pair(solved.path, Some(solved.cost));
        let (path, cost) = cache.lookup_pair(goal_hash(&goal), &class).expect("hit");
        let root = featurize(&task.initial_item, &goal, &registry);
        let transmute = AdvisorAction::ApplyCurrency {
            currency: CurrencyId::from("PerfectOrbOfTransmutation"),
            omens: vec![],
        };
        assert!(path.q_at(root, &transmute).is_some());
        assert!(cost.expect("cost twin").q_at(root, &transmute).is_some());
    }

    #[test]
    fn solve_goal_is_deterministic() {
        let registry = fixture_registry();
        let base_registry = BaseRegistry::default();
        let resolver = DefaultCurrencyResolver::new();
        let goal = es_fire_goal();
        let task = CraftingTask {
            initial_item: normal_item(),
            goal: goal.clone(),
            registry: &registry,
            base_registry: &base_registry,
            resolver: &resolver,
            patch: PatchVersion::PATCH_0_5_0,
            omens: poc2_engine::omen::OmenSet::new(),
        };
        let class = ItemClassId::from(CLASS);
        let actions = basic_solver_actions();
        let a = solve_goal(&task, &class, &actions, SolveProfile::on_demand());
        let b = solve_goal(&task, &class, &actions, SolveProfile::on_demand());
        assert_eq!(a.metrics.states_visited, b.metrics.states_visited);
        assert!((a.metrics.v_path_s0 - b.metrics.v_path_s0).abs() < 1e-12);
        assert_eq!(a.path.q_table.len(), b.path.q_table.len());
    }

    #[test]
    fn terminal_predicate_requires_all_spec_bits() {
        let goal = es_fire_goal();
        let terminal = terminal_predicate(&goal);
        let fv = |tm: u16| FeatureVec {
            rarity: 2,
            target_match: tm,
            n_prefixes: 1,
            n_suffixes: 1,
            has_hidden_desecrated: false,
            has_fractured: false,
            is_corrupted: false,
            has_hinekora_lock: false,
            extra_flags: 0,
        };
        assert!(!terminal(&fv(0b00)));
        assert!(!terminal(&fv(0b01)));
        assert!(!terminal(&fv(0b10)));
        assert!(terminal(&fv(0b11)));
    }
}
