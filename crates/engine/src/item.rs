//! Item state model.
//!
//! Stub for M1; real implementation in M2.
//!
//! Notable invariants the full implementation must enforce (per planning docs):
//! - Hidden desecrated mods occupy a slot for fracture-orb counting but are
//!   ineligible as fracture targets.
//! - Fractured mods are immutable to Divine Orb (cannot reroll their values).
//! - Mod-group exclusivity: at most one mod from each `ModGroup` per item.
//! - Hybrid mods occupy ONE affix slot but produce multiple `Concept` outputs.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Rarity {
    Normal,
    Magic,
    Rare,
    Unique,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AffixType {
    Prefix,
    Suffix,
    Implicit,
    Enchantment,
    Desecrated, // currently sits on a suffix or prefix slot per the bone applied
}

/// Stub item type; full struct in M2.
///
/// TODO(M2): full state — base, ilvl, rarity, mods, sockets, fractures, etc.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Item {}
