//! # poc2-rules
//!
//! Forward-chained heuristic rule engine.
//!
//! A [`Rule`] pairs an [`ItemPredicate`](poc2_strategies::ItemPredicate)
//! `when` clause with a list of [`Suggestion`]s describing the actions a
//! human expert would propose in that state. The advisor consumes
//! suggestions alongside strategy candidates to produce its top-N
//! recommendations.
//!
//! Rules are deliberately simple: they fire when their `when` matches
//! and never recurse. Multi-step plans live in strategies; rules
//! capture single-step heuristics ("if you just got a 4-mod with one
//! T1, fracture next").
//!
//! M3.d ships with ~15 hand-coded seed rules covering the most useful
//! cases from /docs/34-heuristics-rulebook.md. The full ~120-rule
//! catalogue lands as data-driven TOML rules in v1.1+.

#![forbid(unsafe_code)]
#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::doc_markdown)]

pub mod engine;
pub mod loader;
pub mod rule;
pub mod seed;

pub use engine::{evaluate, evaluate_with_ctx, EngineResult};
pub use loader::{load_rule_str, RuleError};
pub use rule::{Category, Confidence, Rule, RuleId, RuleSet, Suggestion, SuggestionAction};
pub use seed::seed_rules;
