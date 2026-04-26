//! Mod definitions, mod groups, hybrid analysis.
//!
//! Stub for M1; real implementation in M2 with the `mod_analyzer` sub-module
//! that classifies each mod as atomic vs hybrid by computing its concept set
//! from RePoE-fork's stat translations.

use serde::{Deserialize, Serialize};

/// A semantic concept (e.g., `Life`, `EnergyShield`, `FireResistance`).
///
/// Atomic mods produce a single concept; hybrid mods produce multiple.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Concept(pub String);

/// Mod group key — at most one mod per group can occupy an item simultaneously.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ModGroup(pub String);
