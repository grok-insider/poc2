//! Item classes and attribute pools.
//!
//! An [`ItemClass`] is a top-level item type (`Boots`, `BodyArmour`,
//! `OneHandSword`, ...). We don't enumerate them — RePoE-fork has ~50 of them
//! and they evolve per patch (e.g., `Talisman` was added in 0.4). Instead we
//! load them from the bundle as data.
//!
//! An [`AttributePool`] is the stat scaling axis a base item draws from
//! (Strength, Dexterity, Intelligence, or hybrid combinations). It determines
//! which defensive mod pool the base rolls against (Armour for Str, Evasion
//! for Dex, Energy Shield for Int).

use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

use crate::ids::{ItemClassId, TagId};
use crate::patch::PatchRange;

/// Top-level item class definition (data, loaded from the bundle).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemClass {
    pub id: ItemClassId,
    /// Human-readable display name (e.g., `"Body Armour"`, `"Two Hand Sword"`).
    pub name: String,
    /// Maximum number of intrinsic implicit modifiers this class can carry.
    pub max_implicits: u8,
    /// Maximum prefix slots for a Rare item of this class. Typically 3.
    pub max_prefixes: u8,
    /// Maximum suffix slots for a Rare item of this class. Typically 3.
    pub max_suffixes: u8,
    /// Maximum gear sockets (0 for jewelry/quivers/foci, 1-2 for armour & weapons).
    pub max_sockets: u8,
    /// Tags inherited by every base in this class (e.g., `boots`, `armour_dex_int`).
    pub class_tags: SmallVec<[TagId; 4]>,
    /// Patch range over which this class definition is valid.
    pub patch_range: PatchRange,
}

/// Attribute pool a base item belongs to. Determines defensive mod pool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttributePool {
    /// Pure Strength — Armour pool.
    Str,
    /// Pure Dexterity — Evasion pool.
    Dex,
    /// Pure Intelligence — Energy Shield pool.
    Int,
    /// Strength + Dexterity hybrid — Armour + Evasion.
    StrDex,
    /// Strength + Intelligence hybrid — Armour + Energy Shield.
    StrInt,
    /// Dexterity + Intelligence hybrid — Evasion + Energy Shield.
    DexInt,
    /// Triple-attribute (rare; some uniques).
    StrDexInt,
    /// No attribute requirement (e.g., quivers, foci, certain weapons).
    None,
}

impl AttributePool {
    /// Convenience: does this pool include Strength?
    pub const fn has_str(self) -> bool {
        matches!(
            self,
            Self::Str | Self::StrDex | Self::StrInt | Self::StrDexInt
        )
    }
    /// Convenience: does this pool include Dexterity?
    pub const fn has_dex(self) -> bool {
        matches!(
            self,
            Self::Dex | Self::StrDex | Self::DexInt | Self::StrDexInt
        )
    }
    /// Convenience: does this pool include Intelligence?
    pub const fn has_int(self) -> bool {
        matches!(
            self,
            Self::Int | Self::StrInt | Self::DexInt | Self::StrDexInt
        )
    }
}
