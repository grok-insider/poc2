//! Catalysts — tagged-quality currency for rings, amulets, and jewels.
//!
//! ## Behavior (PoE2 0.4)
//!
//! - Apply to a ring, amulet, or jewel.
//! - Each catalyst is tagged: `caster`, `attack`, `life`, `breach`, etc.
//! - On apply:
//!   - If `quality_kind` is `Tagged(same_tag)`: add `increment` to quality.
//!   - If `quality_kind` is `Tagged(other_tag)` or `Untagged` with non-zero
//!     quality: reset quality to 0 first, then set tag and add increment.
//!   - If `quality_kind` is `Untagged` and quality is 0: set tag and add.
//! - Quality capped at 20 (vanilla) or 30 (Exceptional bases — TBD M2.6).
//! - Tagged quality boosts the rolled values of mods carrying that tag
//!   by `+quality%` of their value (per [docs/33-strategy-library.md] sec 18).
//!
//! Eligible item classes: `Ring`, `Amulet`, `Belt`, `Jewel`. Belts only
//! accept the breach catalyst in 0.4 per the heuristics rulebook.
//! Sanctified / mirrored / corrupted items reject catalysts.

use crate::currency::{ApplyContext, ApplyOutcome, CannotApply, Currency};
use crate::error::{EngineError, EngineResult};
use crate::ids::{CurrencyId, ItemClassId, TagId};
use crate::item::{Item, QualityKind};

/// Item classes catalysts are eligible to apply to (M14.5).
///
/// Catalysts are tagged-quality currency restricted to jewellery and
/// jewels in PoE2 0.4. Sceptres, weapons, armours, and accessories outside
/// this list reject catalysts.
const CATALYST_ELIGIBLE_CLASSES: &[&str] = &["Ring", "Amulet", "Belt", "Jewel"];

/// PascalCase class ids known to be ineligible for catalysts. Used by the
/// best-effort `Catalyst::can_apply_to` heuristic when `Item.base` carries
/// a class-id placeholder (v3 transitional state). Real-bundle items with
/// metadata-path bases pass this heuristic and get caught by the
/// registry-backed gate inside `apply()`.
const CATALYST_KNOWN_NONELIGIBLE_CLASSES: &[&str] = &[
    "BodyArmour",
    "Helmet",
    "Boots",
    "Gloves",
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

/// Resolve an item's class id for catalyst gating.
///
/// Prefers [`crate::base_registry::BaseRegistry`] when the item's `base`
/// is a real bundle id; falls back to `ItemClassId::from(item.base.as_str())`
/// for the v3 transitional period when fixtures stuff class ids into
/// `Item.base`. Same polymorphic shape as the basic-orb sampler's
/// `class_for_item`, kept inline to avoid leaking the helper across
/// currency modules.
fn resolve_class_for_catalyst(
    item: &Item,
    base_registry: &crate::base_registry::BaseRegistry,
) -> ItemClassId {
    if let Some(c) = base_registry.class_of(&item.base) {
        return c.clone();
    }
    ItemClassId::from(item.base.as_str())
}

/// Quality cap (vanilla bases). Exceptional bases will raise this to 30
/// in M2.6; we'll plumb that through `BaseType::quality_cap` then.
pub const CATALYST_QUALITY_CAP: u8 = 20;

/// Default per-apply quality increment for a normal catalyst.
pub const CATALYST_INCREMENT_DEFAULT: u8 = 5;

/// Increment for Adaptive Catalyst (Breach reward — applies any tag).
pub const CATALYST_INCREMENT_ADAPTIVE: u8 = 10;

/// One catalyst kind — fixed tag + increment.
#[derive(Debug, Clone)]
pub struct Catalyst {
    id: CurrencyId,
    tag: TagId,
    increment: u8,
    display_name: &'static str,
}

impl Catalyst {
    /// Build a catalyst targeting `tag` with the default 5%/apply.
    #[must_use]
    pub fn new(
        id: impl Into<CurrencyId>,
        display_name: &'static str,
        tag: impl Into<TagId>,
    ) -> Self {
        Self {
            id: id.into(),
            tag: tag.into(),
            increment: CATALYST_INCREMENT_DEFAULT,
            display_name,
        }
    }

    /// Build a catalyst with a custom increment (for Adaptive / Greater
    /// catalyst variants).
    #[must_use]
    pub fn with_increment(mut self, increment: u8) -> Self {
        self.increment = increment;
        self
    }

    pub const fn tag(&self) -> &TagId {
        &self.tag
    }

    pub const fn increment(&self) -> u8 {
        self.increment
    }

    // ---- Common presets ---------------------------------------------------
    //
    // The full catalyst catalogue is data-driven from the bundle. These
    // presets are convenience constructors for tests and the seed strategy
    // library.

    pub fn flesh() -> Self {
        Self::new("FleshCatalyst", "Flesh Catalyst", "life")
    }

    pub fn intrinsic() -> Self {
        Self::new("IntrinsicCatalyst", "Intrinsic Catalyst", "attribute")
    }

    pub fn reaver() -> Self {
        Self::new("ReaverCatalyst", "Reaver Catalyst", "attack")
    }

    pub fn carapace() -> Self {
        Self::new("CarapaceCatalyst", "Carapace Catalyst", "defences")
    }

    pub fn unstable() -> Self {
        Self::new("UnstableCatalyst", "Unstable Catalyst", "caster")
    }

    /// Adaptive catalyst (Breach reward) — applies the user's last-used
    /// tag. We model it as a generic "any tag" catalyst with a higher
    /// increment; callers must pass the desired tag in.
    pub fn adaptive(tag: impl Into<TagId>) -> Self {
        Self::new("AdaptiveCatalyst", "Adaptive Catalyst", tag)
            .with_increment(CATALYST_INCREMENT_ADAPTIVE)
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
    /// class outside [`CATALYST_ELIGIBLE_CLASSES`]. Best-effort against
    /// fixture items (where `Item.base` carries the class id directly);
    /// real-bundle items resolve via the registered [`crate::BaseRegistry`]
    /// only at `apply()` time, so `can_apply_to` may pass on real-bundle
    /// items the registry would later reject. The advisor double-checks
    /// at apply time so the hard error path stays correct.
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
        // `item.base` matches a known PascalCase class id that is *not* in
        // the eligible set, reject. Otherwise accept and let `apply()`
        // do the registry-backed check.
        let candidate_class = item.base.as_str();
        if CATALYST_KNOWN_NONELIGIBLE_CLASSES.contains(&candidate_class) {
            return Err(CannotApply::Other(
                "catalysts apply only to Ring / Amulet / Belt / Jewel",
            ));
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
        // Registry-backed class gate (M14.5).
        let class = resolve_class_for_catalyst(item, ctx.base_registry);
        if !CATALYST_ELIGIBLE_CLASSES.contains(&class.as_str()) {
            return Err(EngineError::InvalidApplication(format!(
                "{}: cannot apply to class {} — eligible classes are {}",
                self.display_name,
                class,
                CATALYST_ELIGIBLE_CLASSES.join(", ")
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

    fn ring() -> Item {
        Item {
            base: ItemClassId::from("Ring").as_str().into(),
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
    fn adaptive_catalyst_applies_double_increment() {
        let reg = ModRegistry::from_mods(vec![], vec![]);
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0);
        let mut omens = OmenSet::new();
        let mut item = ring();
        let adaptive = Catalyst::adaptive("life");
        adaptive
            .apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens))
            .unwrap();
        assert_eq!(item.quality, CATALYST_INCREMENT_ADAPTIVE);
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
}
