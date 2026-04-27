//! # poc2-data
//!
//! Bundle loader and schema types for the patch-versioned data feed.
//!
//! Bundles are produced by the `pipeline` crate (separate process / repo) and
//! consumed by the engine. A bundle is a single JSON or compressed JSON
//! document containing every piece of data the engine + advisor need: mods,
//! base items, currencies, omens, essences, bones, catalysts, weights,
//! stat translations, the concept map, and the synergy graph.
//!
//! Every entity carries a `PatchRange`. Loaders filter by the configured
//! patch version; the engine sees only entities valid in that patch.
//!
//! ## Module layout
//!
//! - [`bundle`] — the top-level [`Bundle`] container and its sub-sections
//! - [`sources`] — metadata identifying which upstream revisions produced the bundle
//! - [`weights`] — numerical mod weights with confidence flags
//! - [`concepts`] — stat-id → semantic-concept mapping for hybrid analysis
//! - [`synergy`] — synergy graph: which (currency, omen) pairs are legal + their effects
//! - [`error`] — [`DataError`] for bundle load / validation failures
//! - [`io`] — read / write helpers (JSON and gzipped JSON)
//! - [`validation`] — bundle invariants checker (schema versions, patch coverage, etc.)

#![forbid(unsafe_code)]
#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::doc_markdown)]

pub mod bundle;
pub mod concepts;
pub mod error;
pub mod io;
pub mod sources;
pub mod synergy;
pub mod validation;
pub mod weights;

pub use bundle::{Bundle, BundleHeader, BundleSection};
pub use concepts::{ConceptDefinition, ConceptMap, ConceptMapEntry};
pub use error::{DataError, DataResult};
pub use sources::{SourceRevision, SourceRevisions};
pub use synergy::{SynergyEdge, SynergyOverride};
pub use weights::{Confidence, WeightObservation};

/// Bundle schema version this loader understands.
///
/// Bumped on any breaking change to the bundle format. Mismatch with a bundle's
/// declared version is a hard error; the loader refuses to proceed.
///
/// ## History
///
/// - **v1** — initial release (M1, v1.0). Phase E (Desecrated + Vaal
///   implicit fixture ingestion, `docs/80-crafter-helper-v2-plan.md` §5)
///   was *additive* to the existing schema: it adds entries to
///   `bundle.mods` whose `kind` and `flags` fields already round-trip in
///   v1, so the loader keeps reading older bundles without rebuilds.
/// - **v2** — v3 (M14.7) introduces runtime consumers of `bundle.weights`
///   (M14.1's `ModRegistry::weight_for`) and `bundle.base_items` (M14.2's
///   `BaseRegistry`). v1 bundles missing or stale on either field would
///   silently degrade the trained-policy advisor's accuracy, so the
///   loader now hard-rejects v1 bundles. Bundles must be rebuilt via
///   `cargo run -p poc2-pipeline -- build` after upgrading. The Tauri
///   loader detects v1 on disk and surfaces a structured "rebuild
///   bundle" event to the desktop UI; user state under
///   `~/.config/poc2/state.toml` and `~/.config/poc2/recipes/` is
///   wiped on the first successful v2 launch (cache/ is preserved).
pub const BUNDLE_SCHEMA_VERSION: u32 = 2;
