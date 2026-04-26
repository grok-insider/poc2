//! # poc2-rules
//!
//! Forward-chained production rule engine.
//!
//! ~120 heuristic rules from `/docs/34-heuristics-rulebook.md`, each of the form:
//!
//! ```text
//! when:        ItemPredicate (state matcher)
//! then:        Vec<Suggestion> (recommended actions)
//! explanation: human-readable rationale
//! source:      citation (streamer / guide / VOD)
//! confidence:  Verified | Community | Experimental
//! ```
//!
//! The engine emits all rules whose `when` matches the current state. The
//! advisor then ranks the union of (rule-emitted suggestions + strategy-emitted
//! candidates) by EV, cost, and risk preference.
//!
//! Stub for M1; real implementation in M3.

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
