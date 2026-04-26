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
//! - [`item`] — `Item`, `ModRoll`, `BaseType`, rarity, slots, fractured/sanctified flags
//! - [`mods`] — `ModDefinition`, mod groups, tiers, tags, hybrid analysis
//! - [`currency`] — every orb, essence, bone, catalyst with `apply()` operations
//! - [`omen`] — omen system + synergy hooks into currency operations
//! - [`engine`] — top-level `apply(currency, item, omens, rng) -> Result<Item, Error>`
//! - [`patch`] — `PatchVersion` with `patch_min`/`patch_max` semantics
//! - [`error`] — typed errors for invalid operations
//!
//! All modules are stubs at M1; populated in M2 (Engine Core).

#![forbid(unsafe_code)]
#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::doc_markdown)] // PoE2 / POE / RNG / UI etc. show up everywhere
#![allow(missing_docs)] // TODO(M2): require doc comments on all public items

pub mod currency;
pub mod engine;
pub mod error;
pub mod item;
pub mod mods;
pub mod omen;
pub mod patch;

pub use error::{EngineError, EngineResult};
pub use patch::PatchVersion;

/// Schema version of the engine's serialized types.
///
/// Bumped on any breaking change to `Item`, `ModRoll`, currency definitions, etc.
/// Bundles declare which schema they target; mismatch = refuse to load.
pub const ENGINE_SCHEMA_VERSION: u32 = 1;
