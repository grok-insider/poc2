//! # poc2-engine
//!
//! Core domain model and crafting engine for Path of Crafting 2.
//!
//! This crate is the substrate — it holds no policy. All higher-level reasoning
//! (strategies, rules, advisor) operates on the types and operations defined here.
//!
//! ## Design principles
//!
//! - **Patch-versioned from line 1**: every entity carries `patch_min` / `patch_max`.
//! - **Deterministic by default**: seeded RNG; no hidden global state.
//! - **Sub-millisecond `apply()`**: hot path for the advisor's beam search.
//! - **No I/O**: this crate never touches disk or network. Pure functions over types.
//!
//! ## Module layout
//!
//! - [`ids`] — newtype identifiers (`ModId`, `BaseTypeId`, `TagId`, ...)
//! - [`patch`] — `PatchVersion` / `PatchRange` versioning
//! - [`error`] — typed errors for invalid operations
//! - [`tag`] — gameplay tag definitions
//! - [`item_class`] — `ItemClass`, `AttributePool`
//! - [`base`] — `BaseType` definitions
//! - [`mods`] — `ModDefinition`, `ModGroup`, `Concept`, hybrid analysis
//! - [`item`] — `Item` runtime state, `ModRoll`, `HiddenDesecratedSlot`, sockets
//! - [`currency`] — every orb, essence, bone, catalyst with `apply()` operations (M2.4-M2.5)
//! - [`omen`] — omen system + synergy hooks (M2.6)
//! - [`engine`] — top-level `apply(currency, item, omens, rng)` (M2.4+)

#![forbid(unsafe_code)]
#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::doc_markdown)] // PoE2 / POE / RNG / UI etc. show up everywhere
#![allow(missing_docs)] // TODO(M2): require doc comments on all public items

pub mod analyzer;
pub mod base;
pub mod currency;
pub mod engine;
pub mod error;
pub mod ids;
pub mod item;
pub mod item_class;
pub mod mods;
pub mod omen;
pub mod patch;
pub mod registry;
pub mod tag;

pub use analyzer::{analyze, BuiltInClassifier, Classifier, CompositeClassifier};
pub use base::{BaseType, InventorySize, ReleaseState};
pub use error::{EngineError, EngineResult};
pub use ids::{
    BaseTypeId, ConceptId, CurrencyId, EssenceId, ItemClassId, ModGroupId, ModId, OmenId, StatId,
    TagId,
};
pub use item::{
    AbyssLord, AffixType, AugmentSlot, BoneSize, BoneSubtype, HiddenDesecratedSlot, Item, ModRoll,
    QualityKind, Rarity, Socket,
};
pub use item_class::{AttributePool, ItemClass};
pub use mods::{
    Concept, ModDefinition, ModDomain, ModFlags, ModGroup, ModKind, ModStat, SpawnWeight,
};
pub use omen::{Omen, OmenEffect, OmenSet};
pub use patch::{PatchRange, PatchVersion};
pub use registry::{ModIndex, ModRegistry};
pub use tag::{Tag, TagCategory};

pub use currency::{ApplyContext, ApplyOutcome, Currency};
pub use engine::{apply_currency, commit_with_preview, preview_currency};

/// Schema version of the engine's serialized types.
///
/// Bumped on any breaking change to `Item`, `ModRoll`, currency definitions, etc.
/// Bundles declare which schema they target; mismatch = refuse to load.
pub const ENGINE_SCHEMA_VERSION: u32 = 1;
