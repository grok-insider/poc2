//! Base items.
//!
//! A [`BaseType`] is a specific item template (`Wanderer Shoes`, `Smithed
//! Greaves`, `Heavy Belt`, ...). Bases are the starting point of every craft
//! and determine:
//! - Which item class the rolled item belongs to
//! - Which attribute pool drives its defensive mods
//! - The `drop_level` floor (cannot be dropped below this ilvl)
//! - Intrinsic implicit modifiers
//! - Tags that participate in mod-pool eligibility
//!
//! The full slot layout comes from the parent [`ItemClass`](crate::item_class::ItemClass);
//! `BaseType` mostly carries the override-per-base bits (different drop_level,
//! different implicit pool, different tags).

use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

use crate::ids::{BaseTypeId, ItemClassId, ModId, TagId};
use crate::item_class::AttributePool;
use crate::patch::PatchRange;

/// Definition of a base item (data, loaded from the bundle).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaseType {
    pub id: BaseTypeId,
    /// Human-readable name (`"Wanderer Shoes"`, `"Heavy Belt"`).
    pub name: String,
    pub item_class: ItemClassId,
    pub attribute_pool: AttributePool,
    /// Minimum item level this base can drop at.
    pub drop_level: u32,
    /// Tags carried by this base (item-class tags + base-specific tags).
    pub tags: SmallVec<[TagId; 16]>,
    /// Intrinsic implicit modifiers (rolled at drop, not changeable by most currencies).
    pub implicits: SmallVec<[ModId; 2]>,
    /// Inventory dimensions.
    pub inventory: InventorySize,
    /// Whether the base is currently obtainable in the active patch.
    /// `released` = available; `unreleased` / `legacy` = not in current drop pool.
    pub release_state: ReleaseState,
    pub patch_range: PatchRange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct InventorySize {
    pub width: u8,
    pub height: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReleaseState {
    /// In the live drop pool.
    Released,
    /// Defined in data but not yet enabled (test/unreleased content).
    Unreleased,
    /// Removed from drop pool but existing items still function.
    Legacy,
    /// Unique base type (different rules — see uniques table).
    Unique,
}
