//! Bundle type — top-level container produced by the `pipeline` and consumed by the engine.
//!
//! Stub for M1. Real schema designed in M2 alongside `/docs/21-bundle-schema.json`.

use poc2_engine::PatchVersion;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Bundle {
    pub schema_version: u32,
    pub game_patch: PatchVersion,
    /// ISO 8601 UTC timestamp.
    pub built_at: String,
    /// Build-pipeline revision (e.g., `pipeline@abcdef0`).
    pub built_by: String,
    // TODO(M2): mods, bases, currencies, omens, essences, bones, catalysts,
    //           weights, stat_translations, synergy_overrides.
}
