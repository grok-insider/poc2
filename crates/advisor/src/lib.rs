//! # poc2-advisor
//!
//! Beam-search optimal-path advisor.
//!
//! Given:
//! - Current item state
//! - Target (goal mods, item)
//! - Budget (in DivEquiv)
//! - Stash (currencies/omens the user owns)
//! - Patch version
//! - Risk preference (slider, cautious ↔ greedy)
//!
//! Produces:
//! - Top-N recommended next actions, ranked by utility
//! - Each recommendation cites its source rule/strategy + EV math
//! - Recovery branches surfaced when the last action's outcome was a failure
//!
//! ## Algorithm
//!
//! 1. **Generate candidates** from rules (`poc2-rules`) and strategies (`poc2-strategies`)
//!    matching the current state.
//! 2. **Beam-search** over candidate sequences (configurable width / depth).
//! 3. **Score nodes** via `poc2-probability` Monte Carlo against the engine
//!    (`poc2-engine`) using costs from `poc2-market`.
//! 4. **Rank** by user utility (EV − cost, risk-adjusted).
//! 5. **Stream results** back to UI as the search deepens (Tokio task; cancellable).
//!
//! Stub for M1; real implementation in M4.

#![forbid(unsafe_code)]
#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::doc_markdown)]

pub mod planner;
pub mod recovery;
pub mod scorer;
