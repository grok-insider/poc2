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
pub mod schema;
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
pub const BUNDLE_SCHEMA_VERSION: u32 = 1;
