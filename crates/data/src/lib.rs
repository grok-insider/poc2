//! # poc2-data
//!
//! Bundle loader and schema types for the patch-versioned data feed.
//!
//! Bundles are produced by the `pipeline` crate (separate process / repo) and
//! consumed here. A bundle is a single JSON or TOML document containing every
//! piece of data the engine + advisor need: mods, base items, currencies,
//! omens, essences, bones, catalysts, weights, stat translations, and the
//! synergy graph.
//!
//! Every entity carries `patch_min` / `patch_max`. Loaders filter by the
//! currently configured `PatchVersion`.
//!
//! Stub for M1; real implementation in M2 (Data Pipeline).

#![forbid(unsafe_code)]
#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::doc_markdown)]

pub mod bundle;
pub mod schema;

pub use bundle::Bundle;

/// Bundle schema version this loader understands.
///
/// Bumped on any breaking change to the bundle format. Mismatch with a bundle's
/// declared version is a hard error; the loader refuses to proceed.
pub const BUNDLE_SCHEMA_VERSION: u32 = 1;
