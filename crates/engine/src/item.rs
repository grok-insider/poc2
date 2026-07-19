//! Item state model.
//!
//! [`Item`] is the runtime state object the engine operates on. Currencies
//! consume an `Item` and produce a new one (via `apply()`). It is intentionally
//! cheap to clone — the advisor's beam search clones items thousands of times
//! during a re-plan.
//!
//! ## Notable invariants the engine enforces
//!
//! 1. **Hidden desecrated mods** occupy a slot for the Fracturing Orb's
//!    "≥4 explicit mods" requirement but are NOT eligible as the fracture
//!    target. See [`HiddenDesecratedSlot`].
//! 2. **Fractured mods are immutable to Divine Orb** — they keep their
//!    rolled values forever once locked. See [`ModRoll::is_fractured`].
//! 3. **Mod-group exclusivity** — at most one mod per [`crate::mods::ModGroup`]
//!    on the item simultaneously. Hybrid mods sit in their own groups, so
//!    they don't lock out singleton siblings.
//! 4. **Corrupted / Sanctified items** reject most further crafting. Once
//!    corrupted (Vaal Orb), an item allows only specific operations
//!    (Architect's Orb double-corrupt, certain anoint variants, ...).
//!    Once sanctified (Divine + Omen of Sanctification), most operations
//!    are forbidden.
//! 5. **Hybrid mods** occupy ONE affix slot but produce multiple
//!    [`ConceptId`](crate::ids::ConceptId) outputs. Target matching is
//!    concept-based; a hybrid `+ES + +Life` simultaneously satisfies
//!    `EnergyShield` and `Life` targets.

use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

use crate::ids::{BaseTypeId, ModId, TagId};

// -------------------------------------------------------------------------
// Rarity
// -------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Rarity {
    Normal,
    Magic,
    Rare,
    Unique,
}

impl Rarity {
    /// Maximum number of *explicit* prefixes the item can hold at this rarity.
    pub const fn max_prefixes(self, base_class_max: u8) -> u8 {
        match self {
            Self::Normal => 0,
            Self::Magic => 1,
            Self::Rare | Self::Unique => base_class_max,
        }
    }
    pub const fn max_suffixes(self, base_class_max: u8) -> u8 {
        match self {
            Self::Normal => 0,
            Self::Magic => 1,
            Self::Rare | Self::Unique => base_class_max,
        }
    }
}

// -------------------------------------------------------------------------
// AffixType
// -------------------------------------------------------------------------

/// Where on the item this mod sits.
///
/// In PoE2 desecrated mods occupy regular Prefix/Suffix slots — they are
/// classified by the [`crate::mods::ModKind::Desecrated`] kind, not by a
/// dedicated `Desecrated` affix variant. Implicits and enchantments have
/// their own slots and do not contribute to prefix/suffix occupancy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AffixType {
    Prefix,
    Suffix,
    Implicit,
    Enchantment,
}

impl AffixType {
    /// True for the rollable explicit affix slots.
    pub const fn is_rollable(self) -> bool {
        matches!(self, Self::Prefix | Self::Suffix)
    }
}

// -------------------------------------------------------------------------
// ModRoll
// -------------------------------------------------------------------------

/// An instance of a mod sitting on an item.
///
/// `values` parallels the corresponding [`crate::mods::ModDefinition::stats`]
/// entries: one rolled `f64` per stat. Hybrid mods carry multiple values.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModRoll {
    pub mod_id: ModId,
    pub affix_type: AffixType,
    pub kind: crate::mods::ModKind,
    /// One rolled value per stat in the parent ModDefinition's `stats` array.
    pub values: SmallVec<[f64; 4]>,
    /// Has this roll been Fracturing-Orb'd? Fractured rolls are immutable
    /// (Divine cannot reroll, Annul/Chaos cannot remove) until corruption.
    pub is_fractured: bool,
}

impl ModRoll {
    pub fn new(mod_id: ModId, affix: AffixType, kind: crate::mods::ModKind) -> Self {
        Self {
            mod_id,
            affix_type: affix,
            kind,
            values: SmallVec::new(),
            is_fractured: false,
        }
    }
}

// -------------------------------------------------------------------------
// HiddenDesecratedSlot
// -------------------------------------------------------------------------

/// A slot occupied by an unrevealed desecrated mod (post-bone, pre-reveal).
///
/// Per planning notes & user-supplied worked example:
/// - Counts toward Fracturing Orb's `mod_count >= 4` requirement.
/// - **Cannot** be the fracture target (Fracturing Orb errors with
///   [`crate::error::EngineError::FractureHiddenMod`] if it would otherwise be
///   selected; in practice the engine just samples over the visible mod set).
/// - Reveal-at-Well-of-Souls converts this into a regular [`ModRoll`] of
///   [`crate::mods::ModKind::Desecrated`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HiddenDesecratedSlot {
    /// Forced affix-type by Sinistral / Dextral Necromancy omen at bone-apply
    /// time. If neither omen was active, the engine picks uniformly from the
    /// set of empty affix slots at apply time.
    pub affix_type: AffixType,
    pub bone_size: BoneSize,
    pub bone_subtype: BoneSubtype,
    /// If a Lord-targeting omen (Blackblooded / Liege / Sovereign) was
    /// active when the bone was applied, this restricts the reveal pool.
    /// Only valid for `bone_subtype == Jawbone | Collarbone` (weapons /
    /// jewellery).
    pub abyss_lord: Option<AbyssLord>,
    /// Effective Minimum Modifier Level for the revealed mod, resolved at
    /// bone-apply time from the bone size (Ancient = 40) and bricked to 0
    /// when a lord-targeting omen consumed the Ancient floor. Reveal
    /// sampling filters out tiers whose `required_level` is below this.
    /// Defaulted for backward-compatible deserialization of pre-P3 state.
    #[serde(default)]
    pub min_mod_level: u32,
    /// True when the slot came from an Altered Collarbone (0.5 breach
    /// desecration): the reveal pool then ALSO offers Otherworldly mods
    /// (`ModFlags::OTHERWORLDLY`), which regular bones never surface.
    #[serde(default)]
    pub otherworldly: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BoneSize {
    /// Campaign-tier bone. **Cannot desecrate items above ilvl 64.**
    Gnawed,
    /// Any ilvl, no modifier-level floor.
    Preserved,
    /// Any ilvl, but guarantees a **Minimum Modifier Level of 40** on the
    /// revealed mod (unless a lord-targeting omen bricks the floor).
    Ancient,
    /// Breach-desecration size — exists only as "Altered Collarbone" and
    /// unlocks Otherworldly reveal options on rare jewellery (0.5).
    Altered,
}

impl BoneSize {
    /// Maximum item level this bone size may be applied to. `None` = no
    /// ceiling. Gnawed bones are campaign-tier and reject ilvl > 64.
    /// (poe2db Abyssal Bones; wiki Desecrated Modifier.)
    pub const fn max_ilvl(self) -> Option<u32> {
        match self {
            Self::Gnawed => Some(64),
            Self::Preserved | Self::Ancient | Self::Altered => None,
        }
    }

    /// Minimum modifier level guaranteed on the revealed desecrated mod.
    /// Ancient bones guarantee `>= 40`; Gnawed/Preserved have no floor.
    /// A lord-targeting omen (Liege/Sovereign/Blackblooded) **bricks** the
    /// Ancient floor — see [`crate::currency::bone`].
    pub const fn min_mod_level(self) -> u32 {
        match self {
            Self::Gnawed | Self::Preserved | Self::Altered => 0,
            Self::Ancient => 40,
        }
    }

    /// Which size × subtype combinations exist as real 0.5 currency items
    /// (poe2db Desecrated_Modifiers): Gnawed/Preserved/Ancient exist for
    /// Rib/Jawbone/Collarbone; Cranium exists ONLY as Preserved; Altered
    /// exists ONLY as Collarbone ("Altered Collarbone — Desecrates a Rare
    /// Amulet, Ring or Belt with a chance for otherworldly modifiers").
    pub const fn valid_with(self, subtype: BoneSubtype) -> bool {
        match self {
            Self::Altered => matches!(subtype, BoneSubtype::Collarbone),
            Self::Gnawed | Self::Ancient => !matches!(subtype, BoneSubtype::Cranium),
            Self::Preserved => true,
        }
    }
}

/// Bone subtype determines which item classes accept the bone and which
/// desecrated mod pool gets sampled at reveal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BoneSubtype {
    /// Ring, Amulet, Belt, Talisman.
    Collarbone,
    /// All weapons (one- and two-hand) plus Quiver.
    Jawbone,
    /// Armour pieces: BodyArmour, Helmet, Boots, Gloves.
    Rib,
    /// Jewel (only `Preserved` size exists per current data).
    Cranium,
}

impl BoneSubtype {
    /// Item classes that accept this bone subtype (M14.6).
    ///
    /// Source: `docs/81-engine-training-and-rule-encoding-plan.md` §4.6
    /// Tier 1.6, cross-checked with poe2db Abyssal Bones documentation.
    /// Vertebrae → Waystone is a known gap because the engine's
    /// [`BoneSubtype`] enum doesn't yet include `Vertebrae`; that lands
    /// when waystone crafting graduates beyond the v3 deferred-scope list.
    pub const fn valid_classes(self) -> &'static [&'static str] {
        match self {
            // "Desecrates a Rare Armour" (poe2db) — includes off-hand
            // armour pieces; their 0.5 desecrated pools are non-empty
            // (Shield 20 / Buckler 14 / Focus 18 mods).
            Self::Rib => &[
                "BodyArmour",
                "Helmet",
                "Boots",
                "Gloves",
                "Shield",
                "Buckler",
                "Focus",
            ],
            Self::Jawbone => &[
                "OneHandSword",
                "TwoHandSword",
                "OneHandAxe",
                "TwoHandAxe",
                "OneHandMace",
                "TwoHandMace",
                "Bow",
                "Crossbow",
                "Spear",
                "Staff",
                "Sceptre",
                "Wand",
                "Dagger",
                "Claw",
                "Warstaff",
                "Flail",
                "Quiver",
            ],
            Self::Collarbone => &["Ring", "Amulet", "Belt", "Talisman"],
            Self::Cranium => &["Jewel"],
        }
    }

    /// True iff this subtype's pool can yield a lord-targeted desecrated
    /// mod (Liege=Amanamu / Sovereign=Ulaman / Blackblooded=Kurgal).
    ///
    /// **Lord-targeting omens only work on Weapons and Jewellery.** Per the
    /// wiki (Omen of the Liege / Sovereign / Blackblooded), these omens
    /// "only work when desecrating weapons and jewellery; they do not have
    /// any effect on armour items, jewels, or waystones."
    ///
    /// Mapping to bone subtypes:
    /// - `Jawbone` → weapons + quiver ⇒ supports lord pool.
    /// - `Collarbone` → ring/amulet/belt/talisman (jewellery) ⇒ supports.
    /// - `Rib` → armour ⇒ does NOT support (lord omen is wasted).
    /// - `Cranium` → jewel uses the `Lightless`/`of the Abyss` pool ⇒ does
    ///   NOT support.
    pub const fn supports_lord_pool(self) -> bool {
        matches!(self, Self::Jawbone | Self::Collarbone)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AbyssLord {
    Kurgal,
    Amanamu,
    Ulaman,
}

// -------------------------------------------------------------------------
// Sockets
// -------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Socket {
    /// What's slotted in (rune / soul core / talisman / idol). `None` for empty.
    pub augment: Option<AugmentSlot>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AugmentSlot {
    /// Rune ID (e.g., `"GreaterIronRune"`, `"RuneOfTheChase"`).
    Rune(crate::ids::ModId),
    /// Soul core ID.
    SoulCore(crate::ids::ModId),
    /// Talisman ID — note: in 0.4 nomenclature this is the OLD "augment talisman"
    /// (now renamed to Idol). The new 0.4 weapon class is also called
    /// "Talisman" — that one is NOT a socketable.
    Idol(crate::ids::ModId),
}

// -------------------------------------------------------------------------
// Quality types
// -------------------------------------------------------------------------

/// Quality on rings/amulets is single-tag (a [`Catalyst`](crate::ids::TagId)
/// pins the quality to a specific tag); on other items it's untagged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum QualityKind {
    /// Untagged quality (whetstone / armourer's scrap / glassblower).
    Untagged,
    /// Catalyst-applied quality on a ring or amulet — quality is tagged to a
    /// specific catalyst. Replacing with a different catalyst's quality
    /// resets to 0 first.
    Tagged(TagId),
}

// -------------------------------------------------------------------------
// Item
// -------------------------------------------------------------------------

/// Runtime state of an item.
///
/// `base` is polymorphic in v3 transitional state (M14.2):
/// - **Pipeline-built items** populate `base` with the canonical bundle
///   `BaseTypeId` (e.g., `"Metadata/Items/Armours/BodyArmours/FourBodyInt3"`).
///   The engine consults [`crate::BaseRegistry`] to resolve item class and
///   tags from this id.
/// - **Test fixtures and pre-v3 legacy state** populate `base` with a
///   PascalCase item-class id (e.g., `"BodyArmour"`). The class-resolution
///   helper falls back to `ItemClassId::from(item.base.as_str())` when the
///   registry doesn't recognize the id.
///
/// M14.7 introduces a hard-reset bundle-schema bump that wipes legacy state
/// and guarantees every imported item ships a real `BaseTypeId`; the
/// polymorphic interpretation is removed at that point.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Item {
    pub base: BaseTypeId,
    pub ilvl: u32,
    pub rarity: Rarity,

    // ---- Locks ------------------------------------------------------------
    /// Vaal-corrupted. Forbids most further crafting; allows Architect's Orb
    /// double-corrupt, Vaal Cultivation Orb (uniques), some omen-finishers.
    pub corrupted: bool,
    /// Sanctified by Divine + Omen of Sanctification. Forbids further crafting.
    pub sanctified: bool,
    /// Mirrored by Mirror of Kalandra. Items behind a mirror are read-only.
    pub mirrored: bool,

    // ---- Quality / catalysts --------------------------------------------
    /// 0..=20 (or 0..=30 for "Exceptional" base + certain uniques).
    pub quality: u8,
    pub quality_kind: QualityKind,

    // ---- Mod slots -------------------------------------------------------
    pub implicits: SmallVec<[ModRoll; 2]>,
    pub prefixes: SmallVec<[ModRoll; 3]>,
    pub suffixes: SmallVec<[ModRoll; 3]>,
    pub enchantments: SmallVec<[ModRoll; 2]>,
    /// Hidden desecrated mod from a bone, awaiting reveal.
    pub hidden_desecrated: Option<HiddenDesecratedSlot>,

    // ---- Sockets / augments ---------------------------------------------
    pub sockets: SmallVec<[Socket; 2]>,

    // ---- Hinekora's Lock state ------------------------------------------
    /// `Some(seed)` when the item is currently bound by Hinekora's Lock.
    /// The lock's seed is used (instead of the live RNG) for the next
    /// modifying operation, then consumed. Preview mode draws from the
    /// same seed without consuming. See `crate::engine::apply_currency`
    /// and `crate::engine::preview_currency` for the orchestration layer.
    pub hinekora_lock: Option<u64>,
}

impl Item {
    /// Number of *visible explicit* mods on the item — counts revealed prefixes
    /// and suffixes. Excludes implicits, enchantments, and hidden desecrated.
    pub fn visible_explicit_mod_count(&self) -> usize {
        self.prefixes.len() + self.suffixes.len()
    }

    /// Does the item carry a crafted modifier (Alloy / Emotion / Genesis
    /// crafted-node output)? 0.5 limits items to one crafted mod at a time.
    pub fn has_crafted_mod(&self) -> bool {
        self.prefixes
            .iter()
            .chain(self.suffixes.iter())
            .any(|m| m.kind == crate::mods::ModKind::Crafted)
    }

    /// Number of *revealed* desecrated mods on the item. 0.5 limits items
    /// to one desecrated modifier (the hidden slot is gated separately).
    pub fn desecrated_mod_count(&self) -> usize {
        self.prefixes
            .iter()
            .chain(self.suffixes.iter())
            .filter(|m| m.kind == crate::mods::ModKind::Desecrated)
            .count()
    }

    /// Total mod count for Fracturing Orb's eligibility check.
    ///
    /// Includes the hidden-desecrated slot (which counts) but not implicits
    /// or enchantments. Per the worked example, an item with 3 prefixes + 1
    /// hidden suffix returns `4` here, and Fracturing Orb is eligible.
    pub fn fracturing_eligibility_count(&self) -> usize {
        self.visible_explicit_mod_count() + usize::from(self.hidden_desecrated.is_some())
    }

    /// Iterate over fracture-targetable mods (visible + non-fractured).
    /// Used by Fracturing Orb to sample uniformly. Hidden desecrated and
    /// already-fractured mods are excluded.
    pub fn fracture_targets(&self) -> impl Iterator<Item = &ModRoll> {
        self.prefixes
            .iter()
            .chain(self.suffixes.iter())
            .filter(|m| !m.is_fractured)
    }

    /// Are any mods on this item fractured?
    pub fn has_fractured(&self) -> bool {
        self.prefixes.iter().any(|m| m.is_fractured) || self.suffixes.iter().any(|m| m.is_fractured)
    }

    /// Convenience: is the item still craftable (not corrupted, not sanctified, not mirrored)?
    pub fn is_modifiable(&self) -> bool {
        !self.corrupted && !self.sanctified && !self.mirrored
    }

    /// Mod-group exclusivity check: would adding `group` (the group of a
    /// candidate mod) be blocked by an existing mod on the item?
    ///
    /// Returns `true` iff any existing prefix or suffix has a mod whose
    /// `mod_group` equals `group`. Resolving each `ModRoll`'s actual group
    /// requires a `ModRegistry` lookup (M2.4); this stub takes the group set
    /// directly so call sites can pre-compute it from the registry.
    #[allow(clippy::unused_self)] // populated in M2.4 once the engine has a registry
    pub fn has_mod_group(
        &self,
        _group: &crate::mods::ModGroup,
        _registry_lookup: impl Fn(&crate::ids::ModId) -> Option<crate::mods::ModGroup>,
    ) -> bool {
        // Real implementation lands in M2.4 when ModRegistry is wired in.
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::ModId;
    use crate::mods::ModKind;

    fn fixture_item() -> Item {
        Item {
            base: "Metadata/Items/Armours/Boots/BootsInt5".into(),
            ilvl: 82,
            rarity: Rarity::Rare,
            corrupted: false,
            sanctified: false,
            mirrored: false,
            quality: 0,
            quality_kind: QualityKind::Untagged,
            implicits: SmallVec::new(),
            prefixes: SmallVec::new(),
            suffixes: SmallVec::new(),
            enchantments: SmallVec::new(),
            hidden_desecrated: None,
            sockets: SmallVec::new(),
            hinekora_lock: None,
        }
    }

    fn pf(name: &str) -> ModRoll {
        ModRoll::new(ModId::from(name), AffixType::Prefix, ModKind::Explicit)
    }
    fn sf(name: &str) -> ModRoll {
        ModRoll::new(ModId::from(name), AffixType::Suffix, ModKind::Explicit)
    }

    #[test]
    fn rarity_max_prefixes() {
        assert_eq!(Rarity::Normal.max_prefixes(3), 0);
        assert_eq!(Rarity::Magic.max_prefixes(3), 1);
        assert_eq!(Rarity::Rare.max_prefixes(3), 3);
    }

    #[test]
    fn fracturing_count_includes_hidden_desecrated() {
        // User's worked example, step 7:
        //   3 visible prefixes (T1 ES) + 1 hidden suffix (Preserved Rib + Dextral Necromancy)
        //   => 4 mods total => Fracturing Orb is eligible (≥4 requirement)
        let mut it = fixture_item();
        it.prefixes.push(pf("ESPrefix1"));
        it.prefixes.push(pf("ESPrefix2"));
        it.prefixes.push(pf("ESPrefix3"));
        it.hidden_desecrated = Some(HiddenDesecratedSlot {
            affix_type: AffixType::Suffix,
            bone_size: BoneSize::Preserved,
            bone_subtype: BoneSubtype::Rib,
            abyss_lord: None,
            min_mod_level: 0,
            otherworldly: false,
        });
        assert_eq!(it.fracturing_eligibility_count(), 4);
    }

    #[test]
    fn fracture_targets_excludes_hidden_and_fractured() {
        // The hidden desecrated mod is NOT in the fracture-target sample space.
        // Already-fractured mods are also excluded.
        let mut it = fixture_item();
        it.prefixes.push(pf("ESPrefix1"));
        it.prefixes.push(pf("ESPrefix2"));
        it.prefixes.push({
            let mut m = pf("ESPrefix3");
            m.is_fractured = true; // already fractured
            m
        });
        it.suffixes.push(sf("ResSuffix1"));
        it.hidden_desecrated = Some(HiddenDesecratedSlot {
            affix_type: AffixType::Suffix,
            bone_size: BoneSize::Preserved,
            bone_subtype: BoneSubtype::Rib,
            abyss_lord: None,
            min_mod_level: 0,
            otherworldly: false,
        });

        // 3 visible non-fractured mods (2 prefixes + 1 suffix).
        // The 1 fractured prefix and the hidden suffix are both excluded.
        let count = it.fracture_targets().count();
        assert_eq!(count, 3);
    }

    #[test]
    fn affix_type_is_rollable() {
        assert!(AffixType::Prefix.is_rollable());
        assert!(AffixType::Suffix.is_rollable());
        assert!(!AffixType::Implicit.is_rollable());
        assert!(!AffixType::Enchantment.is_rollable());
    }

    #[test]
    fn item_is_modifiable_when_clean() {
        let mut it = fixture_item();
        assert!(it.is_modifiable());
        it.corrupted = true;
        assert!(!it.is_modifiable());
        it.corrupted = false;
        it.sanctified = true;
        assert!(!it.is_modifiable());
        it.sanctified = false;
        it.mirrored = true;
        assert!(!it.is_modifiable());
    }
}
