//! Mod definitions, mod groups, semantic concepts, hybrid analysis.
//!
//! ## Mod definition shape
//!
//! Each [`ModDefinition`] is a single tier of a single mod-group. RePoE-fork's
//! `mods.json` has one entry per (group × tier) combination; we mirror that
//! 1-to-1. The "tier ladder" of a mod-group is the set of `ModDefinition`s
//! sharing the same [`ModGroupId`], ordered by [`ModDefinition::required_level`].
//!
//! ## Mod-group exclusivity
//!
//! At most one mod from a given [`ModGroupId`] can occupy an item at the same
//! time. This is the "you can only have one Life mod" rule. Hybrid mods
//! typically have their own group (e.g., `BaseLocalDefencesAndLife`) distinct
//! from the singleton groups (`IncreasedLife`, `IncreasedEnergyShield`), so a
//! hybrid `+ES + +Life` mod does NOT lock out a singleton `+Life` mod from
//! rolling — they are different groups.
//!
//! ## Hybrid mods
//!
//! A *hybrid* mod is one whose [`ModDefinition::stats`] array contains
//! multiple distinct **concepts** — e.g., `+X% Energy Shield AND +Y maximum
//! Life` is one mod (one affix slot) producing both an `EnergyShield` stat
//! and a `Life` stat. The [`mod_analyzer`] (M2.7) computes each mod's
//! [`ConceptId`] set; targets matching `EnergyShield` then accept this hybrid.
//!
//! Note that RePoE-fork mod entries always have multiple `stats` for added-
//! damage mods (separate `min` and `max` of a damage range). These are NOT
//! hybrids — they're a single concept (`AddedFireDamage`) split across two
//! stats. The analyzer disambiguates via the [`Concept`] taxonomy.

use bitflags::bitflags;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

use crate::ids::{ConceptId, ItemClassId, ModGroupId, ModId, StatId, TagId};
use crate::item::AffixType;
use crate::patch::PatchRange;

// -------------------------------------------------------------------------
// Concept (semantic grouping over raw stat-ids)
// -------------------------------------------------------------------------

/// A semantic concept — atomic unit of "what stat does this affect".
///
/// Concepts are *our* taxonomy, not GGG's. We map raw `stat_id`s
/// (e.g. `local_energy_shield_+%`, `base_maximum_energy_shield`) to a single
/// `Concept::EnergyShield`. The mapping lives in the data bundle's
/// `concept_map` and is computed at pipeline build time.
///
/// A mod with `|concept_set| > 1` is a hybrid in the sense the user means
/// (single affix slot, multiple distinct stat outputs).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Concept {
    pub id: ConceptId,
    pub display_name: String,
}

// -------------------------------------------------------------------------
// Mod-group key
// -------------------------------------------------------------------------

/// Mod-group key — at most one mod per group can occupy an item at the same
/// time. Hybrid mods live in their own groups distinct from singleton mods.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ModGroup(pub ModGroupId);

// -------------------------------------------------------------------------
// Mod kind / domain
// -------------------------------------------------------------------------

/// What kind of mod this is, orthogonal to where it sits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModKind {
    /// Standard rolled mod (transmute / regal / exalt / chaos / essence outputs).
    Explicit,
    /// Intrinsic implicit (from base item).
    Implicit,
    /// Enchantment from runes / soul cores / Vaal corruption / certain omens.
    Enchantment,
    /// Desecrated mod added by an Abyssal Bone, optionally still hidden.
    Desecrated,
    /// Corrupted mod (post-Vaal, e.g., +1 socket).
    Corrupted,
    /// Crafted mod — guaranteed mods added by Verisium Alloys, Liquid /
    /// Ancient Emotions, and Genesis-Tree crafted-mod nodes (0.5 systems).
    /// Per the 0.5 patch notes: "All crafted modifiers are now guaranteed,
    /// but items can only have 1 crafted modifier at a time."
    Crafted,
}

/// Rough taxonomy of where a mod can roll.
///
/// RePoE-fork's `domain` field — most relevant values for crafting are
/// `Item` (gear), `Map`, `Jewel`, `Atlas`, `AbyssJewel`. Mods on non-`Item`
/// domains exist but are out of scope for the gear crafting advisor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModDomain {
    Item,
    Map,
    Jewel,
    AbyssJewel,
    Atlas,
    Misc,
}

// -------------------------------------------------------------------------
// ModFlags
// -------------------------------------------------------------------------

bitflags! {
    /// Flags on a [`ModDefinition`] modulating its behavior in `apply()`.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct ModFlags: u32 {
        /// Local mod — modifies the item's own base stats (shows blue on the
        /// item header). Local mods do not stack the way globals do, and they
        /// cannot apply when the item is in a different slot.
        const LOCAL = 1 << 0;
        /// Only obtainable via essences (won't roll from regular currencies).
        const ESSENCE_ONLY = 1 << 1;
        /// Only obtainable via desecration bones.
        const DESECRATED_ONLY = 1 << 2;
        /// Mod's `concept_set` has cardinality > 1 (computed by mod analyzer).
        const HYBRID = 1 << 3;
        /// Only obtainable from Vaal corruption (typically enchantments).
        const CORRUPTED_ONLY = 1 << 4;
        /// Rolls only via Altered Collarbone breach desecration on rare
        /// jewellery (poe2db "Otherworldly" sections, 0.5). Excluded from
        /// every other pool incl. regular bone reveals.
        const OTHERWORLDLY = 1 << 5;
    }
}

// Manual serde impls because bitflags's serde feature emits a string array,
// which is fine; we're explicit so future format changes are obvious.
impl serde::Serialize for ModFlags {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.bits().serialize(s)
    }
}
impl<'de> serde::Deserialize<'de> for ModFlags {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let bits = u32::deserialize(d)?;
        Self::from_bits(bits).ok_or_else(|| serde::de::Error::custom("invalid ModFlags bits"))
    }
}

// -------------------------------------------------------------------------
// Stat / SpawnWeight
// -------------------------------------------------------------------------

/// One numerical stat output of a mod.
///
/// A mod with multiple `ModStat`s either (a) represents a single concept
/// across two stats (e.g. min/max of an added-damage range) or (b) is a
/// genuine hybrid (e.g. +ES and +Life). The mod analyzer distinguishes via
/// the concept map.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModStat {
    pub stat_id: StatId,
    pub min: f64,
    pub max: f64,
}

impl ModStat {
    pub fn range_size(&self) -> f64 {
        self.max - self.min
    }

    /// Linear interpolation: `roll(0.0) == min`, `roll(1.0) == max`.
    pub fn roll(&self, t: f64) -> f64 {
        self.min + t.clamp(0.0, 1.0) * self.range_size()
    }
}

/// Tag-eligibility weight from RePoE-fork. Mostly placeholders (1 = eligible
/// on bases carrying this tag, 0 = excluded). Real numerical weights live in
/// [`crate::weights`] (M2.3) sourced from Craft of Exile / poe2db.tw.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpawnWeight {
    pub tag: TagId,
    pub weight: u32,
}

impl SpawnWeight {
    pub const ELIGIBLE: u32 = 1;
    pub const EXCLUDED: u32 = 0;
}

// -------------------------------------------------------------------------
// ModDefinition
// -------------------------------------------------------------------------

/// Full definition of a single mod (one (group × tier) combination).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModDefinition {
    pub id: ModId,
    /// Human-readable affix name (e.g. `"Monk's"`). May be `None` for some
    /// implicit / essence-only / desecrated mods.
    pub name: Option<String>,
    /// Mod-group key — exclusivity with other mods sharing this group.
    pub mod_group: ModGroup,
    pub affix_type: AffixType,
    pub kind: ModKind,
    pub domain: ModDomain,
    /// Tags that influence pool eligibility and tag-conditioning omens
    /// (Catalysing Exaltation, Homogenising — disabled in 0.4).
    pub tags: SmallVec<[TagId; 8]>,
    /// Concept set for target matching. Cardinality > 1 ⇒ hybrid (also
    /// reflected in `flags.contains(ModFlags::HYBRID)`).
    pub concept_set: SmallVec<[ConceptId; 4]>,
    /// Tag → eligibility (placeholder weights). Numerical weights are kept
    /// separately in the bundle's `weights` table.
    pub spawn_weights: SmallVec<[SpawnWeight; 6]>,
    /// One stat output per element. Multiple may be the same concept (added
    /// damage min/max pair) or different (hybrid mods).
    pub stats: SmallVec<[ModStat; 4]>,
    /// Minimum item level for this mod to be eligible.
    pub required_level: u32,
    /// Explicit tier ordinal within the mod-group, where `1` is the
    /// highest/strongest tier (largest stat values, highest `required_level`)
    /// and larger numbers are weaker tiers. `None` when the source data does
    /// not carry an explicit tier; in that case the engine derives tier
    /// ordering from `required_level` (higher `required_level` ⇒ stronger
    /// tier) via [`ModRegistry`]. Defaulted for backward-compatible
    /// deserialization of pre-tier bundles.
    #[serde(default)]
    pub tier: Option<u16>,
    /// Item classes this mod can roll on. Computed by the pipeline by
    /// intersecting `spawn_weights` tags with item-class tag membership.
    pub allowed_item_classes: SmallVec<[ItemClassId; 8]>,
    pub patch_range: PatchRange,
    pub flags: ModFlags,
    /// Display text template (e.g. `"(6-13)% increased Energy Shield\n+(7-10) to maximum Life"`).
    pub text_template: Option<String>,
}

impl ModDefinition {
    /// Is this a hybrid mod (multiple distinct concepts in one slot)?
    pub fn is_hybrid(&self) -> bool {
        self.flags.contains(ModFlags::HYBRID)
    }

    /// Is this mod eligible to roll on the given item-class?
    pub fn allowed_on(&self, class: &ItemClassId) -> bool {
        self.allowed_item_classes.iter().any(|c| c == class)
    }

    /// Resolve this mod's spawn weight against a base's tag list using the
    /// PoE2 **leftmost-tag-wins** rule.
    ///
    /// The mod's `spawn_weights` is an *ordered* list of `(tag, weight)`.
    /// Scanning it in order, the first entry whose tag is present in
    /// `base_tags` decides the weight: a positive weight ⇒ eligible with that
    /// weight; a zero weight ⇒ explicitly excluded (return `Some(0)`). If no
    /// `spawn_weights` entry matches any base tag, the mod cannot roll on
    /// this base ⇒ `None`.
    ///
    /// Note: "leftmost wins" here means the **first** matching entry in the
    /// mod's `spawn_weights` ordering (which mirrors the game's
    /// most-significant-tag-first convention as published by RePoE-fork),
    /// not a scan of the base's tags.
    pub fn spawn_weight_for_tags(&self, base_tags: &[TagId]) -> Option<u32> {
        for sw in &self.spawn_weights {
            if base_tags.contains(&sw.tag) {
                return Some(sw.weight);
            }
        }
        None
    }

    /// Effective tier-strength key for ordering within a mod-group.
    ///
    /// Larger value ⇒ stronger tier (higher stat values). When an explicit
    /// `tier` ordinal is present (1 = strongest), we invert it so the
    /// strongest tier maps to the largest key. Otherwise we fall back to
    /// `required_level`, since higher-required-level tiers are stronger in
    /// PoE2. This key is what the inclusive-tier weighting uses to decide
    /// which peers count as "the same or higher tier".
    pub fn tier_strength_key(&self) -> u32 {
        match self.tier {
            // Invert the 1-based ordinal: tier 1 (strongest) ⇒ large key.
            // `t` is already a `u16`, so the subtraction cannot overflow for
            // any in-range ordinal (tier 0 is unused; 1 = strongest).
            Some(t) => u32::from(u16::MAX - t),
            None => self.required_level,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modstat_roll_clamps() {
        let s = ModStat {
            stat_id: "x".into(),
            min: 0.0,
            max: 10.0,
        };
        assert!((s.roll(0.0) - 0.0).abs() < 1e-9);
        assert!((s.roll(1.0) - 10.0).abs() < 1e-9);
        assert!((s.roll(0.5) - 5.0).abs() < 1e-9);
        // Clamping
        assert!((s.roll(-1.0) - 0.0).abs() < 1e-9);
        assert!((s.roll(2.0) - 10.0).abs() < 1e-9);
    }

    #[test]
    fn modflags_serde() {
        let f = ModFlags::HYBRID | ModFlags::LOCAL;
        let j = serde_json::to_string(&f).unwrap();
        let back: ModFlags = serde_json::from_str(&j).unwrap();
        assert_eq!(back, f);
    }
}
