//! Catalysts — tagged-quality currency for rings, amulets, and jewels.
//!
//! ## Behavior (PoE2 0.5)
//!
//! - Exactly 24 catalysts exist (poe2db catalysts.html): 12 base kinds
//!   that apply to "a ring or amulet", and 12 `Refined` variants of the
//!   same kinds that apply to "a jewel". Belts take no catalyst in 0.5.
//!   The PoE1 names "Intrinsic Catalyst" / "Unstable Catalyst" do not
//!   exist in PoE2.
//! - Each catalyst is tagged: `life`, `mana`, `defences`, `physical`,
//!   `fire`, `cold`, `lightning`, `chaos`, `attack`, `caster`, `speed`,
//!   or `attribute` (Adaptive). All add the same +5%/apply.
//! - On apply:
//!   - If `quality_kind` is `Tagged(same_tag)`: add `increment` to quality.
//!   - If `quality_kind` is `Tagged(other_tag)` or `Untagged` with non-zero
//!     quality: reset quality to 0 first, then set tag and add increment.
//!   - If `quality_kind` is `Untagged` and quality is 0: set tag and add.
//! - Quality capped at 20.
//!   TODO(0.5): Breach Rings cap at 40/45 per poe2db quality.html; needs
//!   a per-base quality cap on `BaseType` — out of scope here.
//! - Tagged quality boosts the rolled *values* of mods carrying that tag
//!   by `+quality%` (per [docs/33-strategy-library.md] sec 18). It does
//!   NOT bias which mods roll; quality converts into mod-roll chance only
//!   via Omen of Catalysing Exaltation on the next Exalt.
//!
//! Sanctified / mirrored / corrupted items reject catalysts.

use crate::currency::{ApplyContext, ApplyOutcome, CannotApply, Currency};
use crate::error::{EngineError, EngineResult};
use crate::ids::{CurrencyId, TagId};
use crate::item::{Item, QualityKind};

/// Item classes base catalysts apply to (poe2db 0.5: "a ring or amulet").
const CATALYST_BASE_CLASSES: &[&str] = &["Ring", "Amulet"];

/// Item classes Refined catalysts apply to (poe2db 0.5: "a jewel").
const CATALYST_REFINED_CLASSES: &[&str] = &["Jewel"];

/// PascalCase class ids known to be ineligible for any catalyst. Used by
/// the best-effort `Catalyst::can_apply_to` heuristic when `Item.base`
/// carries a class-id placeholder (v3 transitional state). Real-bundle
/// items with metadata-path bases pass this heuristic and get caught by
/// the registry-backed gate inside `apply()`.
const CATALYST_KNOWN_NONELIGIBLE_CLASSES: &[&str] = &[
    "BodyArmour",
    "Helmet",
    "Boots",
    "Gloves",
    "Belt",
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
    "Quiver",
    "Focus",
    "Talisman",
    "Waystone",
    "Charm",
    "Tablet",
];

/// Quality cap. TODO(0.5): Breach Rings raise this to 40/45 per poe2db
/// quality.html; needs a per-base cap on `BaseType` — out of scope here.
pub const CATALYST_QUALITY_CAP: u8 = 20;

/// Per-apply quality increment. All 24 catalysts in 0.5 add +5%
/// (poe2db catalysts.html); Adaptive is not special.
pub const CATALYST_INCREMENT_DEFAULT: u8 = 5;

/// Which class family a catalyst gates on. poe2db 0.5: the 12 base
/// catalysts apply to "a ring or amulet"; the 12 `Refined` variants
/// apply to "a jewel". No catalyst covers both families.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalystTarget {
    /// Base catalysts — rings and amulets only.
    RingAmulet,
    /// Refined catalysts — jewels only.
    Jewel,
}

impl CatalystTarget {
    /// Item classes this family may apply to.
    #[must_use]
    pub const fn eligible_classes(self) -> &'static [&'static str] {
        match self {
            Self::RingAmulet => CATALYST_BASE_CLASSES,
            Self::Jewel => CATALYST_REFINED_CLASSES,
        }
    }

    const fn gate_message(self) -> &'static str {
        match self {
            Self::RingAmulet => "catalysts apply only to a Ring or Amulet",
            Self::Jewel => "Refined catalysts apply only to a Jewel",
        }
    }
}

/// One catalyst kind — fixed tag + increment + class-family gate.
#[derive(Debug, Clone)]
pub struct Catalyst {
    id: CurrencyId,
    tag: TagId,
    increment: u8,
    display_name: &'static str,
    target: CatalystTarget,
}

/// The 12 catalyst kinds of PoE2 0.5 (poe2db catalysts.html), as
/// `(base id, base name, refined id, refined name, tag)`. Base entries
/// gate on rings/amulets; `Refined` entries gate on jewels.
const CATALYST_KINDS_0_5: &[(&str, &str, &str, &str, &str)] = &[
    (
        "FleshCatalyst",
        "Flesh Catalyst",
        "RefinedFleshCatalyst",
        "Refined Flesh Catalyst",
        "life",
    ),
    (
        "NeuralCatalyst",
        "Neural Catalyst",
        "RefinedNeuralCatalyst",
        "Refined Neural Catalyst",
        "mana",
    ),
    (
        "CarapaceCatalyst",
        "Carapace Catalyst",
        "RefinedCarapaceCatalyst",
        "Refined Carapace Catalyst",
        "defences",
    ),
    (
        "UulNetolsCatalyst",
        "Uul-Netol's Catalyst",
        "RefinedUulNetolsCatalyst",
        "Refined Uul-Netol's Catalyst",
        "physical",
    ),
    (
        "XophsCatalyst",
        "Xoph's Catalyst",
        "RefinedXophsCatalyst",
        "Refined Xoph's Catalyst",
        "fire",
    ),
    (
        "TulsCatalyst",
        "Tul's Catalyst",
        "RefinedTulsCatalyst",
        "Refined Tul's Catalyst",
        "cold",
    ),
    (
        "EshsCatalyst",
        "Esh's Catalyst",
        "RefinedEshsCatalyst",
        "Refined Esh's Catalyst",
        "lightning",
    ),
    (
        "ChayulasCatalyst",
        "Chayula's Catalyst",
        "RefinedChayulasCatalyst",
        "Refined Chayula's Catalyst",
        "chaos",
    ),
    (
        "ReaverCatalyst",
        "Reaver Catalyst",
        "RefinedReaverCatalyst",
        "Refined Reaver Catalyst",
        "attack",
    ),
    (
        "SibilantCatalyst",
        "Sibilant Catalyst",
        "RefinedSibilantCatalyst",
        "Refined Sibilant Catalyst",
        "caster",
    ),
    (
        "SkitteringCatalyst",
        "Skittering Catalyst",
        "RefinedSkitteringCatalyst",
        "Refined Skittering Catalyst",
        "speed",
    ),
    (
        "AdaptiveCatalyst",
        "Adaptive Catalyst",
        "RefinedAdaptiveCatalyst",
        "Refined Adaptive Catalyst",
        "attribute",
    ),
];

impl Catalyst {
    /// Build a catalyst targeting `tag` with the standard 5%/apply.
    ///
    /// The class-family gate derives from the canonical name prefix: ids
    /// starting with `Refined` gate on jewels, everything else on
    /// rings/amulets. The bundle catalogue carries no structured family
    /// field, so the prefix is the only signal available there.
    #[must_use]
    pub fn new(
        id: impl Into<CurrencyId>,
        display_name: &'static str,
        tag: impl Into<TagId>,
    ) -> Self {
        let id = id.into();
        let target = if id.as_str().starts_with("Refined") {
            CatalystTarget::Jewel
        } else {
            CatalystTarget::RingAmulet
        };
        Self {
            id,
            tag: tag.into(),
            increment: CATALYST_INCREMENT_DEFAULT,
            display_name,
            target,
        }
    }

    pub const fn tag(&self) -> &TagId {
        &self.tag
    }

    pub const fn increment(&self) -> u8 {
        self.increment
    }

    pub const fn target(&self) -> CatalystTarget {
        self.target
    }

    /// Item classes this catalyst may apply to (family-dependent).
    #[must_use]
    pub const fn eligible_classes(&self) -> &'static [&'static str] {
        self.target.eligible_classes()
    }

    // ---- Presets (PoE2 0.5 catalogue) -------------------------------------
    //
    // The full catalyst catalogue is data-driven from the bundle. These
    // presets mirror poe2db catalysts.html so the resolver, tests, and
    // the seed strategy library work without a bundle.

    /// The full 0.5 catalogue: 12 base + 12 Refined catalysts.
    #[must_use]
    pub fn default_catalogue() -> Vec<Self> {
        CATALYST_KINDS_0_5
            .iter()
            .flat_map(|&(base_id, base_name, refined_id, refined_name, tag)| {
                [
                    Self::new(base_id, base_name, tag),
                    Self::new(refined_id, refined_name, tag),
                ]
            })
            .collect()
    }

    pub fn flesh() -> Self {
        Self::new("FleshCatalyst", "Flesh Catalyst", "life")
    }

    pub fn neural() -> Self {
        Self::new("NeuralCatalyst", "Neural Catalyst", "mana")
    }

    pub fn reaver() -> Self {
        Self::new("ReaverCatalyst", "Reaver Catalyst", "attack")
    }

    pub fn carapace() -> Self {
        Self::new("CarapaceCatalyst", "Carapace Catalyst", "defences")
    }

    pub fn sibilant() -> Self {
        Self::new("SibilantCatalyst", "Sibilant Catalyst", "caster")
    }

    /// Adaptive Catalyst — the fixed attribute-tag kind. Not a wildcard:
    /// poe2db 0.5 lists it as a normal +5% catalyst like the other 11.
    pub fn adaptive() -> Self {
        Self::new("AdaptiveCatalyst", "Adaptive Catalyst", "attribute")
    }

    /// Refined Flesh Catalyst — jewel-gated life catalyst.
    pub fn refined_flesh() -> Self {
        Self::new("RefinedFleshCatalyst", "Refined Flesh Catalyst", "life")
    }
}

impl Currency for Catalyst {
    fn id(&self) -> &CurrencyId {
        &self.id
    }

    fn name(&self) -> &'static str {
        self.display_name
    }

    /// Pre-flight class gate. Rejects items whose `base` resolves to a
    /// class outside this catalyst's family ([`Catalyst::eligible_classes`]).
    /// Best-effort against fixture items (where `Item.base` carries the
    /// class id directly); real-bundle items resolve via the registered
    /// [`crate::BaseRegistry`] only at `apply()` time, so `can_apply_to`
    /// may pass on real-bundle items the registry would later reject. The
    /// advisor double-checks at apply time so the hard error path stays
    /// correct.
    fn can_apply_to(&self, item: &Item) -> Result<(), CannotApply> {
        let valid = self.valid_rarities();
        if !valid.contains(item.rarity) {
            return Err(CannotApply::WrongRarity {
                item_rarity: item.rarity,
                expected: valid,
            });
        }
        if item.mirrored {
            return Err(CannotApply::Mirrored);
        }
        if item.corrupted {
            return Err(CannotApply::Corrupted);
        }
        // Best-effort class check using the item.base placeholder. If
        // `item.base` matches a known PascalCase class id that is outside
        // this catalyst's family (including the other family's classes),
        // reject. Otherwise accept and let `apply()` do the registry-backed
        // check.
        let candidate_class = item.base.as_str();
        let known_class = CATALYST_KNOWN_NONELIGIBLE_CLASSES.contains(&candidate_class)
            || CATALYST_BASE_CLASSES.contains(&candidate_class)
            || CATALYST_REFINED_CLASSES.contains(&candidate_class);
        if known_class && !self.eligible_classes().contains(&candidate_class) {
            return Err(CannotApply::Other(self.target.gate_message()));
        }
        Ok(())
    }

    fn apply(&self, item: &mut Item, ctx: &mut ApplyContext<'_>) -> EngineResult<ApplyOutcome> {
        if item.sanctified {
            return Err(EngineError::ItemSanctified);
        }
        if item.mirrored {
            return Err(EngineError::InvalidApplication(
                "Catalyst cannot be applied to a mirrored item".into(),
            ));
        }
        if item.corrupted {
            return Err(EngineError::ItemCorrupted);
        }
        // Registry-backed class gate (M14.5; 0.5 family split).
        let class = ctx.base_registry.resolve_item_class(item);
        let eligible = self.eligible_classes();
        if !eligible.contains(&class.as_str()) {
            return Err(EngineError::InvalidApplication(format!(
                "{}: cannot apply to class {} — eligible classes are {}",
                self.display_name,
                class,
                eligible.join(", ")
            )));
        }

        // Quality is already at the cap → reject (player should know).
        if item.quality >= CATALYST_QUALITY_CAP {
            if let QualityKind::Tagged(t) = &item.quality_kind {
                if t == &self.tag {
                    return Err(EngineError::InvalidApplication(format!(
                        "{} already at the {}% cap with matching tag",
                        self.display_name, CATALYST_QUALITY_CAP
                    )));
                }
            }
        }

        // Tag-switch: reset quality if tag changes (or we're switching from
        // untagged → tagged with non-zero quality).
        let needs_reset = match &item.quality_kind {
            QualityKind::Untagged => item.quality > 0,
            QualityKind::Tagged(t) => t != &self.tag,
        };
        if needs_reset {
            item.quality = 0;
        }
        item.quality_kind = QualityKind::Tagged(self.tag.clone());
        item.quality = item
            .quality
            .saturating_add(self.increment)
            .min(CATALYST_QUALITY_CAP);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::ItemClassId;
    use crate::item::Rarity;
    use crate::omen::OmenSet;
    use crate::patch::PatchVersion;
    use crate::registry::ModRegistry;
    use rand::SeedableRng;
    use rand_xoshiro::Xoshiro256PlusPlus;
    use smallvec::smallvec;

    fn item_of_class(class: &str) -> Item {
        Item {
            base: ItemClassId::from(class).as_str().into(),
            ilvl: 82,
            rarity: Rarity::Rare,
            corrupted: false,
            sanctified: false,
            mirrored: false,
            quality: 0,
            quality_kind: QualityKind::Untagged,
            implicits: smallvec![],
            prefixes: smallvec![],
            suffixes: smallvec![],
            enchantments: smallvec![],
            hidden_desecrated: None,
            sockets: smallvec![],
            hinekora_lock: None,
        }
    }

    fn ring() -> Item {
        item_of_class("Ring")
    }

    fn ctx<'a>(
        reg: &'a ModRegistry,
        rng: &'a mut Xoshiro256PlusPlus,
        omens: &'a mut OmenSet,
    ) -> ApplyContext<'a> {
        ApplyContext::new_without_bases(reg, rng, PatchVersion::PATCH_0_4_0, omens)
    }

    #[test]
    fn first_apply_tags_and_adds_increment() {
        let reg = ModRegistry::from_mods(vec![], vec![]);
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0);
        let mut omens = OmenSet::new();
        let mut item = ring();
        let cat = Catalyst::flesh();
        cat.apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens))
            .unwrap();
        assert_eq!(item.quality, CATALYST_INCREMENT_DEFAULT);
        assert_eq!(item.quality_kind, QualityKind::Tagged(TagId::from("life")));
    }

    #[test]
    fn matching_tag_increments_quality() {
        let reg = ModRegistry::from_mods(vec![], vec![]);
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0);
        let mut omens = OmenSet::new();
        let mut item = ring();
        let cat = Catalyst::flesh();
        for _ in 0..4 {
            cat.apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens))
                .unwrap();
        }
        assert_eq!(item.quality, 4 * CATALYST_INCREMENT_DEFAULT);
        assert_eq!(item.quality_kind, QualityKind::Tagged(TagId::from("life")));
    }

    #[test]
    fn switching_tag_resets_then_adds_increment() {
        let reg = ModRegistry::from_mods(vec![], vec![]);
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0);
        let mut omens = OmenSet::new();
        let mut item = ring();
        let life_cat = Catalyst::flesh();
        let attack_cat = Catalyst::reaver();
        life_cat
            .apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens))
            .unwrap();
        life_cat
            .apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens))
            .unwrap();
        assert_eq!(item.quality, 2 * CATALYST_INCREMENT_DEFAULT);

        attack_cat
            .apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens))
            .unwrap();
        assert_eq!(item.quality, CATALYST_INCREMENT_DEFAULT);
        assert_eq!(
            item.quality_kind,
            QualityKind::Tagged(TagId::from("attack"))
        );
    }

    #[test]
    fn quality_capped_at_20() {
        let reg = ModRegistry::from_mods(vec![], vec![]);
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0);
        let mut omens = OmenSet::new();
        let mut item = ring();
        let cat = Catalyst::flesh();
        // 5*5 = 25, capped to 20.
        for _ in 0..5 {
            let _ = cat.apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens));
        }
        assert_eq!(item.quality, CATALYST_QUALITY_CAP);
    }

    #[test]
    fn adaptive_catalyst_is_attribute_tag_with_default_increment() {
        let reg = ModRegistry::from_mods(vec![], vec![]);
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0);
        let mut omens = OmenSet::new();
        let mut item = ring();
        let adaptive = Catalyst::adaptive();
        adaptive
            .apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens))
            .unwrap();
        assert_eq!(item.quality, CATALYST_INCREMENT_DEFAULT);
        assert_eq!(
            item.quality_kind,
            QualityKind::Tagged(TagId::from("attribute"))
        );
    }

    #[test]
    fn corrupted_item_rejects_catalyst() {
        let reg = ModRegistry::from_mods(vec![], vec![]);
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0);
        let mut omens = OmenSet::new();
        let mut item = ring();
        item.corrupted = true;
        let cat = Catalyst::flesh();
        let r = cat.apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens));
        assert!(matches!(r, Err(EngineError::ItemCorrupted)));
    }

    #[test]
    fn sanctified_item_rejects_catalyst() {
        let reg = ModRegistry::from_mods(vec![], vec![]);
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0);
        let mut omens = OmenSet::new();
        let mut item = ring();
        item.sanctified = true;
        let cat = Catalyst::flesh();
        let r = cat.apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens));
        assert!(matches!(r, Err(EngineError::ItemSanctified)));
    }

    #[test]
    fn untagged_quality_resets_on_switch_to_tagged() {
        let reg = ModRegistry::from_mods(vec![], vec![]);
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0);
        let mut omens = OmenSet::new();
        let mut item = ring();
        // Pretend the item arrived with untagged quality 10 (e.g., from a Glassblower).
        item.quality = 10;
        item.quality_kind = QualityKind::Untagged;
        let cat = Catalyst::flesh();
        cat.apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens))
            .unwrap();
        assert_eq!(item.quality, CATALYST_INCREMENT_DEFAULT);
        assert_eq!(item.quality_kind, QualityKind::Tagged(TagId::from("life")));
    }

    #[test]
    fn base_catalyst_rejects_belt_and_jewel() {
        let cat = Catalyst::flesh();
        for class in ["Belt", "Jewel"] {
            let item = item_of_class(class);
            assert!(
                matches!(cat.can_apply_to(&item), Err(CannotApply::Other(_))),
                "base catalyst must reject {class} in 0.5"
            );
        }
    }

    #[test]
    fn refined_catalyst_gates_on_jewel_only() {
        let cat = Catalyst::refined_flesh();
        assert_eq!(cat.target(), CatalystTarget::Jewel);
        assert!(cat.can_apply_to(&item_of_class("Jewel")).is_ok());
        for class in ["Ring", "Amulet", "Belt"] {
            let item = item_of_class(class);
            assert!(
                matches!(cat.can_apply_to(&item), Err(CannotApply::Other(_))),
                "refined catalyst must reject {class}"
            );
        }
    }

    #[test]
    fn default_catalogue_is_the_24_of_0_5() {
        let all = Catalyst::default_catalogue();
        assert_eq!(all.len(), 24);
        let base = all
            .iter()
            .filter(|c| c.target() == CatalystTarget::RingAmulet)
            .count();
        let refined = all
            .iter()
            .filter(|c| c.target() == CatalystTarget::Jewel)
            .count();
        assert_eq!((base, refined), (12, 12));
        // All increments are the standard +5 — no Adaptive special case.
        assert!(all
            .iter()
            .all(|c| c.increment() == CATALYST_INCREMENT_DEFAULT));
        // PoE1 names must not appear.
        assert!(all.iter().all(
            |c| !c.id().as_str().contains("Intrinsic") && !c.id().as_str().contains("Unstable")
        ));
    }
}
