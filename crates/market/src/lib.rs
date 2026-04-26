//! # poc2-market
//!
//! Currency valuator, live price feeds, and meta-build awareness.
//!
//! ## Modules
//!
//! - [`valuator`] — `DivEquiv(min, expected, max)` and cross-currency conversion graph.
//!   Conservative fallback ranges (per planning):
//!   `1 div = 50-180 ex`, `1 div = 3-30 chaos`, `1 mirror = 1500-6000 div`.
//!   Live data from poe2scout / poe.ninja overrides within 30s of online connection.
//! - [`prices`] — pollers for poe2scout, poe.ninja PoE2.
//! - [`meta`] — meta-build aggregator (poe.ninja PoE2 builds page) + off-meta finder.
//!
//! Stub for M1; real implementation in M5/M6.

#![forbid(unsafe_code)]
#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::doc_markdown)]

pub mod meta;
pub mod prices;
pub mod valuator;
