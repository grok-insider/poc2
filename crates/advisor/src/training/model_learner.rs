//! M16.2 — Per-action transition model learner.
//!
//! Offline-learns `P(s' | s, a)` as a categorical distribution table by
//! Monte Carlo sampling over the engine simulator. The output
//! [`TableModel`] is the input to [`crate::training::value_iteration`]
//! (M16.3) which produces the trained policy's Q-table.
//!
//! ## Algorithm (Britz Algorithm 1, adapted)
//!
//! 1. Initialize `done_states: HashSet<FeatureVec>`, queue =
//!    `[task.initial_item]` with its featurize result.
//! 2. While queue is non-empty: pop an item.
//! 3. If the item is goal-satisfied or any abandon-criterion fires:
//!    skip (terminal state).
//! 4. Mark the item's `FeatureVec` done.
//! 5. For each candidate action: skip if afterstate-aliased (the alias
//!    table tells us this `(s, a)` pair shares its distribution with a
//!    previously-sampled pair).
//! 6. For each non-aliased `(features, action)`: run
//!    `simulate(item, action)` `samples_per_state_action` times,
//!    accumulate next-state counts, push unseen next-items onto the queue.
//! 7. Normalize counts to probabilities.
//!
//! ## Afterstate aliasing
//!
//! Some actions have transition distributions that depend only on
//! parts of the state captured in the [`FeatureVec`] — not on the
//! specific concrete item. Two `(state, action)` pairs whose alias
//! categories match share a single transition distribution. This
//! collapses essence/exalt/etc state-action entries into a smaller table
//! and matches Britz's optimization that cuts 10× off sample budget.
//!
//! ## Reference
//!
//! `docs/81-engine-training-and-rule-encoding-plan.md` §6.2 Tier 3.2.

use ahash::{AHashMap, AHashSet};
use poc2_engine::base_registry::BaseRegistry;
use poc2_engine::currency::CurrencyResolver;
use poc2_engine::item::Item;
use poc2_engine::omen::OmenSet;
use poc2_engine::patch::PatchVersion;
use poc2_engine::registry::ModRegistry;
use rand::{RngCore, SeedableRng};
use rand_xoshiro::Xoshiro256PlusPlus;

use crate::action::AdvisorAction;
use crate::featurize::{featurize, FeatureVec};
use crate::goal::{is_satisfied_with_ctx, should_abandon_with_ctx, Goal};
use crate::simulator::simulate;
use poc2_strategies::PredicateContext;

/// Inputs to a single training run. One `CraftingTask` describes one
/// goal that the model learner explores. The training corpus (M16.6) is
/// a `Vec<CraftingTask>` of canonical goals.
pub struct CraftingTask<'a> {
    pub initial_item: Item,
    pub goal: Goal,
    pub registry: &'a ModRegistry,
    pub base_registry: &'a BaseRegistry,
    pub resolver: &'a dyn CurrencyResolver,
    pub patch: PatchVersion,
    /// Initial omen set the player has active. Most training tasks pass
    /// an empty set.
    pub omens: OmenSet,
}

/// Categorization of a `(state, action)` pair for aliasing.
///
/// Two pairs that resolve to the same `StateActionAlias` share a single
/// transition distribution in the [`TableModel`]. The aliasing is
/// conservative: when in doubt, [`Self::Pair`] preserves the distinct
/// pair so the model is no less accurate than the no-alias case.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum StateActionAlias {
    /// No aliasing — distinct `(state, action)` pair with its own
    /// transition distribution.
    Pair(FeatureVec, AdvisorAction),
    /// All Essence-apply pairs whose post-action target_match bitmap
    /// pattern matches collapse to a single afterstate. The action's
    /// added mod is fixed by the essence's `target_mod` bundle entry,
    /// so the next-state distribution depends only on which existing
    /// mods get preserved + which target_match bit gets flipped.
    AfterEssence {
        essence_id: poc2_engine::ids::CurrencyId,
        target_match: u16,
        n_prefixes: u8,
        n_suffixes: u8,
    },
    /// Exalted Orb afterstate: distribution depends only on which
    /// slot is empty (captured in `n_prefixes`/`n_suffixes`) and the
    /// item's target-match pattern.
    AfterExalt {
        target_match: u16,
        n_prefixes: u8,
        n_suffixes: u8,
    },
    /// Regal Orb afterstate (Magic → Rare with one added mod).
    AfterRegal {
        target_match: u16,
        n_prefixes: u8,
        n_suffixes: u8,
    },
    /// Transmutation afterstate (Normal → Magic with one mod).
    AfterTransmute { target_match: u16 },
    /// Augmentation afterstate (Magic, fills empty slot).
    AfterAugment {
        target_match: u16,
        n_prefixes: u8,
        n_suffixes: u8,
    },
}

impl StateActionAlias {
    /// Compute the alias category for a `(state, action)` pair when
    /// aliasing is enabled. With aliasing disabled, return
    /// [`Self::Pair`] unconditionally.
    #[must_use]
    pub fn from(features: FeatureVec, action: &AdvisorAction, enable: bool) -> Self {
        if !enable {
            return Self::Pair(features, action.clone());
        }
        match action {
            AdvisorAction::ApplyCurrency { currency, omens } => {
                // Don't alias when omens are active — omen-conditioning
                // changes the distribution.
                if !omens.is_empty() {
                    return Self::Pair(features, action.clone());
                }
                let s = currency.as_str();
                if is_essence_id(s) {
                    return Self::AfterEssence {
                        essence_id: currency.clone(),
                        target_match: features.target_match,
                        n_prefixes: features.n_prefixes,
                        n_suffixes: features.n_suffixes,
                    };
                }
                if is_exalt(s) {
                    return Self::AfterExalt {
                        target_match: features.target_match,
                        n_prefixes: features.n_prefixes,
                        n_suffixes: features.n_suffixes,
                    };
                }
                if is_regal(s) {
                    return Self::AfterRegal {
                        target_match: features.target_match,
                        n_prefixes: features.n_prefixes,
                        n_suffixes: features.n_suffixes,
                    };
                }
                if is_transmute(s) {
                    return Self::AfterTransmute {
                        target_match: features.target_match,
                    };
                }
                if is_augment(s) {
                    return Self::AfterAugment {
                        target_match: features.target_match,
                        n_prefixes: features.n_prefixes,
                        n_suffixes: features.n_suffixes,
                    };
                }
                Self::Pair(features, action.clone())
            }
            _ => Self::Pair(features, action.clone()),
        }
    }
}

fn is_essence_id(s: &str) -> bool {
    s.contains("Essence")
}

fn is_exalt(s: &str) -> bool {
    s == "ExaltedOrb" || s == "GreaterExaltedOrb" || s == "PerfectExaltedOrb"
}

fn is_regal(s: &str) -> bool {
    s == "RegalOrb" || s == "GreaterRegalOrb" || s == "PerfectRegalOrb"
}

fn is_transmute(s: &str) -> bool {
    s == "OrbOfTransmutation"
        || s == "GreaterOrbOfTransmutation"
        || s == "PerfectOrbOfTransmutation"
}

fn is_augment(s: &str) -> bool {
    s == "OrbOfAugmentation" || s == "GreaterOrbOfAugmentation" || s == "PerfectOrbOfAugmentation"
}

/// Offline-learned categorical transition model.
///
/// Keyed by [`StateActionAlias`] so aliased pairs share a single entry.
/// The map's values are `next_state -> probability`. Probabilities sum
/// to ~1.0 within each (state, action) entry; failed-action samples
/// produce a self-loop entry rather than being dropped, so the
/// distribution remains a valid categorical.
#[derive(Debug, Clone, Default)]
pub struct TableModel {
    transitions: AHashMap<StateActionAlias, AHashMap<FeatureVec, f64>>,
    /// Total samples per alias entry, retained for diagnostics + the
    /// imitation-seed weighting (M16.5).
    sample_counts: AHashMap<StateActionAlias, u64>,
}

impl TableModel {
    /// Build a fresh empty model.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of distinct `(alias)` entries — proxy for state-space
    /// coverage. Smaller is better when afterstate aliasing is active.
    #[must_use]
    pub fn entry_count(&self) -> usize {
        self.transitions.len()
    }

    /// Return the categorical distribution for a `(state, action)`
    /// pair. `None` when the model has no entry — caller decides whether
    /// to fall back to beam search or run an additional sample pass.
    #[must_use]
    pub fn distribution(
        &self,
        features: FeatureVec,
        action: &AdvisorAction,
        enable_aliasing: bool,
    ) -> Option<&AHashMap<FeatureVec, f64>> {
        let alias = StateActionAlias::from(features, action, enable_aliasing);
        self.transitions.get(&alias)
    }

    /// Lookup variant that returns the entry as a borrowed slice of
    /// `(FeatureVec, probability)` pairs (deterministic order).
    #[must_use]
    pub fn distribution_pairs(
        &self,
        features: FeatureVec,
        action: &AdvisorAction,
        enable_aliasing: bool,
    ) -> Option<Vec<(FeatureVec, f64)>> {
        self.distribution(features, action, enable_aliasing)
            .map(|m| {
                let mut v: Vec<_> = m.iter().map(|(k, p)| (*k, *p)).collect();
                v.sort_by_key(|a| a.0.pack());
                v
            })
    }

    /// Sample-count for a `(state, action)` pair — useful when imitation
    /// seeding wants to weigh expert-derived distributions higher than
    /// random rollouts.
    #[must_use]
    pub fn sample_count(
        &self,
        features: FeatureVec,
        action: &AdvisorAction,
        enable_aliasing: bool,
    ) -> u64 {
        let alias = StateActionAlias::from(features, action, enable_aliasing);
        self.sample_counts.get(&alias).copied().unwrap_or(0)
    }

    /// All learned aliases, in deterministic order. Used by the value
    /// iteration solver to enumerate state-action pairs.
    #[must_use]
    pub fn aliases(&self) -> Vec<&StateActionAlias> {
        let mut v: Vec<_> = self.transitions.keys().collect();
        // Format::Debug fallback for stable ordering (FeatureVec inside
        // alias variants is u-comparable through pack(); enum variants
        // sort by Debug repr — sufficient for determinism).
        v.sort_by_key(|a| format!("{a:?}"));
        v
    }

    /// Distribution lookup keyed by a pre-computed alias. Used by
    /// [`crate::training::value_iteration::value_iteration`] when
    /// enumerating reachable next-states during state-set materialization.
    /// Returns the distribution as an ordered `Vec<(FeatureVec, prob)>`
    /// so caller iteration is deterministic.
    #[must_use]
    pub fn distribution_pairs_by_alias(
        &self,
        alias: &StateActionAlias,
    ) -> Option<Vec<(FeatureVec, f64)>> {
        self.transitions.get(alias).map(|m| {
            let mut v: Vec<_> = m.iter().map(|(k, p)| (*k, *p)).collect();
            v.sort_by_key(|a| a.0.pack());
            v
        })
    }
}

/// Mutable accumulator used during `learn_transition_model`. Encapsulates
/// the sample counts before they're normalized into probabilities.
#[derive(Debug, Default)]
pub struct TableModelBuilder {
    counts: AHashMap<StateActionAlias, AHashMap<FeatureVec, u64>>,
}

impl TableModelBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add `n` observations of `(alias) → next_state`. Used by the
    /// learner's hot loop.
    pub fn add(&mut self, alias: StateActionAlias, next: FeatureVec, n: u64) {
        let entry = self.counts.entry(alias).or_default();
        *entry.entry(next).or_insert(0) += n;
    }

    /// Add a single observation. Convenience for the hot loop's
    /// per-sample accumulation.
    pub fn observe(&mut self, alias: StateActionAlias, next: FeatureVec) {
        self.add(alias, next, 1);
    }

    /// Finalize the builder into a [`TableModel`] by normalizing
    /// per-alias counts to probabilities.
    #[must_use]
    pub fn finalize(self) -> TableModel {
        let mut transitions: AHashMap<StateActionAlias, AHashMap<FeatureVec, f64>> =
            AHashMap::with_capacity(self.counts.len());
        let mut sample_counts: AHashMap<StateActionAlias, u64> =
            AHashMap::with_capacity(self.counts.len());
        for (alias, next_counts) in self.counts {
            let total: u64 = next_counts.values().copied().sum();
            sample_counts.insert(alias.clone(), total);
            if total == 0 {
                transitions.insert(alias, AHashMap::new());
                continue;
            }
            let mut probs: AHashMap<FeatureVec, f64> = AHashMap::with_capacity(next_counts.len());
            for (state, count) in next_counts {
                probs.insert(state, count as f64 / total as f64);
            }
            transitions.insert(alias, probs);
        }
        TableModel {
            transitions,
            sample_counts,
        }
    }

    /// Number of accumulated entries; useful for tests + diagnostics.
    #[must_use]
    pub fn entry_count(&self) -> usize {
        self.counts.len()
    }
}

/// Configurable knobs for [`learn_transition_model`].
#[derive(Debug, Clone, Copy)]
pub struct LearnConfig {
    /// Number of Monte Carlo samples per non-aliased `(state, action)`.
    /// Britz uses 100_000; the v3 ship-prep training pass matches.
    /// Smoke tests use 1_000 or fewer.
    pub samples_per_state_action: u32,
    /// Whether to collapse afterstate-equivalent `(state, action)` pairs
    /// into a single distribution (default: `true`).
    pub afterstate_aliasing: bool,
    /// Random seed for reproducibility.
    pub seed: u64,
    /// Hard cap on the BFS reach. Goals with state-space larger than
    /// `max_states` truncate; the trained policy degrades on
    /// out-of-distribution states (caught by the sim-to-real-gap
    /// detector in M16.4).
    pub max_states: u32,
    /// Per-state action-list cap. v3 starts with the engine's basic-orb
    /// catalogue (~17 actions); set higher when the candidate generator
    /// surfaces strategy-specific actions during imitation seeding.
    pub max_actions_per_state: u32,
}

impl Default for LearnConfig {
    fn default() -> Self {
        Self {
            samples_per_state_action: 100_000,
            afterstate_aliasing: true,
            seed: 0x_5EED_C0DE_C0DE_5EED,
            max_states: 50_000,
            max_actions_per_state: 64,
        }
    }
}

/// Run the model learner over `task` per `config`.
///
/// Returns a finalized [`TableModel`]. Cost is roughly
/// `O(reachable_states × actions_per_state × samples_per_state_action)`
/// simulator calls; the v3 ship-prep training corpus (M16.6) runs this
/// offline once per patch and ships the result with the bundle.
///
/// `enumerate_actions` is the caller-supplied action enumerator.
/// `learn_transition_model` itself doesn't depend on the heavy advisor
/// candidate generator — tests pass a small fixed action list, and the
/// production training binary plugs in `generate_candidates` (M16.6).
pub fn learn_transition_model<F>(
    task: &CraftingTask<'_>,
    config: LearnConfig,
    mut enumerate_actions: F,
) -> TableModel
where
    F: FnMut(&Item, &Goal) -> Vec<AdvisorAction>,
{
    let mut builder = TableModelBuilder::new();
    let mut done_features: AHashSet<FeatureVec> = AHashSet::new();
    let mut representative_for_features: AHashMap<FeatureVec, Item> = AHashMap::new();
    let mut queue: Vec<(Item, FeatureVec)> = Vec::new();

    let initial_features = featurize(&task.initial_item, &task.goal, task.registry);
    queue.push((task.initial_item.clone(), initial_features));
    representative_for_features.insert(initial_features, task.initial_item.clone());

    let predicate_ctx = PredicateContext::new(task.registry);
    let mut visited_count: u32 = 0;
    let mut rng = Xoshiro256PlusPlus::seed_from_u64(config.seed);

    while let Some((item, features)) = queue.pop() {
        if done_features.contains(&features) {
            continue;
        }
        if visited_count >= config.max_states {
            tracing::warn!(
                visited = visited_count,
                cap = config.max_states,
                "model learner hit max_states cap; remaining states are truncated"
            );
            break;
        }
        // Terminal-state shortcut: goal-satisfied or abandon-fired states
        // need no transition entries (they're absorbing).
        if is_satisfied_with_ctx(&task.goal, &item, &predicate_ctx) {
            done_features.insert(features);
            visited_count += 1;
            continue;
        }
        if should_abandon_with_ctx(&task.goal, &item, &predicate_ctx) {
            done_features.insert(features);
            visited_count += 1;
            continue;
        }
        done_features.insert(features);
        visited_count += 1;

        let actions = enumerate_actions(&item, &task.goal);
        let actions_to_sample = actions
            .iter()
            .take(config.max_actions_per_state as usize)
            .cloned()
            .collect::<Vec<_>>();

        for action in actions_to_sample {
            let alias = StateActionAlias::from(features, &action, config.afterstate_aliasing);
            // If the alias already has samples, don't re-sample.
            if builder.counts.contains_key(&alias) {
                continue;
            }
            for _ in 0..config.samples_per_state_action {
                let seed = rng.next_u64();
                let outcome = simulate(
                    &item,
                    &action,
                    &task.omens,
                    task.registry,
                    task.resolver,
                    task.patch,
                    seed,
                );
                let next_features = featurize(&outcome.item, &task.goal, task.registry);
                builder.observe(alias.clone(), next_features);
                if !done_features.contains(&next_features)
                    && !representative_for_features.contains_key(&next_features)
                {
                    representative_for_features.insert(next_features, outcome.item.clone());
                    queue.push((outcome.item, next_features));
                }
            }
        }
    }

    builder.finalize()
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
    use poc2_strategies::{Target, TargetSpec};
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
            tier: None,
            allowed_item_classes: smallvec![ItemClassId::from("BodyArmour")],
            patch_range: PatchRange::ALL,
            flags: ModFlags::empty(),
            text_template: None,
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

    fn es_goal() -> Goal {
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
                suffixes: vec![],
                constraints: vec![],
            },
            DivEquiv::point(100.0),
        )
    }

    fn transmute_action() -> AdvisorAction {
        AdvisorAction::ApplyCurrency {
            currency: CurrencyId::from("OrbOfTransmutation"),
            omens: vec![],
        }
    }

    fn exalt_action() -> AdvisorAction {
        AdvisorAction::ApplyCurrency {
            currency: CurrencyId::from("ExaltedOrb"),
            omens: vec![],
        }
    }

    #[test]
    fn alias_from_essence_action_collapses_to_after_essence() {
        let f = FeatureVec {
            rarity: 1,
            target_match: 0b1,
            n_prefixes: 1,
            n_suffixes: 0,
            has_hidden_desecrated: false,
            has_fractured: false,
            is_corrupted: false,
            has_hinekora_lock: false,
            extra_flags: 0,
        };
        let a = AdvisorAction::ApplyCurrency {
            currency: CurrencyId::from("PerfectEssenceOfBattle"),
            omens: vec![],
        };
        let alias = StateActionAlias::from(f, &a, true);
        assert!(matches!(alias, StateActionAlias::AfterEssence { .. }));
    }

    #[test]
    fn alias_from_exalt_action_collapses_to_after_exalt() {
        let f = FeatureVec {
            rarity: 2,
            target_match: 0,
            n_prefixes: 2,
            n_suffixes: 1,
            has_hidden_desecrated: false,
            has_fractured: false,
            is_corrupted: false,
            has_hinekora_lock: false,
            extra_flags: 0,
        };
        let alias = StateActionAlias::from(f, &exalt_action(), true);
        assert!(matches!(alias, StateActionAlias::AfterExalt { .. }));
    }

    #[test]
    fn alias_disabled_returns_pair_unconditionally() {
        let f = FeatureVec {
            rarity: 0,
            target_match: 0,
            n_prefixes: 0,
            n_suffixes: 0,
            has_hidden_desecrated: false,
            has_fractured: false,
            is_corrupted: false,
            has_hinekora_lock: false,
            extra_flags: 0,
        };
        let alias = StateActionAlias::from(f, &transmute_action(), false);
        assert!(matches!(alias, StateActionAlias::Pair(..)));
    }

    #[test]
    fn alias_with_omens_does_not_collapse() {
        // Even with aliasing on, omen-conditioned actions stay distinct
        // because the distribution shifts under omen consumption.
        let f = FeatureVec {
            rarity: 0,
            target_match: 0,
            n_prefixes: 0,
            n_suffixes: 0,
            has_hidden_desecrated: false,
            has_fractured: false,
            is_corrupted: false,
            has_hinekora_lock: false,
            extra_flags: 0,
        };
        let a = AdvisorAction::ApplyCurrency {
            currency: CurrencyId::from("OrbOfTransmutation"),
            omens: vec![poc2_engine::ids::OmenId::from("OmenOfWhittling")],
        };
        let alias = StateActionAlias::from(f, &a, true);
        assert!(matches!(alias, StateActionAlias::Pair(..)));
    }

    #[test]
    fn builder_normalizes_to_probabilities() {
        let mut b = TableModelBuilder::new();
        let alias = StateActionAlias::Pair(
            FeatureVec {
                rarity: 0,
                target_match: 0,
                n_prefixes: 0,
                n_suffixes: 0,
                has_hidden_desecrated: false,
                has_fractured: false,
                is_corrupted: false,
                has_hinekora_lock: false,
                extra_flags: 0,
            },
            transmute_action(),
        );
        let next_a = FeatureVec {
            rarity: 1,
            target_match: 1,
            n_prefixes: 1,
            n_suffixes: 0,
            has_hidden_desecrated: false,
            has_fractured: false,
            is_corrupted: false,
            has_hinekora_lock: false,
            extra_flags: 0,
        };
        let next_b = FeatureVec {
            rarity: 1,
            target_match: 0,
            n_prefixes: 1,
            n_suffixes: 0,
            has_hidden_desecrated: false,
            has_fractured: false,
            is_corrupted: false,
            has_hinekora_lock: false,
            extra_flags: 0,
        };
        b.add(alias.clone(), next_a, 8);
        b.add(alias.clone(), next_b, 2);
        let model = b.finalize();
        let dist = model.transitions.get(&alias).unwrap();
        assert!((dist[&next_a] - 0.8).abs() < 1e-9);
        assert!((dist[&next_b] - 0.2).abs() < 1e-9);
    }

    #[test]
    fn learn_transition_model_smoke_one_action_distribution() {
        // Tiny task: Normal armour + 1 ES prefix mod available; the only
        // action is OrbOfTransmutation which produces a Magic item with
        // ES1 in the prefix slot. Per the M14.1 weighted-sampling logic,
        // the only eligible mod is ES1, so every sampled next-state is
        // identical (rarity Magic, target_match=0b1, n_prefixes=1).
        let registry = ModRegistry::from_mods(vec![mk_es_mod("ES1")], vec![]);
        let base_registry = BaseRegistry::default();
        let resolver = DefaultCurrencyResolver::new();
        let task = CraftingTask {
            initial_item: mk_normal_armour(),
            goal: es_goal(),
            registry: &registry,
            base_registry: &base_registry,
            resolver: &resolver,
            patch: PatchVersion::PATCH_0_4_0,
            omens: OmenSet::new(),
        };
        let config = LearnConfig {
            samples_per_state_action: 200,
            afterstate_aliasing: true,
            seed: 1,
            max_states: 1_000,
            max_actions_per_state: 8,
        };
        let model = learn_transition_model(&task, config, |_item, _goal| vec![transmute_action()]);
        let initial = featurize(&task.initial_item, &task.goal, &registry);
        let dist = model
            .distribution_pairs(initial, &transmute_action(), true)
            .expect("transmute distribution should be learned");
        assert!(!dist.is_empty());
        let total: f64 = dist.iter().map(|(_, p)| *p).sum();
        assert!(
            (total - 1.0).abs() < 1e-6,
            "probs should sum to 1; got {total}"
        );
        // Transmute uniformly picks Prefix or Suffix; the suffix branch
        // finds no eligible suffix mod and leaves the item Normal. The
        // prefix branch produces a Magic item with ES1 (target_match
        // bit 0 set). Both outcomes appear in the categorical
        // distribution — assert both flavours are observed and that the
        // success-path entry hits the target_match.
        let saw_success = dist
            .iter()
            .any(|(s, _)| s.rarity == 1 && (s.target_match & 1) == 1);
        let saw_failure_or_unchanged = dist.iter().any(|(s, _)| s.rarity == 0);
        assert!(
            saw_success,
            "expected at least one Magic-with-ES1 outcome; got {dist:?}"
        );
        assert!(
            saw_failure_or_unchanged,
            "expected at least one Normal-unchanged outcome (suffix branch fails); got {dist:?}"
        );
    }

    #[test]
    fn afterstate_aliasing_collapses_essence_states() {
        // Build a task with two essence actions referencing the same
        // (state, action) shape (different essence ids). With aliasing
        // ON, each essence id maps to a distinct AfterEssence alias.
        // With aliasing OFF, every (s, a) is a distinct Pair — but
        // because the essence ids differ, the OFF count equals the ON
        // count for THIS specific test. To isolate the aliasing benefit,
        // we use a single essence id called from multiple states.

        // Construct a model with 2 states and 1 action.
        let f1 = FeatureVec {
            rarity: 1,
            target_match: 0b1,
            n_prefixes: 1,
            n_suffixes: 0,
            has_hidden_desecrated: false,
            has_fractured: false,
            is_corrupted: false,
            has_hinekora_lock: false,
            extra_flags: 0,
        };
        let f2 = FeatureVec {
            rarity: 1,
            target_match: 0b1,
            n_prefixes: 1,
            n_suffixes: 0,
            has_hidden_desecrated: false,
            has_fractured: true,
            is_corrupted: false,
            has_hinekora_lock: false,
            extra_flags: 0,
        };
        let action = AdvisorAction::ApplyCurrency {
            currency: CurrencyId::from("PerfectEssenceOfBattle"),
            omens: vec![],
        };
        let alias_on_1 = StateActionAlias::from(f1, &action, true);
        let alias_on_2 = StateActionAlias::from(f2, &action, true);
        // Essence aliasing ignores `has_fractured`, so f1 and f2 collapse.
        assert_eq!(alias_on_1, alias_on_2, "essence aliasing should collapse");

        let alias_off_1 = StateActionAlias::from(f1, &action, false);
        let alias_off_2 = StateActionAlias::from(f2, &action, false);
        assert_ne!(
            alias_off_1, alias_off_2,
            "no aliasing should keep pairs distinct"
        );
    }
}
