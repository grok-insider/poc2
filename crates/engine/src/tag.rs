//! Gameplay tags.
//!
//! Tags are string identifiers used throughout PoE2 to classify mods and base
//! items. The mod-pool eligibility system is tag-based: a mod's `spawn_weights`
//! map tags to weights, and the actual eligible tag set on an item determines
//! which mods can roll.
//!
//! Tags fall into several rough categories (we don't enforce these in the type
//! system — RePoE-fork uses flat strings):
//!
//! | Category | Examples |
//! |----------|----------|
//! | Item-class | `boots`, `helmet`, `body_armour`, `ring`, `amulet`, `quiver` |
//! | Attribute pool | `int_armour`, `dex_armour`, `str_armour`, `str_dex_armour`, ... |
//! | Damage | `damage`, `physical`, `elemental`, `fire`, `cold`, `lightning`, `chaos` |
//! | Skill | `attack`, `caster`, `minion` |
//! | Resource | `life`, `mana`, `energy_shield` |
//! | Defence | `defences`, `resistance` |
//! | Modifier kind | `essence_only`, `corrupted_only`, `desecrated`, `fractured` |
//!
//! ## Tag conditioning
//!
//! Several crafting omens and catalysts condition outcomes on tags:
//! - **Catalysing Exaltation** (omen): consumes catalyst quality to bias the
//!   next Exalt toward mods sharing the catalyst's tag.
//! - **Catalysts** (orbs): tag-targeted quality on rings/amulets.
//! - **Homogenising Exaltation/Coronation** (omens, *disabled in 0.4*):
//!   would add a mod sharing a tag with an existing mod.

use serde::{Deserialize, Serialize};

use crate::ids::TagId;

/// A gameplay tag with a stable identifier.
///
/// In the data bundle, tags carry a small amount of metadata — a category
/// and a human-readable display name. For storage we use a `Tag` wrapper
/// over `TagId` to keep the type signature explicit at API boundaries.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Tag {
    pub id: TagId,
    pub category: TagCategory,
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TagCategory {
    /// Top-level item type: `boots`, `body_armour`, `ring`, etc.
    ItemClass,
    /// Attribute-pool eligibility: `int_armour`, `str_dex_armour`, etc.
    AttributePool,
    /// Damage characteristic: `physical`, `fire`, `cold`, `chaos`, ...
    Damage,
    /// Skill / build characteristic: `caster`, `attack`, `minion`, `speed`, ...
    Skill,
    /// Resource: `life`, `mana`, `energy_shield`.
    Resource,
    /// Defence: `defences`, `resistance`, `armour`, `evasion`.
    Defence,
    /// Mod-kind classification: `essence_only`, `desecrated`, `fractured`, etc.
    ModKind,
    /// Catch-all for tags we have not yet classified.
    Other,
}
