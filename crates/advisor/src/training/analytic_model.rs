//! Analytic (exact) transition-model builder — the Monte Carlo replacement.
//!
//! [`crate::training::model_learner::learn_transition_model`] estimates
//! `P(s' | s, a)` by drawing `samples_per_state_action` simulator rollouts
//! per `(state, action)` alias. But the simulator's randomness is fully
//! parameterized by data the engine already holds in closed form:
//!
//! - **which affix slot** — `pick_open_affix`: a fair coin iff both sides
//!   have an open slot, deterministic otherwise;
//! - **which mod** — a weighted categorical over
//!   [`poc2_engine::currency::enumerate_eligible_mods`] (the
//!   sampling-identical pool builder factored out of `sample_eligible_mod`);
//! - **which removal** — uniform over
//!   [`poc2_engine::currency::collect_removable_filtered`];
//! - **value rolls** — irrelevant: [`crate::featurize::FeatureVec`] never
//!   reads rolled values, so value randomness marginalizes out.
//!
//! This module therefore *constructs* the exact categorical distribution
//! for every basic-orb action instead of estimating it: zero sampling
//! error, no `samples_per_state_action` budget, and a full-corpus training
//! run drops from hours to seconds. Actions without a closed form here
//! (omen-conditioned applies, essences pre-Phase-B, bones, locks, reveal)
//! fall back to a per-alias Monte Carlo estimate so the builder is never
//! less capable than the MC learner.
//!
//! ## Fidelity contract
//!
//! The analytic distributions must match what [`crate::simulator::simulate`]
//! samples, because the plan-time beam search Monte-Carlos through that same
//! simulator. Two consequences:
//!
//! - Pool enumeration uses [`poc2_engine::base_registry::EMPTY`], exactly
//!   like `simulate`'s `apply_currency` (no-bases) path — NOT
//!   `CraftingTask::base_registry`. If `simulate` ever threads the real
//!   `BaseRegistry`, switch this module at the marked call sites.
//! - Failed applies contribute **self-loop** mass on the *unchanged* item
//!   (the engine orchestrator restores the snapshot on `Err`), mirroring the
//!   MC learner's failed-sample accounting.
//!
//! The cross-validation tests in this module pin `analytic ≈ MC` per action
//! family; a disagreement is a bug in one of the two paths.

use ahash::{AHashMap, AHashSet};
use poc2_engine::currency::{
    collect_removable_filtered, enumerate_eligible_mods, min_mod_level_floor, BASIC_ORB_EXCLUDES,
};
use poc2_engine::ids::CurrencyId;
use poc2_engine::item::{AffixType, Item, ModRoll, Rarity};
use poc2_engine::mods::ModDefinition;
use rand::{RngCore, SeedableRng};
use rand_xoshiro::Xoshiro256PlusPlus;

use crate::action::AdvisorAction;
use crate::featurize::{featurize, FeatureVec};
use crate::goal::{is_satisfied_with_ctx, should_abandon_with_ctx};
use crate::simulator::simulate;
use crate::training::model_learner::{CraftingTask, StateActionAlias, TableModel};
use poc2_strategies::PredicateContext;

/// Sentinel `sample_count` for exact (analytically constructed)
/// distributions. `u64::MAX` so any sample-count-weighted consumer
/// (imitation seeding) treats exact entries as maximally trustworthy.
pub const EXACT_DISTRIBUTION_SAMPLES: u64 = u64::MAX;

/// Knobs for [`learn_transition_model_analytic`].
#[derive(Debug, Clone, Copy)]
pub struct AnalyticConfig {
    /// Collapse afterstate-equivalent `(state, action)` pairs (see
    /// [`StateActionAlias`]). Same semantics as
    /// [`crate::training::model_learner::LearnConfig::afterstate_aliasing`].
    pub afterstate_aliasing: bool,
    /// Seed for the **Monte Carlo fallback only** — exact distributions
    /// are seed-independent. Per-alias streams are derived from this.
    pub seed: u64,
    /// Hard cap on the BFS reach (same as the MC learner's `max_states`).
    pub max_states: u32,
    /// Per-state action-list cap.
    pub max_actions_per_state: u32,
    /// Samples per alias for actions without a closed form. Far smaller
    /// than the MC learner's default because it only covers the exotic
    /// action tail (omens, reveal, locks).
    pub mc_fallback_samples: u32,
}

impl Default for AnalyticConfig {
    fn default() -> Self {
        Self {
            afterstate_aliasing: true,
            seed: 0x_5EED_C0DE_C0DE_5EED,
            max_states: 50_000,
            max_actions_per_state: 64,
            mc_fallback_samples: 10_000,
        }
    }
}

/// Build the transition [`TableModel`] for `task` analytically.
///
/// Drop-in replacement for
/// [`crate::training::model_learner::learn_transition_model`]: identical
/// BFS structure and terminal handling, identical output shape (so
/// [`crate::training::value_iteration::value_iteration`] consumes it
/// unchanged), but each supported `(state, action)` alias gets the exact
/// categorical distribution instead of `samples_per_state_action`
/// simulator rollouts.
pub fn learn_transition_model_analytic<F>(
    task: &CraftingTask<'_>,
    config: AnalyticConfig,
    mut enumerate_actions: F,
) -> TableModel
where
    F: FnMut(&Item, &crate::goal::Goal) -> Vec<AdvisorAction>,
{
    let mut model = TableModel::new();
    let mut done_features: AHashSet<FeatureVec> = AHashSet::new();
    let mut representative_for_features: AHashMap<FeatureVec, Item> = AHashMap::new();
    let mut queue: Vec<(Item, FeatureVec)> = Vec::new();

    let initial_features = featurize(&task.initial_item, &task.goal, task.registry);
    queue.push((task.initial_item.clone(), initial_features));
    representative_for_features.insert(initial_features, task.initial_item.clone());

    let predicate_ctx = PredicateContext::new(task.registry);
    let mut visited_count: u32 = 0;

    while let Some((item, features)) = queue.pop() {
        if done_features.contains(&features) {
            continue;
        }
        if visited_count >= config.max_states {
            tracing::warn!(
                visited = visited_count,
                cap = config.max_states,
                "analytic model builder hit max_states cap; remaining states are truncated"
            );
            break;
        }
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
        let actions_to_expand = actions
            .iter()
            .take(config.max_actions_per_state as usize)
            .cloned()
            .collect::<Vec<_>>();

        for action in actions_to_expand {
            let alias = StateActionAlias::from(features, &action, config.afterstate_aliasing);
            if model.contains_alias(&alias) {
                continue;
            }
            if let Some(pairs) = analytic_transition(&item, &action, task) {
                insert_exact_entry(
                    &mut model,
                    alias,
                    pairs,
                    task,
                    &done_features,
                    &mut representative_for_features,
                    &mut queue,
                );
            } else {
                insert_mc_fallback_entry(
                    &mut model,
                    alias,
                    &item,
                    &action,
                    task,
                    config,
                    &done_features,
                    &mut representative_for_features,
                    &mut queue,
                );
            }
        }
    }

    model
}

/// Bucket an exact `(next_item, p)` list into a FeatureVec categorical and
/// insert it, enqueueing unseen representatives.
fn insert_exact_entry(
    model: &mut TableModel,
    alias: StateActionAlias,
    pairs: Vec<(Item, f64)>,
    task: &CraftingTask<'_>,
    done_features: &AHashSet<FeatureVec>,
    representative_for_features: &mut AHashMap<FeatureVec, Item>,
    queue: &mut Vec<(Item, FeatureVec)>,
) {
    let mut dist: AHashMap<FeatureVec, f64> = AHashMap::new();
    for (next_item, p) in pairs {
        let next_features = featurize(&next_item, &task.goal, task.registry);
        *dist.entry(next_features).or_insert(0.0) += p;
        maybe_enqueue(
            next_item,
            next_features,
            done_features,
            representative_for_features,
            queue,
        );
    }
    debug_assert!(
        (dist.values().sum::<f64>() - 1.0).abs() < 1e-9,
        "analytic distribution must sum to 1 (alias {alias:?})"
    );
    model.insert_distribution(alias, dist, EXACT_DISTRIBUTION_SAMPLES);
}

/// Monte Carlo fallback for actions without a closed form.
#[allow(clippy::too_many_arguments)] // internal hot-loop helper, mirrors the learner state
fn insert_mc_fallback_entry(
    model: &mut TableModel,
    alias: StateActionAlias,
    item: &Item,
    action: &AdvisorAction,
    task: &CraftingTask<'_>,
    config: AnalyticConfig,
    done_features: &AHashSet<FeatureVec>,
    representative_for_features: &mut AHashMap<FeatureVec, Item>,
    queue: &mut Vec<(Item, FeatureVec)>,
) {
    let mut rng = Xoshiro256PlusPlus::seed_from_u64(fallback_seed(config.seed, &alias));
    let n = config.mc_fallback_samples.max(1);
    let mut counts: AHashMap<FeatureVec, u64> = AHashMap::new();
    for _ in 0..n {
        let seed = rng.next_u64();
        let outcome = simulate(
            item,
            action,
            &task.omens,
            task.registry,
            task.resolver,
            task.patch,
            seed,
        );
        let next_features = featurize(&outcome.item, &task.goal, task.registry);
        *counts.entry(next_features).or_insert(0) += 1;
        maybe_enqueue(
            outcome.item,
            next_features,
            done_features,
            representative_for_features,
            queue,
        );
    }
    let total = f64::from(n);
    let dist: AHashMap<FeatureVec, f64> = counts
        .into_iter()
        .map(|(k, c)| (k, c as f64 / total))
        .collect();
    model.insert_distribution(alias, dist, u64::from(n));
}

/// Push `next_item` as the representative of an unseen feature state.
fn maybe_enqueue(
    next_item: Item,
    next_features: FeatureVec,
    done_features: &AHashSet<FeatureVec>,
    representative_for_features: &mut AHashMap<FeatureVec, Item>,
    queue: &mut Vec<(Item, FeatureVec)>,
) {
    if !done_features.contains(&next_features)
        && !representative_for_features.contains_key(&next_features)
    {
        representative_for_features.insert(next_features, next_item.clone());
        queue.push((next_item, next_features));
    }
}

/// Deterministic per-alias seed for the MC fallback (stable within a build;
/// the shipped artefact is regenerated per patch, so cross-rustc-version
/// hash stability is not required).
fn fallback_seed(base: u64, alias: &StateActionAlias) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    alias.hash(&mut h);
    base ^ h.finish()
}

/// Add-a-mod orb family parameters (Transmute / Augment / Regal / Exalt
/// and their Greater / Perfect tiers).
struct AddModSpec {
    require: Rarity,
    promote: Option<Rarity>,
    max_slots: u8,
}

enum Family {
    AddMod(AddModSpec),
    Chaos,
    Annul,
    Divine,
}

/// Classify a currency id into an analytically supported family. Ids match
/// the `Currency::id` values in `poc2_engine::currency::{basic, variants}`.
fn family_for(id: &CurrencyId) -> Option<Family> {
    match id.as_str() {
        "OrbOfTransmutation" | "GreaterOrbOfTransmutation" | "PerfectOrbOfTransmutation" => {
            Some(Family::AddMod(AddModSpec {
                require: Rarity::Normal,
                promote: Some(Rarity::Magic),
                max_slots: 1,
            }))
        }
        "OrbOfAugmentation" | "GreaterOrbOfAugmentation" | "PerfectOrbOfAugmentation" => {
            Some(Family::AddMod(AddModSpec {
                require: Rarity::Magic,
                promote: None,
                max_slots: 1,
            }))
        }
        "RegalOrb" | "GreaterRegalOrb" | "PerfectRegalOrb" => Some(Family::AddMod(AddModSpec {
            require: Rarity::Magic,
            promote: Some(Rarity::Rare),
            max_slots: 3,
        })),
        "ExaltedOrb" | "GreaterExaltedOrb" | "PerfectExaltedOrb" => {
            Some(Family::AddMod(AddModSpec {
                require: Rarity::Rare,
                promote: None,
                max_slots: 3,
            }))
        }
        "ChaosOrb" | "GreaterChaosOrb" | "PerfectChaosOrb" => Some(Family::Chaos),
        "OrbOfAnnulment" => Some(Family::Annul),
        "DivineOrb" => Some(Family::Divine),
        _ => None,
    }
}

/// Exact next-state distribution for `action` on `item`, as
/// `(next_item, probability)` pairs summing to 1. `None` = no closed form
/// (caller falls back to Monte Carlo).
fn analytic_transition(
    item: &Item,
    action: &AdvisorAction,
    task: &CraftingTask<'_>,
) -> Option<Vec<(Item, f64)>> {
    match action {
        AdvisorAction::ApplyCurrency { currency, omens } => {
            // Omen-conditioned applies change the distribution (forced
            // affixes, Whittling, Greater Exaltation, ...) — no closed form
            // here; the alias layer also refuses to alias them.
            if !omens.is_empty() {
                return None;
            }
            let floor = min_mod_level_floor(currency, task.patch);
            match family_for(currency)? {
                Family::AddMod(spec) => Some(add_mod_distribution(item, &spec, floor, task)),
                Family::Chaos => Some(chaos_distribution(item, floor, task)),
                Family::Annul => Some(annul_distribution(item)),
                Family::Divine => Some(divine_distribution(item)),
            }
        }
        // simulate() treats these as unconditional no-ops → identity.
        AdvisorAction::Stop | AdvisorAction::Abandon { .. } | AdvisorAction::Guidance { .. } => {
            Some(self_loop(item))
        }
        AdvisorAction::ActivateOmen { .. } => Some(self_loop(item)),
        // Reveal / Lock / Recombine / Recurring keep engine-owned semantics
        // → Monte Carlo fallback.
        _ => None,
    }
}

fn self_loop(item: &Item) -> Vec<(Item, f64)> {
    vec![(item.clone(), 1.0)]
}

/// Open-slot distribution mirroring `pick_open_affix`: fair coin iff both
/// sides are open, deterministic when one is, `None` when neither.
fn slot_distribution(item: &Item, max_slots: u8) -> Option<Vec<(AffixType, f64)>> {
    let prefix_open = item.prefixes.len() < max_slots as usize;
    let suffix_open = item.suffixes.len() < max_slots as usize;
    match (prefix_open, suffix_open) {
        (true, true) => Some(vec![(AffixType::Prefix, 0.5), (AffixType::Suffix, 0.5)]),
        (true, false) => Some(vec![(AffixType::Prefix, 1.0)]),
        (false, true) => Some(vec![(AffixType::Suffix, 1.0)]),
        (false, false) => None,
    }
}

/// Representative `ModRoll` for a sampled definition. Rolled values never
/// influence [`FeatureVec`], mod-group occupancy, or removability, so a
/// midpoint roll is a faithful representative.
fn mk_roll(def: &ModDefinition) -> ModRoll {
    ModRoll {
        mod_id: def.id.clone(),
        affix_type: def.affix_type,
        kind: def.kind,
        values: def.stats.iter().map(|s| s.roll(0.5)).collect(),
        is_fractured: false,
    }
}

fn push_roll(item: &mut Item, roll: ModRoll) {
    match roll.affix_type {
        AffixType::Prefix => item.prefixes.push(roll),
        AffixType::Suffix => item.suffixes.push(roll),
        _ => {}
    }
}

/// Exact distribution for the add-one-mod orbs.
fn add_mod_distribution(
    item: &Item,
    spec: &AddModSpec,
    floor: u32,
    task: &CraftingTask<'_>,
) -> Vec<(Item, f64)> {
    if !item.is_modifiable() || item.rarity != spec.require {
        return self_loop(item);
    }
    let Some(slots) = slot_distribution(item, spec.max_slots) else {
        // AffixSlotFull → failed apply → unchanged item.
        return self_loop(item);
    };
    let mut out: Vec<(Item, f64)> = Vec::new();
    for (affix, p_slot) in slots {
        // EMPTY base registry: parity with simulate()'s apply_currency
        // (no-bases) path — see the module docs' fidelity contract.
        let pool = enumerate_eligible_mods(
            task.registry,
            &poc2_engine::base_registry::EMPTY,
            item,
            affix,
            task.patch,
            floor,
            BASIC_ORB_EXCLUDES,
        );
        let total: f64 = pool.iter().map(|(_, w)| *w).sum();
        if pool.is_empty() || total <= 0.0 {
            // NoEligibleMods on the drawn slot → failed apply → self-loop.
            out.push((item.clone(), p_slot));
            continue;
        }
        for (idx, w) in pool {
            let Some(def) = task.registry.at(idx) else {
                continue;
            };
            let mut next = item.clone();
            if let Some(promoted) = spec.promote {
                next.rarity = promoted;
            }
            push_roll(&mut next, mk_roll(def));
            out.push((next, p_slot * w / total));
        }
    }
    out
}

/// Exact distribution for Annulment: uniform over removable mods.
fn annul_distribution(item: &Item) -> Vec<(Item, f64)> {
    if !item.is_modifiable() || !matches!(item.rarity, Rarity::Magic | Rarity::Rare) {
        return self_loop(item);
    }
    let removables = collect_removable_filtered(item, None, false);
    if removables.is_empty() {
        return self_loop(item);
    }
    let p = 1.0 / removables.len() as f64;
    removables
        .into_iter()
        .map(|(affix, idx)| {
            let mut next = item.clone();
            match affix {
                AffixType::Prefix => {
                    next.prefixes.remove(idx);
                }
                AffixType::Suffix => {
                    next.suffixes.remove(idx);
                }
                _ => {}
            }
            (next, p)
        })
        .collect()
}

/// Exact distribution for PoE2 Chaos (remove-one-add-one): uniform removal
/// convolved with the post-removal add distribution. A failed add restores
/// the *original* item (engine-orchestrator atomicity).
fn chaos_distribution(item: &Item, floor: u32, task: &CraftingTask<'_>) -> Vec<(Item, f64)> {
    if !item.is_modifiable() || item.rarity != Rarity::Rare {
        return self_loop(item);
    }
    let removables = collect_removable_filtered(item, None, false);
    if removables.is_empty() {
        return self_loop(item);
    }
    let p_removal = 1.0 / removables.len() as f64;
    let mut out: Vec<(Item, f64)> = Vec::new();
    for (removed_affix, idx) in removables {
        let mut post = item.clone();
        match removed_affix {
            AffixType::Prefix => {
                post.prefixes.remove(idx);
            }
            AffixType::Suffix => {
                post.suffixes.remove(idx);
            }
            _ => {}
        }
        let Some(slots) = slot_distribution(&post, 3) else {
            // Cannot happen (a slot just opened), but keep the engine's
            // failure semantics: atomic restore of the original.
            out.push((item.clone(), p_removal));
            continue;
        };
        for (new_affix, p_slot) in slots {
            let pool = enumerate_eligible_mods(
                task.registry,
                &poc2_engine::base_registry::EMPTY,
                &post,
                new_affix,
                task.patch,
                floor,
                BASIC_ORB_EXCLUDES,
            );
            let total: f64 = pool.iter().map(|(_, w)| *w).sum();
            if pool.is_empty() || total <= 0.0 {
                // Add step failed → orchestrator restores the ORIGINAL item.
                out.push((item.clone(), p_removal * p_slot));
                continue;
            }
            for (pool_idx, w) in pool {
                let Some(def) = task.registry.at(pool_idx) else {
                    continue;
                };
                let mut next = post.clone();
                push_roll(&mut next, mk_roll(def));
                out.push((next, p_removal * p_slot * w / total));
            }
        }
    }
    out
}

/// Divine rerolls values only — [`FeatureVec`] never reads them, so the
/// transition is the identity with probability 1 (success and failure are
/// indistinguishable in feature space).
fn divine_distribution(item: &Item) -> Vec<(Item, f64)> {
    self_loop(item)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::goal::Goal;
    use crate::training::model_learner::{learn_transition_model, LearnConfig};
    use crate::training::value_iteration::{value_iteration, ValueIterationConfig};
    use poc2_engine::base_registry::BaseRegistry;
    use poc2_engine::currency::DefaultCurrencyResolver;
    use poc2_engine::ids::{BaseTypeId, ConceptId, ItemClassId, ModGroupId, ModId, StatId, TagId};
    use poc2_engine::item::QualityKind;
    use poc2_engine::mods::{ModDomain, ModFlags, ModGroup, ModKind, ModStat, SpawnWeight};
    use poc2_engine::patch::{PatchRange, PatchVersion};
    use poc2_engine::registry::ModRegistry;
    use poc2_engine::weights::{Confidence, WeightObservation, WeightScope};
    use poc2_market::DivEquiv;
    use poc2_strategies::{Target, TargetSpec};
    use smallvec::smallvec;

    const CLASS: &str = "BodyArmour";

    fn mk_mod(
        id: &str,
        group: &str,
        affix: AffixType,
        concept: &str,
        required_level: u32,
    ) -> ModDefinition {
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
            required_level,
            tier: None,
            allowed_item_classes: smallvec![ItemClassId::from(CLASS)],
            patch_range: PatchRange::ALL,
            flags: ModFlags::empty(),
            text_template: None,
        }
    }

    fn weight(id: &str, w: f64) -> WeightObservation {
        WeightObservation {
            mod_id: ModId::from(id),
            scope: WeightScope::ItemClass {
                item_class: ItemClassId::from(CLASS),
            },
            primary_weight: w,
            secondary_weight: None,
            confidence: Confidence::Verified,
            note: None,
        }
    }

    /// Registry with weighted prefix/suffix pools across two groups per
    /// side, so slot draws, weighted picks, and group exclusion all show
    /// up in the distributions.
    fn fixture_registry() -> ModRegistry {
        ModRegistry::from_mods(
            vec![
                mk_mod("ES1", "ES", AffixType::Prefix, "EnergyShield", 1),
                mk_mod("Life1", "Life", AffixType::Prefix, "Life", 1),
                mk_mod("Armour1", "Armour", AffixType::Prefix, "Armour", 1),
                mk_mod(
                    "FireRes1",
                    "FireRes",
                    AffixType::Suffix,
                    "FireResistance",
                    1,
                ),
                mk_mod(
                    "ColdRes1",
                    "ColdRes",
                    AffixType::Suffix,
                    "ColdResistance",
                    1,
                ),
            ],
            vec![
                weight("ES1", 300.0),
                weight("Life1", 100.0),
                weight("Armour1", 50.0),
                weight("FireRes1", 200.0),
                weight("ColdRes1", 100.0),
            ],
        )
    }

    fn mk_item(rarity: Rarity, prefixes: &[&str], suffixes: &[&str]) -> Item {
        let roll = |id: &&str, affix| ModRoll {
            mod_id: ModId::from(*id),
            affix_type: affix,
            kind: ModKind::Explicit,
            values: smallvec![],
            is_fractured: false,
        };
        Item {
            base: BaseTypeId::from(CLASS),
            ilvl: 82,
            rarity,
            corrupted: false,
            sanctified: false,
            mirrored: false,
            quality: 0,
            quality_kind: QualityKind::Untagged,
            implicits: smallvec![],
            prefixes: prefixes
                .iter()
                .map(|id| roll(id, AffixType::Prefix))
                .collect(),
            suffixes: suffixes
                .iter()
                .map(|id| roll(id, AffixType::Suffix))
                .collect(),
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

    fn currency(id: &str) -> AdvisorAction {
        AdvisorAction::ApplyCurrency {
            currency: CurrencyId::from(id),
            omens: vec![],
        }
    }

    fn basic_actions() -> Vec<AdvisorAction> {
        vec![
            currency("OrbOfTransmutation"),
            currency("OrbOfAugmentation"),
            currency("RegalOrb"),
            currency("ExaltedOrb"),
            currency("ChaosOrb"),
            currency("OrbOfAnnulment"),
            currency("DivineOrb"),
        ]
    }

    fn task<'a>(
        registry: &'a ModRegistry,
        base_registry: &'a BaseRegistry,
        resolver: &'a DefaultCurrencyResolver,
        initial: Item,
    ) -> CraftingTask<'a> {
        CraftingTask {
            initial_item: initial,
            goal: es_fire_goal(),
            registry,
            base_registry,
            resolver,
            patch: PatchVersion::PATCH_0_5_0,
            omens: poc2_engine::omen::OmenSet::new(),
        }
    }

    /// L1 distance between two categorical distributions over FeatureVec.
    fn l1(a: &[(FeatureVec, f64)], b: &[(FeatureVec, f64)]) -> f64 {
        let mut keys: AHashSet<FeatureVec> = AHashSet::new();
        keys.extend(a.iter().map(|(k, _)| *k));
        keys.extend(b.iter().map(|(k, _)| *k));
        let lookup = |v: &[(FeatureVec, f64)], k: FeatureVec| {
            v.iter().find(|(kk, _)| *kk == k).map_or(0.0, |(_, p)| *p)
        };
        keys.iter()
            .map(|k| (lookup(a, *k) - lookup(b, *k)).abs())
            .sum()
    }

    /// Direct MC estimate of `P(FeatureVec' | item, action)` by sampling
    /// the simulator — the ground truth the analytic construction must hit.
    fn mc_distribution(
        item: &Item,
        action: &AdvisorAction,
        t: &CraftingTask<'_>,
        n: u32,
        seed: u64,
    ) -> Vec<(FeatureVec, f64)> {
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(seed);
        let mut counts: AHashMap<FeatureVec, u64> = AHashMap::new();
        for _ in 0..n {
            let s = rng.next_u64();
            let out = simulate(item, action, &t.omens, t.registry, t.resolver, t.patch, s);
            *counts
                .entry(featurize(&out.item, &t.goal, t.registry))
                .or_insert(0) += 1;
        }
        let mut v: Vec<(FeatureVec, f64)> = counts
            .into_iter()
            .map(|(k, c)| (k, c as f64 / f64::from(n)))
            .collect();
        v.sort_by_key(|a| a.0.pack());
        v
    }

    /// Bucket an analytic `(next_item, p)` list into a FeatureVec
    /// categorical, mirroring what the learner inserts.
    fn analytic_distribution(
        item: &Item,
        action: &AdvisorAction,
        t: &CraftingTask<'_>,
    ) -> Vec<(FeatureVec, f64)> {
        let pairs = analytic_transition(item, action, t)
            .unwrap_or_else(|| panic!("no closed form for {action:?}"));
        let mut dist: AHashMap<FeatureVec, f64> = AHashMap::new();
        for (next, p) in pairs {
            *dist
                .entry(featurize(&next, &t.goal, t.registry))
                .or_insert(0.0) += p;
        }
        let mut v: Vec<(FeatureVec, f64)> = dist.into_iter().collect();
        v.sort_by_key(|a| a.0.pack());
        v
    }

    /// The fidelity pin: for a sweep of concrete items covering every
    /// rarity / occupancy / fracture shape, the analytic distribution of
    /// every basic-orb action must agree with a high-sample Monte Carlo
    /// estimate of the engine's own sampler. A disagreement is a bug in
    /// either the analytic construction or the engine sampling path.
    ///
    /// (Comparison is per concrete item, NOT per afterstate alias: aliases
    /// collapse states whose occupied mod-groups differ, so two learners
    /// may legitimately pick different representative items for the same
    /// alias — a documented approximation of the aliasing design.)
    #[test]
    fn analytic_cross_validates_against_monte_carlo_per_item() {
        let registry = fixture_registry();
        let base_registry = BaseRegistry::default();
        let resolver = DefaultCurrencyResolver::new();
        let t = task(
            &registry,
            &base_registry,
            &resolver,
            mk_item(Rarity::Normal, &[], &[]),
        );

        let mut fractured_rare = mk_item(Rarity::Rare, &["Life1", "ES1"], &["FireRes1"]);
        fractured_rare.prefixes[0].is_fractured = true;

        let items: Vec<Item> = vec![
            mk_item(Rarity::Normal, &[], &[]),
            mk_item(Rarity::Magic, &["Life1"], &[]),
            mk_item(Rarity::Magic, &["ES1"], &[]),
            mk_item(Rarity::Magic, &[], &["FireRes1"]),
            mk_item(Rarity::Magic, &["Life1"], &["ColdRes1"]),
            mk_item(Rarity::Rare, &["Life1"], &[]),
            mk_item(Rarity::Rare, &["Life1", "ES1"], &["ColdRes1"]),
            mk_item(Rarity::Rare, &["Life1", "ES1", "Armour1"], &["ColdRes1"]),
            mk_item(
                Rarity::Rare,
                &["Life1", "ES1", "Armour1"],
                &["ColdRes1", "FireRes1"],
            ),
            fractured_rare,
        ];
        let actions: Vec<AdvisorAction> = vec![
            currency("OrbOfTransmutation"),
            currency("PerfectOrbOfTransmutation"),
            currency("OrbOfAugmentation"),
            currency("RegalOrb"),
            currency("ExaltedOrb"),
            currency("ChaosOrb"),
            currency("PerfectChaosOrb"),
            currency("OrbOfAnnulment"),
            currency("DivineOrb"),
        ];

        let mut seed = 0x00F1_DE11_u64;
        for (i, item) in items.iter().enumerate() {
            for action in &actions {
                seed = seed.wrapping_add(0x9E37_79B9);
                let an = analytic_distribution(item, action, &t);
                let total: f64 = an.iter().map(|(_, p)| *p).sum();
                assert!(
                    (total - 1.0).abs() < 1e-9,
                    "item#{i} {action:?}: analytic mass {total}"
                );
                let mc = mc_distribution(item, action, &t, 20_000, seed);
                let d = l1(&an, &mc);
                assert!(
                    d < 0.03,
                    "item#{i} {action:?}: L1(analytic, mc) = {d:.4}\n an={an:?}\n mc={mc:?}"
                );
            }
        }
    }

    /// Full-model smoke: the analytic learner covers at least every alias
    /// the MC learner discovers on the same task, and every entry is a
    /// valid categorical.
    #[test]
    fn analytic_full_model_covers_mc_alias_set() {
        let registry = fixture_registry();
        let base_registry = BaseRegistry::default();
        let resolver = DefaultCurrencyResolver::new();
        let t = task(
            &registry,
            &base_registry,
            &resolver,
            mk_item(Rarity::Normal, &[], &[]),
        );

        let analytic = learn_transition_model_analytic(
            &t,
            AnalyticConfig {
                max_states: 2_000,
                ..AnalyticConfig::default()
            },
            |_i, _g| basic_actions(),
        );
        let mc = learn_transition_model(
            &t,
            LearnConfig {
                samples_per_state_action: 2_000,
                afterstate_aliasing: true,
                seed: 0x00C0_FFEE,
                max_states: 2_000,
                max_actions_per_state: 64,
            },
            |_i, _g| basic_actions(),
        );

        assert!(analytic.entry_count() > 0);
        for alias in mc.aliases() {
            let an_dist = analytic
                .distribution_pairs_by_alias(alias)
                .unwrap_or_else(|| panic!("analytic model missing alias {alias:?}"));
            let total: f64 = an_dist.iter().map(|(_, p)| *p).sum();
            assert!(
                (total - 1.0).abs() < 1e-9,
                "alias {alias:?}: analytic mass {total}"
            );
        }
    }

    #[test]
    fn analytic_transmute_matches_engine_weights_exactly() {
        // Normal item, prefix pool = ES1(300) Life1(100) Armour1(50),
        // suffix pool = FireRes1(200) ColdRes1(100). Transmute flips a fair
        // coin between the sides. ES1 satisfies goal-spec bit 0, FireRes1
        // satisfies bit 1 (first suffix spec).
        let registry = fixture_registry();
        let base_registry = BaseRegistry::default();
        let resolver = DefaultCurrencyResolver::new();
        let t = task(
            &registry,
            &base_registry,
            &resolver,
            mk_item(Rarity::Normal, &[], &[]),
        );
        let model = learn_transition_model_analytic(&t, AnalyticConfig::default(), |_i, _g| {
            vec![currency("OrbOfTransmutation")]
        });
        let f0 = featurize(&t.initial_item, &t.goal, &registry);
        let dist = model
            .distribution_pairs(f0, &currency("OrbOfTransmutation"), true)
            .expect("transmute entry");
        let total: f64 = dist.iter().map(|(_, p)| *p).sum();
        assert!((total - 1.0).abs() < 1e-9);

        // P(Magic prefix with ES1 → bit0) = 0.5 × 300/450.
        let p_es = dist
            .iter()
            .find(|(s, _)| s.rarity == 1 && s.n_prefixes == 1 && s.target_match == 0b01)
            .map_or(0.0, |(_, p)| *p);
        assert!((p_es - 0.5 * 300.0 / 450.0).abs() < 1e-9, "p_es={p_es}");
        // P(Magic suffix with FireRes1 → bit1) = 0.5 × 200/300.
        let p_fire = dist
            .iter()
            .find(|(s, _)| s.rarity == 1 && s.n_suffixes == 1 && s.target_match == 0b10)
            .map_or(0.0, |(_, p)| *p);
        assert!(
            (p_fire - 0.5 * 200.0 / 300.0).abs() < 1e-9,
            "p_fire={p_fire}"
        );
        // Non-goal outcomes: prefix Life1/Armour1 (bit clear) and suffix
        // ColdRes1: 0.5×150/450 + 0.5×100/300.
        let p_other: f64 = dist
            .iter()
            .filter(|(s, _)| s.target_match == 0)
            .map(|(_, p)| *p)
            .sum();
        assert!(
            (p_other - (0.5 * 150.0 / 450.0 + 0.5 * 100.0 / 300.0)).abs() < 1e-9,
            "p_other={p_other}"
        );
    }

    #[test]
    fn analytic_divine_is_feature_space_identity() {
        let registry = fixture_registry();
        let base_registry = BaseRegistry::default();
        let resolver = DefaultCurrencyResolver::new();
        let item = mk_item(Rarity::Rare, &["ES1"], &["FireRes1"]);
        let t = task(&registry, &base_registry, &resolver, item.clone());
        let model = learn_transition_model_analytic(&t, AnalyticConfig::default(), |_i, _g| {
            vec![currency("DivineOrb")]
        });
        let f0 = featurize(&item, &t.goal, &registry);
        // The fixture item satisfies the goal → initial state is terminal
        // and gets no entries. Use a non-satisfying Rare instead.
        assert!(model
            .distribution_pairs(f0, &currency("DivineOrb"), true)
            .is_none());

        let item2 = mk_item(Rarity::Rare, &["Life1"], &["ColdRes1"]);
        let t2 = task(&registry, &base_registry, &resolver, item2.clone());
        let model2 = learn_transition_model_analytic(&t2, AnalyticConfig::default(), |_i, _g| {
            vec![currency("DivineOrb")]
        });
        let f2 = featurize(&item2, &t2.goal, &registry);
        let dist = model2
            .distribution_pairs(f2, &currency("DivineOrb"), true)
            .expect("divine entry");
        assert_eq!(dist.len(), 1, "divine must be identity: {dist:?}");
        assert_eq!(dist[0].0, f2);
        assert!((dist[0].1 - 1.0).abs() < 1e-12);
    }

    #[test]
    fn analytic_failure_paths_self_loop() {
        let registry = fixture_registry();
        let base_registry = BaseRegistry::default();
        let resolver = DefaultCurrencyResolver::new();
        // Wrong rarity: Exalt on a Normal item → self-loop with p=1.
        let item = mk_item(Rarity::Normal, &[], &[]);
        let t = task(&registry, &base_registry, &resolver, item.clone());
        let model = learn_transition_model_analytic(&t, AnalyticConfig::default(), |_i, _g| {
            vec![currency("ExaltedOrb"), currency("OrbOfAnnulment")]
        });
        let f0 = featurize(&item, &t.goal, &registry);
        for action in [currency("ExaltedOrb"), currency("OrbOfAnnulment")] {
            let dist = model
                .distribution_pairs(f0, &action, true)
                .expect("failure entry");
            assert_eq!(dist.len(), 1, "{action:?} should self-loop: {dist:?}");
            assert_eq!(dist[0].0, f0);
            assert!((dist[0].1 - 1.0).abs() < 1e-12);
        }
    }

    #[test]
    fn analytic_perfect_transmute_honours_floor_and_keep_one_tier_exception() {
        // Life group has T3 (rl 1) + T1 (rl 75): the Perfect floor (70)
        // keeps only T1 directly. Mana group has ONLY a sub-floor tier →
        // the keep-≥1-tier exception re-adds Mana_T3. Suffix side has no
        // mods at all → the suffix coin-flip branch fails → self-loop mass.
        let registry = ModRegistry::from_mods(
            vec![
                mk_mod("Life_T3", "Life", AffixType::Prefix, "Life", 1),
                mk_mod("Life_T1", "Life", AffixType::Prefix, "Life", 75),
                mk_mod("Mana_T3", "Mana", AffixType::Prefix, "Mana", 1),
            ],
            vec![
                weight("Life_T3", 1000.0),
                weight("Life_T1", 100.0),
                weight("Mana_T3", 200.0),
            ],
        );
        let base_registry = BaseRegistry::default();
        let resolver = DefaultCurrencyResolver::new();
        let item = mk_item(Rarity::Normal, &[], &[]);
        let t = task(&registry, &base_registry, &resolver, item.clone());
        let action = currency("PerfectOrbOfTransmutation");
        let model = learn_transition_model_analytic(&t, AnalyticConfig::default(), |_i, _g| {
            vec![action.clone()]
        });
        let f0 = featurize(&item, &t.goal, &registry);
        let dist = model
            .distribution_pairs(f0, &action, true)
            .expect("perfect transmute entry");
        // Self-loop (suffix branch, empty pool) carries exactly 0.5.
        let p_self = dist.iter().find(|(s, _)| *s == f0).map_or(0.0, |(_, p)| *p);
        assert!((p_self - 0.5).abs() < 1e-9, "p_self={p_self}");
        // Prefix branch: pool = Life_T1 (100, inclusive) + Mana_T3 (200,
        // floor exception). Mana outcome has target_match 0 (Mana is not a
        // goal concept), Life outcome likewise 0 — both are Magic 1-prefix
        // states, so they collapse to ONE FeatureVec with p=0.5. Cross-check
        // via the MC learner for the exact same alias.
        let mc = learn_transition_model(
            &t,
            LearnConfig {
                samples_per_state_action: 20_000,
                afterstate_aliasing: true,
                seed: 7,
                max_states: 100,
                max_actions_per_state: 4,
            },
            |_i, _g| vec![action.clone()],
        );
        let mc_dist = mc.distribution_pairs(f0, &action, true).unwrap();
        let d = l1(&dist, &mc_dist);
        assert!(d < 0.03, "L1={d}: analytic={dist:?} mc={mc_dist:?}");
    }

    #[test]
    fn analytic_model_is_deterministic() {
        let registry = fixture_registry();
        let base_registry = BaseRegistry::default();
        let resolver = DefaultCurrencyResolver::new();
        let t = task(
            &registry,
            &base_registry,
            &resolver,
            mk_item(Rarity::Normal, &[], &[]),
        );
        let a = learn_transition_model_analytic(&t, AnalyticConfig::default(), |_i, _g| {
            basic_actions()
        });
        let b = learn_transition_model_analytic(&t, AnalyticConfig::default(), |_i, _g| {
            basic_actions()
        });
        assert_eq!(a.entry_count(), b.entry_count());
        for alias in a.aliases() {
            assert_eq!(
                a.distribution_pairs_by_alias(alias),
                b.distribution_pairs_by_alias(alias),
                "alias {alias:?} differs between runs"
            );
        }
    }

    #[test]
    fn value_iteration_consumes_analytic_model() {
        // End-to-end smoke: analytic model + Bellman solve. The ES+Fire
        // goal is reachable from a Normal item via Transmute/Aug/Regal/
        // Exalt chains, so V(s0) must be finite and negative (steps cost
        // -1) and strictly better than the -1000 degenerate floor.
        let registry = fixture_registry();
        let base_registry = BaseRegistry::default();
        let resolver = DefaultCurrencyResolver::new();
        let t = task(
            &registry,
            &base_registry,
            &resolver,
            mk_item(Rarity::Normal, &[], &[]),
        );
        let model = learn_transition_model_analytic(&t, AnalyticConfig::default(), |_i, _g| {
            basic_actions()
        });
        let actions = basic_actions();
        // Terminal: both goal bits set.
        let result = value_iteration(
            &model,
            &actions,
            true,
            |s: &FeatureVec| (s.target_match & 0b11) == 0b11,
            |_s, _a| -1.0,
            ValueIterationConfig::default(),
        );
        let f0 = featurize(&t.initial_item, &t.goal, &registry);
        let v0 = result.value.get(&f0).copied().expect("V(s0)");
        assert!(v0.is_finite());
        assert!(v0 < 0.0, "V(s0)={v0} should cost steps");
        assert!(
            v0 > -100.0,
            "V(s0)={v0} should be reachable well before -100"
        );
        assert!(result.final_delta < 1e-6, "VI should converge");
    }
}
