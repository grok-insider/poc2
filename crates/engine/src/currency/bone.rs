//! Desecration bones + Well-of-Souls reveal.
//!
//! Bones occupy an item slot with a *hidden* desecrated mod. The mod's
//! identity is decided later via [`reveal_at_well_of_souls`], which samples
//! a small set of options (default 3, or 6 with Omen of Abyssal Echoes
//! in M2.6) from the desecrated mod pool and lets the caller pick one.
//!
//! ## Eligibility
//!
//! - Item must be Rare and modifiable.
//! - Item must NOT already carry a hidden desecrated mod (one at a time).
//! - Item must have a free affix slot. Without an active Necromancy omen
//!   the engine picks Prefix-or-Suffix uniformly across open slots; with
//!   Sinistral/Dextral Necromancy active (M2.6) the choice is forced.
//! - Bone subtype gates which item classes accept the bone:
//!   `Jawbone` → weapons & quivers; `Rib` → armour; `Collarbone` → necklace,
//!   ring, belt; `Cranium` → jewel. M2.5 enforces this via a class-tag check
//!   the caller passes in (the engine doesn't yet have a `BaseRegistry`
//!   mapping `BaseTypeId` → class tags; M2.4-followup item).
//!
//! ## Reveal pool
//!
//! Real desecrated mod definitions land via the poe2db pipeline in M2.5b.
//! Until then, the reveal sampler accepts a caller-provided `pool` slice;
//! tests provide a small synthetic pool to verify the mechanic.

use rand::seq::SliceRandom;
use rand::Rng;
use smallvec::SmallVec;

use crate::currency::{ApplyContext, ApplyOutcome, CannotApply, Currency};
use crate::error::{EngineError, EngineResult};
use crate::ids::{CurrencyId, ModId};
use crate::item::{
    AbyssLord, AffixType, BoneSize, BoneSubtype, HiddenDesecratedSlot, Item, ModRoll, Rarity,
};
use crate::mods::{ModDefinition, ModKind};

/// Item classes whose desecrated pools are *empty* — applying a bone is
/// legal mechanically but the mods sampled at reveal won't include any
/// lord-pool entries. Per the poe2db Desecrated Modifier table, sceptres
/// have no exclusive desecrated mods at all; lord-targeting omens
/// (Blackblooded/Liege/Sovereign) on sceptres are pure waste and are
/// rejected at `Bone::apply` time.
const SCEPTRE_CLASSES_NO_EXCLUSIVE_DESECRATED: &[&str] = &["Sceptre"];

// =========================================================================
// Bone — applies a hidden desecrated mod to an item
// =========================================================================

/// A desecration bone (size × subtype).
#[derive(Debug)]
pub struct Bone {
    pub size: BoneSize,
    pub subtype: BoneSubtype,
    id: CurrencyId,
}

impl Bone {
    pub fn new(size: BoneSize, subtype: BoneSubtype) -> Self {
        // Compose a stable id like "PreservedRib" / "AncientJawbone".
        let id = format!("{size:?}{subtype:?}");
        Self {
            size,
            subtype,
            id: CurrencyId::from(id.as_str()),
        }
    }

    pub const PRESERVED_RIB: fn() -> Self = || Bone::new(BoneSize::Preserved, BoneSubtype::Rib);
    pub const ANCIENT_RIB: fn() -> Self = || Bone::new(BoneSize::Ancient, BoneSubtype::Rib);
    pub const PRESERVED_JAWBONE: fn() -> Self =
        || Bone::new(BoneSize::Preserved, BoneSubtype::Jawbone);
    pub const PRESERVED_COLLARBONE: fn() -> Self =
        || Bone::new(BoneSize::Preserved, BoneSubtype::Collarbone);
    pub const PRESERVED_CRANIUM: fn() -> Self =
        || Bone::new(BoneSize::Preserved, BoneSubtype::Cranium);

    /// Minimum modifier level guaranteed on the revealed desecrated mod,
    /// from the bone size (P3): Ancient = 40, Gnawed/Preserved = 0.
    pub const fn min_mod_level(&self) -> u32 {
        self.size.min_mod_level()
    }

    /// Shared `apply` pre-flight: modifiable Rare, no pending hidden slot,
    /// the 0.5 one-desecrated-mod cap, and the bone-size ilvl ceiling.
    fn pre_apply_checks(&self, item: &Item, patch: crate::patch::PatchVersion) -> EngineResult<()> {
        // Size × subtype combinations that exist as real currency items
        // (poe2db): Cranium is Preserved-only; Altered is Collarbone-only.
        if !self.size.valid_with(self.subtype) {
            return Err(EngineError::InvalidApplication(format!(
                "Bone: {:?} {:?} does not exist as a currency item",
                self.size, self.subtype
            )));
        }
        if !item.is_modifiable() {
            return Err(EngineError::InvalidApplication(
                "Bone requires a modifiable item".into(),
            ));
        }
        if item.rarity != Rarity::Rare {
            return Err(EngineError::InvalidApplication(
                "Bone requires a Rare item".into(),
            ));
        }
        if item.hidden_desecrated.is_some() {
            return Err(EngineError::InvalidApplication(
                "Bone: item already carries an unrevealed desecrated mod".into(),
            ));
        }
        // 0.5: "items are limited to 1 Desecrated modifier" — a revealed
        // desecrated mod blocks further desecration. (Pre-0.5, desecrated
        // mods counted as crafted mods; the engine kept the historical
        // behaviour of gating only the hidden slot.)
        if patch >= crate::patch::PatchVersion::PATCH_0_5_0 && item.desecrated_mod_count() >= 1 {
            return Err(EngineError::InvalidApplication(
                "Bone: item already has a desecrated modifier (limit 1 in 0.5)".into(),
            ));
        }
        // Bone size ilvl ceiling (P3): Gnawed cannot exceed ilvl 64.
        if let Some(max) = self.size.max_ilvl() {
            if item.ilvl > max {
                return Err(EngineError::InvalidApplication(format!(
                    "Bone {:?}: cannot desecrate item level {} (max {max} for this bone size)",
                    self.size, item.ilvl
                )));
            }
        }
        Ok(())
    }

    /// Maximum item level this bone may be applied to (Gnawed = 64).
    pub const fn max_ilvl(&self) -> Option<u32> {
        self.size.max_ilvl()
    }
}

impl Bone {
    /// Which affix sides the class's desecrated pool can actually fill
    /// (0.5 armour pools are suffix-only; a prefix-side desecration there
    /// could never reveal anything). Otherworldly mods count only for
    /// Altered bones. Errors when a populated registry has NO desecrated
    /// pool for the class — those classes (swords / axes / claws / daggers /
    /// flails / sceptres) genuinely cannot be desecrated in 0.5; their
    /// ilvl-65 pool is the unmodeled "Thrud's Might" mechanic. Empty
    /// unit-test registries keep the permissive both-sides fallback.
    fn desecratable_sides(
        &self,
        class: &crate::ids::ItemClassId,
        ctx: &ApplyContext<'_>,
    ) -> EngineResult<(bool, bool)> {
        let mut pool_prefix = false;
        let mut pool_suffix = false;
        let mut pool_known = false;
        for affix in [AffixType::Prefix, AffixType::Suffix] {
            for &idx in ctx.registry.for_class_affix(class, affix) {
                let Some(m) = ctx.registry.at(idx) else {
                    continue;
                };
                if m.kind != ModKind::Desecrated {
                    continue;
                }
                if m.flags.contains(crate::mods::ModFlags::OTHERWORLDLY)
                    && self.size != BoneSize::Altered
                {
                    continue;
                }
                pool_known = true;
                match affix {
                    AffixType::Prefix => pool_prefix = true,
                    AffixType::Suffix => pool_suffix = true,
                    _ => {}
                }
            }
        }
        if !pool_known && !ctx.registry.is_empty() {
            return Err(EngineError::InvalidApplication(format!(
                "Bone: no desecrated modifiers exist for class {class} in 0.5"
            )));
        }
        if pool_known {
            Ok((pool_prefix, pool_suffix))
        } else {
            Ok((true, true))
        }
    }
}

/// poe2db Desecrated_Modifiers: "If modifiers are full then a random
/// modifier is also removed." Generalized to per-side pools: when no
/// pool-bearing side has an open slot (e.g. a suffix-only armour pool with
/// 3 suffixes rolled), remove a random non-fractured mod from a pool side
/// so the desecration can land.
fn free_pool_side_slot(
    item: &mut Item,
    pool_prefix: bool,
    pool_suffix: bool,
    ctx: &mut ApplyContext<'_>,
) -> EngineResult<()> {
    let pool_side_open =
        (pool_prefix && item.prefixes.len() < 3) || (pool_suffix && item.suffixes.len() < 3);
    if pool_side_open {
        return Ok(());
    }
    let mut removables: smallvec::SmallVec<[(AffixType, usize); 8]> = smallvec::SmallVec::new();
    if pool_prefix {
        for (i, m) in item.prefixes.iter().enumerate() {
            if !m.is_fractured {
                removables.push((AffixType::Prefix, i));
            }
        }
    }
    if pool_suffix {
        for (i, m) in item.suffixes.iter().enumerate() {
            if !m.is_fractured {
                removables.push((AffixType::Suffix, i));
            }
        }
    }
    if removables.is_empty() {
        return Err(EngineError::InvalidApplication(
            "Bone: no desecratable slot can be freed (pool sides full or fractured)".into(),
        ));
    }
    let pick = ctx.rng.gen_range(0..removables.len());
    match removables[pick] {
        (AffixType::Prefix, i) => {
            item.prefixes.remove(i);
        }
        (AffixType::Suffix, i) => {
            item.suffixes.remove(i);
        }
        _ => unreachable!(),
    }
    Ok(())
}

impl Currency for Bone {
    fn id(&self) -> &CurrencyId {
        &self.id
    }

    fn name(&self) -> &'static str {
        // For M2.5 we report just the kind. Production naming is data-driven.
        "Desecration Bone"
    }

    fn valid_rarities(&self) -> crate::currency::RaritySet {
        crate::currency::RaritySet::RARE
    }

    /// Pre-flight class gate (M14.6). Like [`Catalyst::can_apply_to`],
    /// uses the polymorphic `Item.base` interpretation: when the string
    /// matches a known PascalCase class id outside this subtype's valid
    /// list, reject. Real-bundle items with metadata-path bases pass
    /// through and get caught by the registry-backed gate inside `apply()`.
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
        if item.hidden_desecrated.is_some() {
            return Err(CannotApply::Other(
                "item already carries an unrevealed desecrated mod",
            ));
        }
        // Bone size ilvl ceiling (P3): Gnawed bones cannot desecrate items
        // above ilvl 64.
        if let Some(max) = self.size.max_ilvl() {
            if item.ilvl > max {
                return Err(CannotApply::Other(
                    "Gnawed bone cannot desecrate items above item level 64",
                ));
            }
        }
        // Best-effort class check routed through the shared resolver
        // (registry-less here, so only a legacy PascalCase placeholder
        // base resolves). An unresolvable base (e.g., a metadata path)
        // passes through — `apply()` does the registry-backed check.
        if let Some(class) = crate::base_registry::EMPTY.resolve_item_class_opt(item) {
            if !self.subtype.valid_classes().contains(&class.as_str()) {
                return Err(CannotApply::Other(
                    "bone subtype is not valid on this item class",
                ));
            }
        }
        Ok(())
    }

    fn apply(&self, item: &mut Item, ctx: &mut ApplyContext<'_>) -> EngineResult<ApplyOutcome> {
        self.pre_apply_checks(item, ctx.patch)?;

        // Registry-backed class gate (M14.6).
        let class = ctx.base_registry.resolve_item_class(item);
        let valid_classes = self.subtype.valid_classes();
        if !valid_classes.contains(&class.as_str()) {
            return Err(EngineError::InvalidApplication(format!(
                "Bone {:?}: cannot apply to class {} — valid classes are {}",
                self.subtype,
                class,
                valid_classes.join(", ")
            )));
        }

        // Pick affix slot.
        // Sinistral / Dextral Necromancy force the affix.
        // Lord-targeting omens (Blackblooded / Liege / Sovereign) tag the
        //   slot so the reveal pool is restricted to that lord's mods.
        let forced_affix = ctx.omens.consume_affix_only(ctx.patch);
        let lord = ctx.omens.consume_lord_target(ctx.patch);

        // Lord-pool restrictions (M14.6):
        //  - Cranium → Jewel uses the `Lightless` / `of the Abyss` pool;
        //    lord-named omens are illegal on jewels.
        //  - Sceptres have no exclusive desecrated; lord-named omens are
        //    pure waste on them.
        if let Some(lord_value) = lord {
            if !self.subtype.supports_lord_pool() {
                return Err(EngineError::InvalidApplication(format!(
                    "Bone {:?}: lord-targeting omen ({lord_value:?}) is not valid on this subtype's pool",
                    self.subtype,
                )));
            }
            if SCEPTRE_CLASSES_NO_EXCLUSIVE_DESECRATED.contains(&class.as_str()) {
                return Err(EngineError::InvalidApplication(format!(
                    "Bone: lord-targeting omen ({lord_value:?}) is invalid on {class} (no exclusive desecrated mods)"
                )));
            }
        }

        let (pool_prefix, pool_suffix) = self.desecratable_sides(&class, ctx)?;
        free_pool_side_slot(item, pool_prefix, pool_suffix, ctx)?;
        let prefix_open = item.prefixes.len() < 3 && pool_prefix;
        let suffix_open = item.suffixes.len() < 3 && pool_suffix;
        let affix = match forced_affix {
            Some(AffixType::Prefix) if prefix_open => AffixType::Prefix,
            Some(AffixType::Prefix) => {
                return Err(EngineError::AffixSlotFull {
                    affix_type: "Sinistral Necromancy: prefix slots are full",
                });
            }
            Some(AffixType::Suffix) if suffix_open => AffixType::Suffix,
            Some(AffixType::Suffix) => {
                return Err(EngineError::AffixSlotFull {
                    affix_type: "Dextral Necromancy: suffix slots are full",
                });
            }
            Some(_) => {
                return Err(EngineError::AffixSlotFull {
                    affix_type: "non-prefix/suffix omen affix for Bone",
                });
            }
            None => match (prefix_open, suffix_open) {
                (true, true) => {
                    if ctx.rng.gen::<bool>() {
                        AffixType::Prefix
                    } else {
                        AffixType::Suffix
                    }
                }
                (true, false) => AffixType::Prefix,
                (false, true) => AffixType::Suffix,
                (false, false) => {
                    return Err(EngineError::AffixSlotFull {
                        affix_type: "no open prefix/suffix slot for Bone",
                    });
                }
            },
        };

        // Effective Minimum Modifier Level for the reveal (P3):
        //   - Ancient bones guarantee >= 40.
        //   - A lord-targeting omen **bricks** that floor: the wiki notes
        //     "using this omen will brick the minimum modifier level effect
        //     of an Ancient Jawbone / Ancient Collarbone", allowing low-level
        //     mods to be generated on reveal. So when a lord omen is consumed,
        //     the floor drops to 0.
        let min_mod_level = if lord.is_some() {
            0
        } else {
            self.size.min_mod_level()
        };

        item.hidden_desecrated = Some(HiddenDesecratedSlot {
            affix_type: affix,
            bone_size: self.size,
            bone_subtype: self.subtype,
            abyss_lord: lord,
            min_mod_level,
            otherworldly: self.size == BoneSize::Altered,
        });
        Ok(())
    }
}

// =========================================================================
// Reveal at the Well of Souls
// =========================================================================

/// Reveal options offered to the player at the Well of Souls.
///
/// Without omens, exactly 3 options are sampled. With Omen of Abyssal Echoes
/// (M2.6), a second set of 3 can be rerolled and the player picks from
/// either set — modeled by re-running [`sample_reveal_options`] with a
/// different RNG draw and presenting the union to the caller.
pub type RevealOptions = SmallVec<[ModId; 6]>;

/// Sample `n` desecrated mod options from `pool`, filtered by the bone's
/// affix-type, abyss lord, and ilvl floor. Caller consumes the options
/// (typically via `reveal_at_well_of_souls`).
pub fn sample_reveal_options(
    item: &Item,
    pool: &[ModDefinition],
    n: usize,
    rng: &mut dyn rand::RngCore,
) -> RevealOptions {
    let Some(hidden) = item.hidden_desecrated.as_ref() else {
        return SmallVec::new();
    };

    // Candidate filter (P3):
    //  - desecrated, matching the hidden slot's affix.
    //  - rollable at the item's ilvl (`required_level <= ilvl`).
    //  - at or above the slot's effective Minimum Modifier Level floor
    //    (Ancient = 40, bricked to 0 by a lord omen at apply time).
    //  - keep-≥1-tier exception: if a lord omen was active and no tier in a
    //    mod-group clears the floor, the floor was bricked to 0 anyway, so
    //    this naturally allows low-level mods. We don't reapply the
    //    per-group exception here because the floor is the slot's already-
    //    resolved `min_mod_level`, not a currency-variant floor.
    let candidates: Vec<&ModDefinition> = pool
        .iter()
        .filter(|m| {
            m.kind == ModKind::Desecrated
                && m.affix_type == hidden.affix_type
                && m.required_level <= item.ilvl
                && m.required_level >= hidden.min_mod_level
                // Otherworldly mods surface only from an Altered Collarbone
                // slot ("a chance for otherworldly modifiers" — the pool
                // then mixes regular desecrated + Otherworldly options).
                && (hidden.otherworldly
                    || !m.flags.contains(crate::mods::ModFlags::OTHERWORLDLY))
        })
        .collect();

    if candidates.is_empty() {
        return SmallVec::new();
    }

    let take = n.min(candidates.len());
    candidates
        .choose_multiple(rng, take)
        .map(|m| m.id.clone())
        .collect()
}

/// Commit a reveal: caller-chosen `mod_id` (one of the options previously
/// returned by [`sample_reveal_options`]) becomes a real `ModRoll` on the
/// item, replacing the hidden slot. Values are rolled per the mod's stats.
pub fn reveal_at_well_of_souls(
    item: &mut Item,
    pool: &[ModDefinition],
    chosen: &ModId,
    rng: &mut dyn rand::RngCore,
) -> EngineResult<()> {
    let Some(hidden) = item.hidden_desecrated.as_ref() else {
        return Err(EngineError::InvalidApplication(
            "reveal: no hidden desecrated mod to reveal".into(),
        ));
    };
    let chosen_def = pool.iter().find(|m| &m.id == chosen).ok_or_else(|| {
        EngineError::InvalidApplication(format!(
            "reveal: chosen mod `{chosen}` is not in the desecrated pool"
        ))
    })?;
    if chosen_def.kind != ModKind::Desecrated {
        return Err(EngineError::InvalidApplication(format!(
            "reveal: chosen mod `{chosen}` is not a desecrated mod"
        )));
    }
    if chosen_def.affix_type != hidden.affix_type {
        let expected = hidden.affix_type;
        let got = chosen_def.affix_type;
        return Err(EngineError::InvalidApplication(format!(
            "reveal: chosen mod's affix ({got:?}) doesn't match hidden slot's affix ({expected:?})"
        )));
    }

    let values = chosen_def
        .stats
        .iter()
        .map(|s| s.roll(rng.gen::<f64>()))
        .collect::<SmallVec<_>>();

    let roll = ModRoll {
        mod_id: chosen.clone(),
        affix_type: hidden.affix_type,
        kind: ModKind::Desecrated,
        values,
        is_fractured: false,
    };

    let affix = hidden.affix_type;
    item.hidden_desecrated = None;
    match affix {
        AffixType::Prefix => item.prefixes.push(roll),
        AffixType::Suffix => item.suffixes.push(roll),
        _ => {
            return Err(EngineError::InvalidApplication(
                "reveal: hidden slot affix is not Prefix or Suffix".into(),
            ));
        }
    }
    Ok(())
}

#[allow(dead_code)] // wired into omen apply paths in M2.6
pub(crate) fn set_abyss_lord(item: &mut Item, lord: AbyssLord) -> EngineResult<()> {
    let Some(slot) = item.hidden_desecrated.as_mut() else {
        return Err(EngineError::InvalidApplication(
            "no hidden desecrated mod to assign abyss lord to".into(),
        ));
    };
    slot.abyss_lord = Some(lord);
    Ok(())
}

#[cfg(test)]
mod tests {
    use rand::SeedableRng;
    use rand_xoshiro::Xoshiro256PlusPlus;
    use smallvec::smallvec;

    use super::*;
    use crate::ids::{ItemClassId, ModGroupId, StatId, TagId};
    use crate::item::{ModRoll, QualityKind};
    use crate::mods::{ModDomain, ModFlags, ModGroup, ModStat, SpawnWeight};
    use crate::patch::{PatchRange, PatchVersion};
    use crate::registry::ModRegistry;

    fn fixture_rare_armour() -> Item {
        Item {
            base: ItemClassId::from("BodyArmour").as_str().into(),
            ilvl: 82,
            rarity: Rarity::Rare,
            corrupted: false,
            sanctified: false,
            mirrored: false,
            quality: 0,
            quality_kind: QualityKind::Untagged,
            implicits: smallvec![],
            prefixes: smallvec![ModRoll {
                mod_id: ModId::from("ES1"),
                affix_type: AffixType::Prefix,
                kind: ModKind::Explicit,
                values: smallvec![],
                is_fractured: false,
            }],
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
        omens: &'a mut crate::omen::OmenSet,
    ) -> ApplyContext<'a> {
        ApplyContext::new_without_bases(reg, rng, PatchVersion::PATCH_0_4_0, omens)
    }

    fn desecrated_mod(id: &str, affix: AffixType, group: &str) -> ModDefinition {
        ModDefinition {
            id: ModId::from(id),
            name: None,
            mod_group: ModGroup(ModGroupId::from(group)),
            affix_type: affix,
            kind: ModKind::Desecrated,
            domain: ModDomain::Item,
            tags: smallvec![],
            concept_set: smallvec![],
            spawn_weights: smallvec![SpawnWeight {
                tag: TagId::from("BodyArmour"),
                weight: 1,
            }],
            stats: smallvec![ModStat {
                stat_id: StatId::from("test_stat"),
                min: 1.0,
                max: 10.0,
            }],
            required_level: 1,
            tier: None,
            allowed_item_classes: smallvec![ItemClassId::from("BodyArmour")],
            patch_range: PatchRange::ALL,
            flags: ModFlags::DESECRATED_ONLY,
            text_template: None,
        }
    }

    #[test]
    fn bone_adds_hidden_desecrated_slot() {
        let reg = ModRegistry::from_mods(vec![], vec![]);
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(1);
        let mut omens = crate::omen::OmenSet::new();
        let mut item = fixture_rare_armour();
        Bone::new(BoneSize::Preserved, BoneSubtype::Rib)
            .apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens))
            .unwrap();
        assert!(item.hidden_desecrated.is_some());
        let h = item.hidden_desecrated.as_ref().unwrap();
        assert_eq!(h.bone_size, BoneSize::Preserved);
        assert_eq!(h.bone_subtype, BoneSubtype::Rib);
        // Visible mod count unchanged; hidden adds a slot.
        assert_eq!(item.prefixes.len() + item.suffixes.len(), 1);
        assert_eq!(item.fracturing_eligibility_count(), 2);
    }

    #[test]
    fn bone_rejects_when_already_has_hidden() {
        let reg = ModRegistry::from_mods(vec![], vec![]);
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(2);
        let mut omens = crate::omen::OmenSet::new();
        let mut item = fixture_rare_armour();
        Bone::new(BoneSize::Preserved, BoneSubtype::Rib)
            .apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens))
            .unwrap();
        let r = Bone::new(BoneSize::Ancient, BoneSubtype::Rib)
            .apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens));
        assert!(matches!(r, Err(EngineError::InvalidApplication(_))));
    }

    #[test]
    fn bone_can_apply_to_gates_legacy_class_ids_but_fails_open_on_metadata_paths() {
        let bone = Bone::new(BoneSize::Preserved, BoneSubtype::Rib);
        // Legacy placeholder base in the Rib valid list → accepted.
        assert!(bone.can_apply_to(&fixture_rare_armour()).is_ok());
        // Recognisable class id outside the valid list → hard rejection.
        let mut ring = fixture_rare_armour();
        ring.base = ItemClassId::from("Ring").as_str().into();
        assert!(matches!(
            bone.can_apply_to(&ring),
            Err(CannotApply::Other(_))
        ));
        // Unresolvable metadata-path base → fail open; the registry-backed
        // gate inside `apply()` decides.
        let mut bundled = fixture_rare_armour();
        bundled.base = "Metadata/Items/Armours/BodyArmours/FourBodyInt3".into();
        assert!(bone.can_apply_to(&bundled).is_ok());
    }

    #[test]
    fn bone_rejects_non_rare() {
        let reg = ModRegistry::from_mods(vec![], vec![]);
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(3);
        let mut omens = crate::omen::OmenSet::new();
        let mut item = fixture_rare_armour();
        item.rarity = Rarity::Magic;
        let r = Bone::new(BoneSize::Preserved, BoneSubtype::Rib)
            .apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens));
        assert!(matches!(r, Err(EngineError::InvalidApplication(_))));
    }

    /// poe2db Desecrated_Modifiers: "If modifiers are full then a random
    /// modifier is also removed." A 3+3 rare accepts the bone after the
    /// engine frees one slot.
    #[test]
    fn bone_on_full_item_removes_a_mod_then_desecrates() {
        let reg = ModRegistry::from_mods(vec![], vec![]);
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(4);
        let mut omens = crate::omen::OmenSet::new();
        let mut item = fixture_rare_armour();
        // Fill all 3+3 slots.
        item.prefixes = smallvec![
            ModRoll {
                mod_id: ModId::from("P1"),
                affix_type: AffixType::Prefix,
                kind: ModKind::Explicit,
                values: smallvec![],
                is_fractured: false,
            };
            3
        ];
        item.suffixes = smallvec![
            ModRoll {
                mod_id: ModId::from("S1"),
                affix_type: AffixType::Suffix,
                kind: ModKind::Explicit,
                values: smallvec![],
                is_fractured: false,
            };
            3
        ];
        let r = Bone::new(BoneSize::Preserved, BoneSubtype::Rib)
            .apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens));
        assert!(r.is_ok(), "full item must accept the bone: {r:?}");
        assert_eq!(
            item.prefixes.len() + item.suffixes.len(),
            5,
            "exactly one mod removed to free the slot"
        );
        assert!(item.hidden_desecrated.is_some());

        // All-fractured full item: nothing removable → reject.
        let mut locked = fixture_rare_armour();
        locked.prefixes = smallvec![
            ModRoll {
                mod_id: ModId::from("P1"),
                affix_type: AffixType::Prefix,
                kind: ModKind::Explicit,
                values: smallvec![],
                is_fractured: true,
            };
            3
        ];
        locked.suffixes = smallvec![
            ModRoll {
                mod_id: ModId::from("S1"),
                affix_type: AffixType::Suffix,
                kind: ModKind::Explicit,
                values: smallvec![],
                is_fractured: true,
            };
            3
        ];
        let r = Bone::new(BoneSize::Preserved, BoneSubtype::Rib)
            .apply(&mut locked, &mut ctx(&reg, &mut rng, &mut omens));
        assert!(matches!(r, Err(EngineError::InvalidApplication(_))));
    }

    /// Size × subtype gating: Cranium exists only as Preserved; Altered
    /// only as Collarbone (poe2db Desecrated_Modifiers item list).
    #[test]
    fn nonexistent_size_subtype_combos_reject() {
        let reg = ModRegistry::from_mods(vec![], vec![]);
        let mut omens = crate::omen::OmenSet::new();
        for (size, subtype) in [
            (BoneSize::Gnawed, BoneSubtype::Cranium),
            (BoneSize::Ancient, BoneSubtype::Cranium),
            (BoneSize::Altered, BoneSubtype::Rib),
            (BoneSize::Altered, BoneSubtype::Jawbone),
            (BoneSize::Altered, BoneSubtype::Cranium),
        ] {
            let mut rng = Xoshiro256PlusPlus::seed_from_u64(7);
            let mut item = fixture_rare_armour();
            item.ilvl = 60; // below the Gnawed ceiling so only validity gates
            let r = Bone::new(size, subtype).apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens));
            assert!(
                matches!(r, Err(EngineError::InvalidApplication(ref m)) if m.contains("does not exist")),
                "{size:?} {subtype:?} must reject; got {r:?}"
            );
        }
    }

    /// Altered Collarbone marks the hidden slot otherworldly; reveal then
    /// offers OTHERWORLDLY-flagged mods, which regular bones never surface.
    #[test]
    fn altered_collarbone_unlocks_otherworldly_reveals() {
        use crate::mods::ModFlags;
        let reg = ModRegistry::from_mods(vec![], vec![]);
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(9);
        let mut omens = crate::omen::OmenSet::new();

        let mk = |id: &str, flags: ModFlags| {
            let mut m = desecrated_mod(id, AffixType::Prefix, id);
            m.flags = flags;
            m
        };
        let pool = vec![
            mk("RegularDesecrated", ModFlags::DESECRATED_ONLY),
            mk(
                "OtherworldlyMod",
                ModFlags::DESECRATED_ONLY | ModFlags::OTHERWORLDLY,
            ),
        ];

        // Regular Preserved Collarbone on a rare ring: otherworldly excluded.
        let mut preserved_ring = fixture_rare_armour();
        preserved_ring.base = "Ring".into();
        Bone::new(BoneSize::Preserved, BoneSubtype::Collarbone)
            .apply(&mut preserved_ring, &mut ctx(&reg, &mut rng, &mut omens))
            .unwrap();
        let hidden = preserved_ring.hidden_desecrated.as_ref().unwrap();
        assert!(!hidden.otherworldly);
        let mut pool_affix = pool.clone();
        for m in &mut pool_affix {
            m.affix_type = hidden.affix_type;
        }
        let opts = sample_reveal_options(&preserved_ring, &pool_affix, 3, &mut rng);
        assert!(
            !opts.iter().any(|o| o.as_str() == "OtherworldlyMod"),
            "regular bone reveals must exclude OTHERWORLDLY mods: {opts:?}"
        );

        // Altered Collarbone: otherworldly included.
        let mut altered_ring = fixture_rare_armour();
        altered_ring.base = "Ring".into();
        Bone::new(BoneSize::Altered, BoneSubtype::Collarbone)
            .apply(&mut altered_ring, &mut ctx(&reg, &mut rng, &mut omens))
            .unwrap();
        let hidden2 = altered_ring.hidden_desecrated.as_ref().unwrap();
        assert!(hidden2.otherworldly);
        let mut pool_affix2 = pool.clone();
        for m in &mut pool_affix2 {
            m.affix_type = hidden2.affix_type;
        }
        let opts = sample_reveal_options(&altered_ring, &pool_affix2, 3, &mut rng);
        assert!(
            opts.iter().any(|o| o.as_str() == "OtherworldlyMod"),
            "altered bone reveals must include OTHERWORLDLY mods: {opts:?}"
        );
    }

    #[test]
    fn reveal_offers_three_options_and_filters_by_affix() {
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(5);
        let mut _omens = crate::omen::OmenSet::new();
        let mut item = fixture_rare_armour();
        item.hidden_desecrated = Some(HiddenDesecratedSlot {
            affix_type: AffixType::Suffix,
            bone_size: BoneSize::Preserved,
            bone_subtype: BoneSubtype::Rib,
            abyss_lord: None,
            min_mod_level: 0,
            otherworldly: false,
        });
        let pool = vec![
            // 5 suffix candidates -> sample picks 3
            desecrated_mod("DS_SUF1", AffixType::Suffix, "GA"),
            desecrated_mod("DS_SUF2", AffixType::Suffix, "GB"),
            desecrated_mod("DS_SUF3", AffixType::Suffix, "GC"),
            desecrated_mod("DS_SUF4", AffixType::Suffix, "GD"),
            desecrated_mod("DS_SUF5", AffixType::Suffix, "GE"),
            // Prefix candidates filtered out
            desecrated_mod("DS_PFX1", AffixType::Prefix, "GP"),
        ];
        let opts = sample_reveal_options(&item, &pool, 3, &mut rng);
        assert_eq!(opts.len(), 3);
        for id in &opts {
            assert!(id.as_str().starts_with("DS_SUF"));
        }
    }

    #[test]
    fn reveal_commits_chosen_mod_to_correct_slot() {
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(6);
        let mut _omens = crate::omen::OmenSet::new();
        let mut item = fixture_rare_armour();
        item.hidden_desecrated = Some(HiddenDesecratedSlot {
            affix_type: AffixType::Suffix,
            bone_size: BoneSize::Preserved,
            bone_subtype: BoneSubtype::Rib,
            abyss_lord: None,
            min_mod_level: 0,
            otherworldly: false,
        });
        let pool = vec![desecrated_mod("DS_SUF1", AffixType::Suffix, "GA")];
        let chosen = ModId::from("DS_SUF1");
        reveal_at_well_of_souls(&mut item, &pool, &chosen, &mut rng).unwrap();

        assert!(item.hidden_desecrated.is_none());
        assert_eq!(item.suffixes.len(), 1);
        assert_eq!(item.suffixes[0].mod_id, chosen);
        assert_eq!(item.suffixes[0].kind, ModKind::Desecrated);
        assert!(item.suffixes[0].values[0] >= 1.0);
        assert!(item.suffixes[0].values[0] <= 10.0);
    }

    #[test]
    fn reveal_rejects_chosen_with_wrong_affix() {
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(7);
        let mut _omens = crate::omen::OmenSet::new();
        let mut item = fixture_rare_armour();
        item.hidden_desecrated = Some(HiddenDesecratedSlot {
            affix_type: AffixType::Suffix,
            bone_size: BoneSize::Preserved,
            bone_subtype: BoneSubtype::Rib,
            abyss_lord: None,
            min_mod_level: 0,
            otherworldly: false,
        });
        let pool = vec![desecrated_mod("DS_PFX1", AffixType::Prefix, "GA")];
        let r = reveal_at_well_of_souls(&mut item, &pool, &ModId::from("DS_PFX1"), &mut rng);
        assert!(matches!(r, Err(EngineError::InvalidApplication(_))));
        // Item state should be unchanged.
        assert!(item.hidden_desecrated.is_some());
    }

    #[test]
    fn reveal_rejects_unknown_mod() {
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(8);
        let mut _omens = crate::omen::OmenSet::new();
        let mut item = fixture_rare_armour();
        item.hidden_desecrated = Some(HiddenDesecratedSlot {
            affix_type: AffixType::Suffix,
            bone_size: BoneSize::Preserved,
            bone_subtype: BoneSubtype::Rib,
            abyss_lord: None,
            min_mod_level: 0,
            otherworldly: false,
        });
        let pool: Vec<ModDefinition> = vec![];
        let r = reveal_at_well_of_souls(&mut item, &pool, &ModId::from("GHOST"), &mut rng);
        assert!(matches!(r, Err(EngineError::InvalidApplication(_))));
    }

    #[test]
    fn reveal_rejects_when_no_hidden_slot() {
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(9);
        let mut _omens = crate::omen::OmenSet::new();
        let mut item = fixture_rare_armour();
        let pool = vec![desecrated_mod("DS_SUF1", AffixType::Suffix, "GA")];
        let r = reveal_at_well_of_souls(&mut item, &pool, &ModId::from("DS_SUF1"), &mut rng);
        assert!(matches!(r, Err(EngineError::InvalidApplication(_))));
    }

    #[test]
    fn set_abyss_lord_marks_slot() {
        let mut item = fixture_rare_armour();
        item.hidden_desecrated = Some(HiddenDesecratedSlot {
            affix_type: AffixType::Suffix,
            bone_size: BoneSize::Preserved,
            bone_subtype: BoneSubtype::Jawbone,
            abyss_lord: None,
            min_mod_level: 0,
            otherworldly: false,
        });
        set_abyss_lord(&mut item, AbyssLord::Kurgal).unwrap();
        assert_eq!(
            item.hidden_desecrated.as_ref().unwrap().abyss_lord,
            Some(AbyssLord::Kurgal)
        );
    }
}
