//! poc2-pipeline — data bundle builder library.
//!
//! Pulls source data from upstream feeds (RePoE-fork, Craft of Exile,
//! poe2db.tw, GGG trade-stat API) and produces a validated [`poc2_data::Bundle`].
//!
//! Run via the bin (`poc2-pipeline build`) or use these modules directly.

#![forbid(unsafe_code)]
#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::doc_lazy_continuation)]
#![allow(clippy::too_many_lines)]

pub mod build;
pub mod error;
pub mod http;
pub mod normalize;
pub mod sources;

pub use build::{build_bundle, BuildOptions};
pub use error::{PipelineError, PipelineResult};
