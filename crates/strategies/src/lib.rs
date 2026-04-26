//! # poc2-strategies
//!
//! Codified strategy library — multi-step crafting recipes loaded from
//! TOML or JSON. Strategies are **data**, not code: they ship in the data
//! bundle and can be authored, swapped, and patched without rebuilding the
//! binary.
//!
//! The seed catalogue is documented in
//! [`/docs/33-strategy-library.md`](../../../docs/33-strategy-library.md);
//! the canonical user-authored "Triple T1 Energy Shield Body Armour"
//! fixture lives in `assets/strategies/3xt1-es-body-armour.toml`.
//!
//! ## Module layout
//!
//! - [`dsl`] — strategy types (Strategy, Step, Action, Branch, Predicate,
//!   Target, RecoveryHint)
//! - [`loader`] — TOML / JSON file loader, plus directory walker
//! - [`registry`] — runtime registry of loaded strategies, queryable by
//!   item class + goal
//!
//! In v1.1+ the plugin system (Wasm Component Model) will allow third-party
//! strategy authors to ship strategies as plugins. v1.0 supports only TOML
//! (or in-process Strategy values).

#![forbid(unsafe_code)]
#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::doc_markdown)]

pub mod dsl;
pub mod executor;
pub mod loader;
pub mod predicate;
pub mod registry;

pub use dsl::{
    Action, Branch, CmpOp, Confidence, FloatValuePredicate, ItemPredicate, RecoveryHint, Source,
    Step, StepId, Strategy, StrategyId, Target, TargetSpec, ValuePredicate,
};
pub use executor::{
    advance, dry_run, enter, next_recommendation, DryRunStep, EnterError, ExecutionResult,
    ExecutionState, TerminalKind,
};
pub use loader::{load_strategy_str, load_strategy_toml, StrategyError, StrategyResult};
pub use predicate::{eval, eval_all, eval_any, is_hybrid_mod, PredicateContext, StashView};
pub use registry::StrategyRegistry;
