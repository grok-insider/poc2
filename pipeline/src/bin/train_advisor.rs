//! M16.6 — `train-advisor` binary.
//!
//! Loads a training corpus (`pipeline/data/training_goals.toml` by default)
//! and runs the offline training pipeline:
//!
//! 1. For each goal, build a [`CraftingTask`] over a synthetic registry
//!    (or a bundle when `--bundle` is supplied).
//! 2. Run [`learn_transition_model`] to produce the per-action transition
//!    model `P(s' | s, a)`.
//! 3. Solve the Bellman equation twice via [`value_iteration`] — once
//!    with the path-length reward, once with the cost reward.
//! 4. Package the results into a [`TrainedModel`] per goal × class.
//! 5. Serialize the `Vec<TrainedModel>` to JSON (or bincode when
//!    `--format bincode` is supplied) and write to `--out`.
//!
//! ## Smoke vs production parameters
//!
//! v3 ships a smoke-level default (`--samples 1000`) that completes in
//! seconds and verifies the pipeline end-to-end. Production training
//! uses `--samples 100000` per Britz; full corpus training takes a few
//! hours per patch.
//!
//! Reference: `docs/81-engine-training-and-rule-encoding-plan.md` §6.6
//! Tier 3.6.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use clap::Parser;
use poc2_advisor::action::AdvisorAction;
use poc2_advisor::featurize::FeatureVec;
use poc2_advisor::training::{
    learn_transition_model, trained_model_from, value_iteration, CraftingTask, LearnConfig,
    RewardKind, TrainedModelArtefact, TrainingArtefactMetrics, ValueIterationConfig,
};
use poc2_advisor::{featurize, Goal};
use poc2_data::Bundle;
use poc2_engine::base_registry::BaseRegistry;
use poc2_engine::currency::DefaultCurrencyResolver;
use poc2_engine::ids::{BaseTypeId, ConceptId, CurrencyId, ItemClassId};
use poc2_engine::item::{Item, QualityKind, Rarity};
use poc2_engine::omen::OmenSet;
use poc2_engine::patch::PatchVersion;
use poc2_engine::registry::ModRegistry;
use poc2_engine::ENGINE_SCHEMA_VERSION;
use poc2_market::DivEquiv;
use poc2_strategies::{Target, TargetSpec};
use serde::Deserialize;
use smallvec::smallvec;

/// Top-level CLI shape.
#[derive(Parser, Debug)]
#[command(
    name = "train-advisor",
    about = "Train the advisor's Q-tables for the canonical goal corpus."
)]
struct Cli {
    /// Path to the corpus TOML.
    #[arg(long, default_value = "pipeline/data/training_goals.toml")]
    corpus: PathBuf,

    /// Output trained-models artefact path.
    #[arg(long, default_value = "trained-models.json")]
    out: PathBuf,

    /// Monte Carlo samples per (state, action) pair. Smoke = 1000;
    /// production ship-prep = 100000.
    #[arg(long, default_value_t = 1_000)]
    samples: u32,

    /// Hard cap on reachable-state BFS. Truncates large state spaces.
    #[arg(long, default_value_t = 5_000)]
    max_states: u32,

    /// Disable afterstate aliasing. v3 default is on; turning it off
    /// produces a larger trained model with marginally higher fidelity
    /// at high sample counts.
    #[arg(long)]
    no_aliasing: bool,

    /// Bundle to load engine data (mods + bases + currency catalogues)
    /// from. When omitted, training runs against an empty synthetic
    /// registry — useful only for plumbing smoke tests; every goal's
    /// `V_path(s0)` degenerates to the floor because no currency can
    /// roll any mod and no terminal state is reachable. Always supply
    /// this flag for production training runs.
    #[arg(long)]
    bundle: Option<PathBuf>,

    /// Treat corpus-audit drops as a hard error instead of a warning.
    /// CI-friendly: fails fast when a goal references concepts that
    /// don't exist in the loaded bundle, so the canonical corpus stays
    /// in lock-step with the engine's concept taxonomy. Has no effect
    /// without `--bundle` (audit only runs when a bundle is loaded).
    #[arg(long)]
    strict_audit: bool,

    /// Output format.
    #[arg(long, default_value = "json")]
    format: OutputFormat,

    /// Verbose logging.
    #[arg(long)]
    verbose: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
enum OutputFormat {
    Json,
}

/// One entry in the corpus TOML — a serializable [`Goal`] specification.
#[derive(Debug, Clone, Deserialize)]
struct CorpusGoal {
    id: String,
    display_name: String,
    item_class: String,
    ilvl: u32,
    budget_div: f64,
    target: CorpusTarget,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct CorpusTarget {
    #[serde(default)]
    prefixes: Vec<CorpusTargetSpec>,
    #[serde(default)]
    suffixes: Vec<CorpusTargetSpec>,
}

#[derive(Debug, Clone, Deserialize)]
struct CorpusTargetSpec {
    #[serde(default)]
    concept: Option<String>,
    #[serde(default)]
    concept_any: Vec<String>,
    #[serde(default = "one")]
    count: u8,
    #[serde(default)]
    min_tier: Option<u8>,
    #[serde(default = "yes")]
    allow_hybrid: bool,
}

const fn one() -> u8 {
    1
}
const fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
struct CorpusFile {
    #[serde(default)]
    goal: Vec<CorpusGoal>,
}

// Output artefact and metrics structs are re-exported from
// `poc2_advisor::training::artefact` so the desktop loader can
// rehydrate the JSON without duplicating the schema.

fn lift_target(spec: &CorpusTargetSpec) -> TargetSpec {
    TargetSpec {
        concept: spec.concept.as_ref().map(|s| ConceptId::from(s.as_str())),
        concept_any: spec
            .concept_any
            .iter()
            .map(|s| ConceptId::from(s.as_str()))
            .collect(),
        affix: None,
        count: spec.count,
        min_tier: spec.min_tier,
        allow_hybrid: spec.allow_hybrid,
    }
}

fn build_goal(corpus_goal: &CorpusGoal) -> Goal {
    let target = Target {
        prefixes: corpus_goal
            .target
            .prefixes
            .iter()
            .map(lift_target)
            .collect(),
        suffixes: corpus_goal
            .target
            .suffixes
            .iter()
            .map(lift_target)
            .collect(),
        constraints: vec![],
    };
    Goal::new(target, DivEquiv::point(corpus_goal.budget_div))
}

fn build_initial_item(corpus_goal: &CorpusGoal, base: BaseTypeId) -> Item {
    Item {
        base,
        ilvl: corpus_goal.ilvl,
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

/// Resolve the corpus goal's item class to a concrete `BaseTypeId`
/// for the initial training item.
///
/// **Returns the v3 placeholder convention** (`BaseTypeId::from(class.as_str())`)
/// rather than a real bundle base id. The reason is that the
/// advisor's [`crate::simulate`] (and the planner that calls it) uses
/// [`poc2_engine::apply_currency`] — the variant that does NOT take a
/// [`BaseRegistry`] — so the engine's `class_for_item` falls through
/// to `ItemClassId::from(item.base.as_str())`. With the placeholder
/// convention, that returns the class id directly and mod-eligibility
/// (`for_class_affix(class, affix)`) finds the right mods. With a real
/// bundle base id (e.g., `Metadata/Items/Armours/.../Plate1`) the
/// fallback would produce an unknown class id and no mods would be
/// eligible — the training model would degenerate to V_path = -1000.
///
/// The trained models are class-level (not base-specific) — the
/// [`FeatureVec`] doesn't carry the base id, so per-base specialization
/// would be lost in the trained Q-table anyway. The `base_registry`
/// argument is retained for the symmetric `EngineContext` API and so a
/// future refactor that threads `base_registry` through the simulator
/// can switch this helper to real bundle bases without touching the
/// call sites.
fn pick_base_for_class(
    _base_registry: &BaseRegistry,
    class: &ItemClassId,
    _target_ilvl: u32,
) -> BaseTypeId {
    BaseTypeId::from(class.as_str())
}

/// Action set explored at every state during training. v3 trains on the
/// basic-orb catalogue + a Greater Essence to seed a deterministic
/// rare-promotion path. Production training adds bones and Vaal but
/// requires a populated bundle for class-aware bone reveal.
fn enumerate_basic_actions() -> Vec<AdvisorAction> {
    vec![
        AdvisorAction::ApplyCurrency {
            currency: CurrencyId::from("PerfectOrbOfTransmutation"),
            omens: vec![],
        },
        AdvisorAction::ApplyCurrency {
            currency: CurrencyId::from("PerfectOrbOfAugmentation"),
            omens: vec![],
        },
        AdvisorAction::ApplyCurrency {
            currency: CurrencyId::from("PerfectRegalOrb"),
            omens: vec![],
        },
        AdvisorAction::ApplyCurrency {
            currency: CurrencyId::from("PerfectExaltedOrb"),
            omens: vec![],
        },
        AdvisorAction::ApplyCurrency {
            currency: CurrencyId::from("PerfectChaosOrb"),
            omens: vec![],
        },
        AdvisorAction::ApplyCurrency {
            currency: CurrencyId::from("OrbOfAnnulment"),
            omens: vec![],
        },
        AdvisorAction::ApplyCurrency {
            currency: CurrencyId::from("DivineOrb"),
            omens: vec![],
        },
    ]
}

fn cost_for_action(action: &AdvisorAction) -> f64 {
    // Synthetic cost-reward weights — the production training binary
    // wires in a real `Valuator` so live league prices drive the cost
    // reward. Smoke training uses these stable defaults so the trained
    // models are reproducible across runs.
    let AdvisorAction::ApplyCurrency { currency, .. } = action else {
        return 0.05;
    };
    match currency.as_str() {
        "PerfectOrbOfTransmutation" => 0.5,
        "PerfectOrbOfAugmentation" => 0.4,
        "PerfectRegalOrb" => 0.6,
        "PerfectExaltedOrb" => 1.5,
        "PerfectChaosOrb" => 0.8,
        "OrbOfAnnulment" => 0.3,
        "DivineOrb" => 0.5,
        _ => 0.05,
    }
}

/// Engine context passed to every `train_one_goal` call. Built once in
/// `main()` from either the loaded bundle (`--bundle`) or a synthetic
/// empty registry (smoke testing only).
struct EngineContext {
    registry: ModRegistry,
    base_registry: BaseRegistry,
    resolver: DefaultCurrencyResolver,
    /// `true` when the context was built from a real bundle. Drives
    /// per-goal logging and gates the corpus audit.
    has_bundle: bool,
}

impl EngineContext {
    fn synthetic_empty() -> Self {
        Self {
            registry: ModRegistry::from_mods(vec![], vec![]),
            base_registry: BaseRegistry::default(),
            resolver: DefaultCurrencyResolver::new(),
            has_bundle: false,
        }
    }

    fn from_bundle(bundle: Bundle) -> Self {
        let essences = bundle.essence_catalogue();
        let catalysts = bundle.catalyst_catalogue();
        let mut alloys = bundle.alloy_catalogue();
        alloys.extend(bundle.emotion_catalogue());
        let base_registry = BaseRegistry::from_bases(bundle.base_items);
        let registry = ModRegistry::from_mods(bundle.mods, bundle.weights);
        let resolver = DefaultCurrencyResolver::new()
            .with_essences(essences)
            .with_catalysts(catalysts)
            .with_alloys(alloys);
        Self {
            registry,
            base_registry,
            resolver,
            has_bundle: true,
        }
    }
}

/// Build the bitmap-full terminal predicate for `goal`.
///
/// Returns a closure that fires when every spec the goal cares about
/// has its corresponding `target_match` bit set. Uses the same
/// 16-spec cap as [`FeatureVec::target_match`].
///
/// **Caveat**: the bitmap captures *presence* of a satisfying mod, not
/// the spec's `count` / `min_tier` constraints. For goals with
/// `count > 1` or `min_tier > None` the trained `V_path` will be
/// slightly *over-optimistic* (terminal fires earlier than real
/// satisfaction). Real satisfaction is re-checked by the desktop
/// planner via [`is_satisfied_with_ctx`] at runtime; the trained Q
/// gives directional guidance, not the final yes/no.
fn build_terminal_predicate(goal: &Goal) -> impl Fn(&FeatureVec) -> bool {
    let n_specs = (goal.target.prefixes.len() + goal.target.suffixes.len()).min(16);
    let mask: u16 = if n_specs == 16 {
        u16::MAX
    } else if n_specs == 0 {
        // Empty target — never terminal (defensive; the planner
        // short-circuits empty goals before reaching here).
        0
    } else {
        (1u16 << n_specs) - 1
    };
    move |state: &FeatureVec| mask != 0 && (state.target_match & mask) == mask
}

fn train_one_goal(
    corpus_goal: &CorpusGoal,
    ctx: &EngineContext,
    samples: u32,
    max_states: u32,
    afterstate_aliasing: bool,
    verbose: bool,
) -> Result<TrainedModelArtefact> {
    let goal = build_goal(corpus_goal);
    let item_class = ItemClassId::from(corpus_goal.item_class.as_str());
    let base = pick_base_for_class(&ctx.base_registry, &item_class, corpus_goal.ilvl);
    let initial_item = build_initial_item(corpus_goal, base);

    let task = CraftingTask {
        initial_item: initial_item.clone(),
        goal: goal.clone(),
        registry: &ctx.registry,
        base_registry: &ctx.base_registry,
        resolver: &ctx.resolver,
        patch: PatchVersion::PATCH_0_4_0,
        omens: OmenSet::new(),
    };

    let learn_config = LearnConfig {
        samples_per_state_action: samples,
        afterstate_aliasing,
        seed: 0x_5EED_C0DE_C0DE_5EED,
        max_states,
        max_actions_per_state: 32,
    };

    if verbose {
        eprintln!(
            "training `{}` (class={}, base={}, ilvl={}, budget={}): samples/pair={}",
            corpus_goal.id,
            corpus_goal.item_class,
            initial_item.base.as_str(),
            corpus_goal.ilvl,
            corpus_goal.budget_div,
            samples
        );
    }

    let actions = enumerate_basic_actions();
    let actions_clone = actions.clone();
    let model = learn_transition_model(&task, learn_config, move |_item, _goal| {
        actions_clone.clone()
    });

    let initial_features = featurize(&initial_item, &goal, &ctx.registry);
    let value_config = ValueIterationConfig::default();
    let terminal = build_terminal_predicate(&goal);

    let path_result = value_iteration(
        &model,
        &actions,
        afterstate_aliasing,
        &terminal,
        |_state, _action| -1.0,
        value_config,
    );
    let cost_result = value_iteration(
        &model,
        &actions,
        afterstate_aliasing,
        &terminal,
        |_state, action| -cost_for_action(action),
        value_config,
    );

    let goal_h = poc2_advisor::training::goal_hash(&goal);

    let model_path_length = trained_model_from(
        goal_h,
        item_class.clone(),
        poc2_data::BUNDLE_SCHEMA_VERSION,
        ENGINE_SCHEMA_VERSION,
        RewardKind::PathLength,
        &path_result,
        Some(&cost_result),
    );
    let model_cost = trained_model_from(
        goal_h,
        item_class.clone(),
        poc2_data::BUNDLE_SCHEMA_VERSION,
        ENGINE_SCHEMA_VERSION,
        RewardKind::Cost,
        &cost_result,
        Some(&cost_result),
    );

    let metrics = TrainingArtefactMetrics {
        states_visited: model.entry_count(),
        transitions_learned: model.aliases().len(),
        value_iteration_iters_path: path_result.iterations,
        value_iteration_iters_cost: cost_result.iterations,
        initial_state_v_path: path_result
            .value
            .get(&initial_features)
            .copied()
            .unwrap_or(0.0),
        initial_state_v_cost: cost_result
            .value
            .get(&initial_features)
            .copied()
            .unwrap_or(0.0),
    };

    Ok(TrainedModelArtefact {
        goal_id: corpus_goal.id.clone(),
        display_name: corpus_goal.display_name.clone(),
        item_class: corpus_goal.item_class.clone(),
        model_path_length,
        model_cost,
        metrics,
    })
}

// =========================================================================
// Bundle loading
// =========================================================================

/// Load a bundle from `path` and validate its schema. Returns a useful
/// rebuild-instruction error on schema mismatch so the operator
/// doesn't have to dig through the data crate's error type.
fn load_bundle(path: &Path) -> Result<Bundle> {
    let bundle = poc2_data::io::read_bundle(path)
        .with_context(|| format!("read bundle from {}", path.display()))?;
    if bundle.header.schema_version != poc2_data::BUNDLE_SCHEMA_VERSION {
        return Err(anyhow!(
            "bundle at {} has schema_version=v{} but train-advisor expects v{}. \
             Rebuild via `cargo run -p poc2-pipeline -- build --out {} --patch <patch>`.",
            path.display(),
            bundle.header.schema_version,
            poc2_data::BUNDLE_SCHEMA_VERSION,
            path.display(),
        ));
    }
    Ok(bundle)
}

// =========================================================================
// Corpus audit
// =========================================================================

/// Per-goal audit verdict.
#[derive(Debug, Clone)]
struct AuditEntry {
    goal_id: String,
    /// Concept ids referenced by the goal that are missing from the
    /// bundle's mod taxonomy. Empty when the goal is fully satisfiable.
    missing_concepts: Vec<String>,
}

#[derive(Debug, Clone)]
struct AuditReport {
    /// Goals whose targets reference only known concepts.
    kept: Vec<String>,
    /// Goals dropped because at least one target spec referenced an
    /// unknown concept.
    dropped: Vec<AuditEntry>,
}

impl AuditReport {
    fn print(&self) {
        eprintln!(
            "corpus audit: {} goal(s) trainable, {} dropped due to unknown concepts",
            self.kept.len(),
            self.dropped.len()
        );
        for entry in &self.dropped {
            eprintln!(
                "  drop `{}`: missing concepts = [{}]",
                entry.goal_id,
                entry.missing_concepts.join(", ")
            );
        }
    }
}

/// Collect every distinct `ConceptId` referenced by any mod in
/// `registry`. Used as the audit's "known concepts" set.
fn known_concepts(registry: &ModRegistry) -> HashSet<String> {
    let mut set = HashSet::new();
    for m in registry.iter() {
        for c in &m.concept_set {
            set.insert(c.as_str().to_string());
        }
    }
    set
}

/// Classify each corpus goal as trainable (every referenced concept
/// exists in the registry) or droppable (at least one missing concept).
fn audit_corpus(corpus: &CorpusFile, registry: &ModRegistry) -> AuditReport {
    let known = known_concepts(registry);
    let mut kept = Vec::new();
    let mut dropped = Vec::new();
    for goal in &corpus.goal {
        let mut missing: Vec<String> = Vec::new();
        let specs = goal
            .target
            .prefixes
            .iter()
            .chain(goal.target.suffixes.iter());
        for spec in specs {
            if let Some(c) = spec.concept.as_deref() {
                if !known.contains(c) {
                    missing.push(c.to_string());
                }
            }
            for c in &spec.concept_any {
                if !known.contains(c) {
                    missing.push(c.clone());
                }
            }
        }
        // Deduplicate so a concept referenced by multiple specs in the
        // same goal is reported once.
        missing.sort();
        missing.dedup();
        if missing.is_empty() {
            kept.push(goal.id.clone());
        } else {
            dropped.push(AuditEntry {
                goal_id: goal.id.clone(),
                missing_concepts: missing,
            });
        }
    }
    AuditReport { kept, dropped }
}

// =========================================================================
// Main
// =========================================================================

fn main() -> Result<()> {
    let cli = Cli::parse();
    let corpus_str = fs::read_to_string(&cli.corpus)
        .with_context(|| format!("read corpus from {}", cli.corpus.display()))?;
    let corpus: CorpusFile = toml::from_str(&corpus_str)
        .with_context(|| format!("parse corpus TOML at {}", cli.corpus.display()))?;
    if cli.verbose {
        eprintln!(
            "loaded {} goal(s) from {}",
            corpus.goal.len(),
            cli.corpus.display()
        );
    }

    // Build engine context: real bundle when --bundle is supplied, else
    // synthetic empty (smoke-only).
    let ctx = if let Some(path) = cli.bundle.as_ref() {
        let bundle = load_bundle(path)?;
        eprintln!(
            "loaded bundle {} (mods={}, bases={}, weights={}, essences={}, catalysts={})",
            path.display(),
            bundle.mods.len(),
            bundle.base_items.len(),
            bundle.weights.len(),
            bundle.essences.entries.len(),
            bundle.catalysts.entries.len(),
        );
        EngineContext::from_bundle(bundle)
    } else {
        eprintln!(
            "running with synthetic empty registry — V_path will degenerate to the floor for every goal. \
             Pass --bundle <path> for production training."
        );
        EngineContext::synthetic_empty()
    };

    // Audit the corpus when we have a real bundle. Drop unsatisfiable
    // goals (or fail-fast under --strict-audit).
    let trainable_ids: HashSet<String> = if ctx.has_bundle {
        let report = audit_corpus(&corpus, &ctx.registry);
        report.print();
        if cli.strict_audit && !report.dropped.is_empty() {
            return Err(anyhow!(
                "corpus audit dropped {} goal(s); --strict-audit requested fail-fast",
                report.dropped.len()
            ));
        }
        report.kept.into_iter().collect()
    } else {
        // No bundle ⇒ no audit ⇒ train every goal in the corpus.
        corpus.goal.iter().map(|g| g.id.clone()).collect()
    };

    let mut artefacts = Vec::with_capacity(trainable_ids.len());
    for goal in &corpus.goal {
        if !trainable_ids.contains(&goal.id) {
            continue;
        }
        let artefact = train_one_goal(
            goal,
            &ctx,
            cli.samples,
            cli.max_states,
            !cli.no_aliasing,
            cli.verbose,
        )?;
        if cli.verbose {
            eprintln!(
                "  done `{}`: {} states, V_path(s0)={:.4}, V_cost(s0)={:.4}",
                artefact.goal_id,
                artefact.metrics.states_visited,
                artefact.metrics.initial_state_v_path,
                artefact.metrics.initial_state_v_cost,
            );
        }
        artefacts.push(artefact);
    }

    let serialized = match cli.format {
        OutputFormat::Json => serde_json::to_string_pretty(&artefacts)
            .context("serialize trained-models artefact to JSON")?,
    };
    // Ensure the output directory exists before writing — saves the
    // operator one `mkdir -p` step on first run.
    if let Some(parent) = cli.out.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)
            .with_context(|| format!("create parent dir {}", parent.display()))?;
    }
    fs::write(&cli.out, serialized).with_context(|| format!("write {}", cli.out.display()))?;
    eprintln!(
        "wrote {} trained model(s) to {}",
        artefacts.len(),
        cli.out.display()
    );
    Ok(())
}

// =========================================================================
// Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use poc2_engine::ids::ModGroupId;
    use poc2_engine::mods::{ModDefinition, ModDomain, ModFlags, ModGroup, ModKind};
    use poc2_engine::patch::PatchRange;

    fn mk_mod(id: &str, concept: &str) -> ModDefinition {
        ModDefinition {
            id: id.into(),
            name: None,
            mod_group: ModGroup(ModGroupId::from(id)),
            affix_type: poc2_engine::item::AffixType::Prefix,
            kind: ModKind::Explicit,
            domain: ModDomain::Item,
            tags: smallvec![],
            concept_set: smallvec![ConceptId::from(concept)],
            spawn_weights: smallvec![],
            stats: smallvec![],
            required_level: 1,
            tier: None,
            allowed_item_classes: smallvec![],
            patch_range: PatchRange::ALL,
            flags: ModFlags::empty(),
            text_template: None,
        }
    }

    fn mk_corpus_goal(id: &str, class: &str, ilvl: u32, prefix_concept: &str) -> CorpusGoal {
        CorpusGoal {
            id: id.to_string(),
            display_name: id.to_string(),
            item_class: class.to_string(),
            ilvl,
            budget_div: 50.0,
            target: CorpusTarget {
                prefixes: vec![CorpusTargetSpec {
                    concept: Some(prefix_concept.to_string()),
                    concept_any: vec![],
                    count: 1,
                    min_tier: None,
                    allow_hybrid: true,
                }],
                suffixes: vec![],
            },
        }
    }

    // ---- pick_base_for_class -------------------------------------------

    #[test]
    fn pick_base_returns_class_id_placeholder() {
        // The current implementation always returns the class-id
        // placeholder so the engine's `class_for_item` (which uses the
        // EMPTY base_registry through `apply_currency`) falls through
        // correctly. See the helper's docstring for context.
        let registry = BaseRegistry::default();
        let pick = pick_base_for_class(&registry, &ItemClassId::from("BodyArmour"), 82);
        assert_eq!(pick.as_str(), "BodyArmour");
        let pick_helmet = pick_base_for_class(&registry, &ItemClassId::from("Helmet"), 50);
        assert_eq!(pick_helmet.as_str(), "Helmet");
    }

    #[test]
    fn pick_base_ignores_real_bundle_bases_for_now() {
        // Even when the BaseRegistry has real bundle bases for the
        // class, the helper still returns the placeholder id so the
        // back-compat `class_for_item` path resolves correctly.
        // This test guards against regression — flipping to real bases
        // requires the simulator to thread a base_registry through.
        use poc2_engine::base::{BaseType, InventorySize, ReleaseState};
        use poc2_engine::ids::TagId;
        use poc2_engine::item_class::AttributePool;
        let real_base = BaseType {
            id: BaseTypeId::from("Metadata/Items/Belts/HeavyBelt"),
            name: "Heavy Belt".into(),
            item_class: ItemClassId::from("Belt"),
            attribute_pool: AttributePool::Str,
            drop_level: 50,
            tags: smallvec![TagId::from("belt")],
            implicits: smallvec![],
            inventory: InventorySize {
                width: 2,
                height: 1,
            },
            release_state: ReleaseState::Released,
            patch_range: PatchRange::ALL,
        };
        let registry = BaseRegistry::from_bases(vec![real_base]);
        let pick = pick_base_for_class(&registry, &ItemClassId::from("Belt"), 82);
        assert_eq!(pick.as_str(), "Belt");
    }

    // ---- build_terminal_predicate --------------------------------------

    fn mk_goal_with_n_specs(n_prefixes: usize, n_suffixes: usize) -> Goal {
        let prefixes: Vec<TargetSpec> = (0..n_prefixes)
            .map(|_| TargetSpec {
                concept: Some(ConceptId::from("Life")),
                concept_any: vec![],
                affix: None,
                count: 1,
                min_tier: None,
                allow_hybrid: true,
            })
            .collect();
        let suffixes: Vec<TargetSpec> = (0..n_suffixes)
            .map(|_| TargetSpec {
                concept: Some(ConceptId::from("FireResistance")),
                concept_any: vec![],
                affix: None,
                count: 1,
                min_tier: None,
                allow_hybrid: true,
            })
            .collect();
        Goal::new(
            Target {
                prefixes,
                suffixes,
                constraints: vec![],
            },
            DivEquiv::point(50.0),
        )
    }

    fn fv(target_match: u16) -> FeatureVec {
        FeatureVec {
            rarity: 2,
            target_match,
            n_prefixes: 0,
            n_suffixes: 0,
            has_hidden_desecrated: false,
            has_fractured: false,
            is_corrupted: false,
            has_hinekora_lock: false,
            extra_flags: 0,
        }
    }

    #[test]
    fn terminal_predicate_fires_only_when_full_bitmap_set() {
        let goal = mk_goal_with_n_specs(1, 1);
        let terminal = build_terminal_predicate(&goal);
        // 0b00 — no specs satisfied → not terminal
        assert!(!terminal(&fv(0b00)));
        // 0b01 — only prefix → not terminal
        assert!(!terminal(&fv(0b01)));
        // 0b10 — only suffix → not terminal
        assert!(!terminal(&fv(0b10)));
        // 0b11 — both → terminal
        assert!(terminal(&fv(0b11)));
        // 0b111 — extra bit set beyond the goal's specs → still terminal
        // (the extra bits don't affect the mask check).
        assert!(terminal(&fv(0b111)));
    }

    #[test]
    fn terminal_predicate_never_fires_for_empty_goal() {
        let goal = mk_goal_with_n_specs(0, 0);
        let terminal = build_terminal_predicate(&goal);
        // An empty target is degenerate; the predicate must never fire
        // because the planner short-circuits empty goals upstream.
        assert!(!terminal(&fv(0)));
        assert!(!terminal(&fv(u16::MAX)));
    }

    // ---- audit_corpus --------------------------------------------------

    #[test]
    fn audit_keeps_known_concepts_drops_unknown() {
        let registry = ModRegistry::from_mods(vec![mk_mod("LifeMod", "Life")], vec![]);
        let corpus = CorpusFile {
            goal: vec![
                mk_corpus_goal("life-goal", "BodyArmour", 82, "Life"),
                mk_corpus_goal("es-goal", "BodyArmour", 82, "EnergyShield"),
            ],
        };
        let report = audit_corpus(&corpus, &registry);
        assert_eq!(report.kept, vec!["life-goal".to_string()]);
        assert_eq!(report.dropped.len(), 1);
        assert_eq!(report.dropped[0].goal_id, "es-goal");
        assert_eq!(report.dropped[0].missing_concepts, vec!["EnergyShield"]);
    }

    #[test]
    fn audit_de_duplicates_missing_concepts_per_goal() {
        let registry = ModRegistry::from_mods(vec![], vec![]);
        let corpus = CorpusFile {
            goal: vec![CorpusGoal {
                id: "g".into(),
                display_name: "g".into(),
                item_class: "Helmet".into(),
                ilvl: 82,
                budget_div: 50.0,
                target: CorpusTarget {
                    prefixes: vec![CorpusTargetSpec {
                        concept: Some("Life".to_string()),
                        concept_any: vec!["Life".into(), "EnergyShield".into()],
                        count: 1,
                        min_tier: None,
                        allow_hybrid: true,
                    }],
                    suffixes: vec![CorpusTargetSpec {
                        concept: Some("Life".to_string()),
                        concept_any: vec![],
                        count: 1,
                        min_tier: None,
                        allow_hybrid: true,
                    }],
                },
            }],
        };
        let report = audit_corpus(&corpus, &registry);
        assert!(report.kept.is_empty());
        assert_eq!(report.dropped.len(), 1);
        // Life appears 3 times across the spec set, EnergyShield once;
        // both should be reported once each, sorted.
        assert_eq!(
            report.dropped[0].missing_concepts,
            vec!["EnergyShield".to_string(), "Life".to_string()]
        );
    }

    // ---- simulator probe -----------------------------------------------

    /// Diagnostic: with a synthetic Life prefix mod and a placeholder
    /// `Item.base = "BodyArmour"`, the engine's basic Transmute orb
    /// must successfully roll the Life mod with non-zero probability.
    /// If this assertion fails, the engine isn't seeing our mod set —
    /// every subsequent training run will degenerate to V_path = -1000
    /// because no state ever advances past Normal/empty.
    #[test]
    fn simulate_transmute_actually_rolls_life_mod() {
        use poc2_advisor::simulate;
        let registry = ModRegistry::from_mods(
            vec![ModDefinition {
                id: "LifeProbe".into(),
                name: Some("of Life".into()),
                mod_group: ModGroup(ModGroupId::from("LifeProbeGrp")),
                affix_type: poc2_engine::item::AffixType::Prefix,
                kind: ModKind::Explicit,
                domain: ModDomain::Item,
                tags: smallvec![],
                concept_set: smallvec![ConceptId::from("Life")],
                spawn_weights: smallvec![poc2_engine::mods::SpawnWeight {
                    tag: poc2_engine::ids::TagId::from("body_armour"),
                    weight: 1000,
                }],
                stats: smallvec![],
                // Perfect orbs require `required_level >= 70` (the
                // `MIN_LEVEL_PERFECT_ALL` filter in
                // `crates/engine/src/currency/basic.rs`). The corpus
                // training defaults to Perfect-orb actions, so the
                // synthetic mod must clear that bar.
                required_level: 75,
                tier: None,
                allowed_item_classes: smallvec![ItemClassId::from("BodyArmour")],
                patch_range: PatchRange::ALL,
                flags: ModFlags::empty(),
                text_template: None,
            }],
            vec![],
        );
        let resolver = DefaultCurrencyResolver::new();
        let item = build_initial_item(
            &CorpusGoal {
                id: "probe".into(),
                display_name: "probe".into(),
                item_class: "BodyArmour".into(),
                ilvl: 82,
                budget_div: 50.0,
                target: CorpusTarget::default(),
            },
            BaseTypeId::from("BodyArmour"),
        );
        let action = AdvisorAction::ApplyCurrency {
            currency: CurrencyId::from("PerfectOrbOfTransmutation"),
            omens: vec![],
        };
        // Try 32 different RNG seeds. With 50/50 prefix/suffix slot
        // selection, statistically all 32 should NOT all pick suffix.
        // Some must pick prefix and roll the Life mod.
        let mut life_rolls = 0;
        let mut errors: Vec<String> = Vec::new();
        for seed in 0..32u64 {
            let outcome = simulate(
                &item,
                &action,
                &OmenSet::new(),
                &registry,
                &resolver,
                PatchVersion::PATCH_0_4_0,
                seed,
            );
            if outcome.success {
                if outcome
                    .item
                    .prefixes
                    .iter()
                    .any(|m| m.mod_id.as_str() == "LifeProbe")
                {
                    life_rolls += 1;
                }
            } else if let Some(err) = outcome.error {
                errors.push(err);
            }
        }
        assert!(
            life_rolls > 0,
            "Transmute should roll the Life mod at least once across 32 seeds; \
             error samples: {:?}",
            errors.iter().take(3).collect::<Vec<_>>()
        );
    }

    // ---- load_bundle ---------------------------------------------------

    #[test]
    fn load_bundle_rejects_wrong_schema() {
        // Build a v1-stamped bundle on disk and confirm the loader
        // returns the rebuild-instruction error.
        let tmp = std::env::temp_dir().join("poc2_train_advisor_schema_test.json");
        let mut bundle = Bundle::empty(PatchVersion::PATCH_0_4_0, "test");
        bundle.header.schema_version = 1;
        let serialized = serde_json::to_string(&bundle).unwrap();
        std::fs::write(&tmp, serialized).unwrap();

        let err = load_bundle(&tmp).unwrap_err();
        let msg = format!("{err:#}");
        assert!(
            msg.contains("schema_version=v1"),
            "error should call out the actual schema version: {msg}"
        );
        assert!(
            msg.contains("Rebuild via"),
            "error should include rebuild instructions: {msg}"
        );

        std::fs::remove_file(&tmp).ok();
    }
}
