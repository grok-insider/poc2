//! # poc2-probability
//!
//! Probability and expected-value math for the advisor.
//!
//! - **Monte Carlo**: simulate `N` trials of a strategy or single step,
//!   producing distributions over outcomes (success/cost/duration).
//! - **Geometric distribution**: closed-form expected attempts for
//!   chaos-spam and annul-aug-spam strategies.
//! - **Confidence intervals**: surface uncertainty when weights are
//!   community-sourced rather than canonical.
//!
//! Stub for M1; real implementation in M5.

#![forbid(unsafe_code)]
#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::doc_markdown)]

pub mod ev;
pub mod geometric;
pub mod montecarlo;
