//! Offline training infrastructure for the trained-policy advisor (M16).
//!
//! ## Module map
//!
//! - [`analytic_model`] — builds the per-action transition model
//!   `P(s' | s, a)` **exactly** from the engine's pool-weight enumeration
//!   (the production default; Monte Carlo fallback for exotic actions).
//! - [`model_learner`] — the original Monte Carlo learner (M16.2), kept as
//!   the cross-validation reference and the `--model mc` path.
//! - [`value_iteration`] — solves the Bellman equation over the learned
//!   transition model to produce a Q-table (M16.3).
//!
//! Imitation seeding (M16.5), the hybrid planner integration (M16.4),
//! and the training-corpus binary (M16.6) live in subsequent tiers.
//!
//! ## Reference
//!
//! Algorithmic shape follows Britz, *Solving the Path of Exile crafting
//! MDP* (<https://dennybritz.com/posts/poe-crafting/>) adapted for PoE2's
//! state shape and the v3 plan's afterstate-aliasing policy
//! (`docs/81-engine-training-and-rule-encoding-plan.md` §6).

pub mod analytic_model;
pub mod artefact;
pub(crate) mod families;
pub mod hybrid;
pub mod imitation;
pub mod metrics;
pub mod model_learner;
pub mod value_iteration;

pub use analytic_model::{
    learn_transition_model_analytic, AnalyticConfig, EXACT_DISTRIBUTION_SAMPLES,
};
pub use artefact::{
    load_artefact_file, load_artefacts_str, load_cache_from_dir, ArtefactLoadOutcome,
    TrainedModelArtefact, TrainingArtefactMetrics,
};
pub use hybrid::{
    goal_hash, score_with_trained_policy, sim_to_real_gap, trained_model_from, QEntry, RewardKind,
    SimToRealVerdict, TrainedModel, TrainedModelCache, TRAINED_ARTEFACT_SCHEMA_VERSION,
};
pub use imitation::{lift_strategy_action, seed_from_strategies, ImitationConfig};
pub use metrics::{argmax_actions, loop_iteration_estimate, top_action_agreement, TrainingMetrics};
pub use model_learner::{
    learn_transition_model, CraftingTask, LearnConfig, StateActionAlias, TableModel,
    TableModelBuilder,
};
pub use value_iteration::{value_iteration, ValueIterationConfig, ValueIterationResult};
