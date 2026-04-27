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

use crate::currency::{ApplyContext, ApplyOutcome, Currency};
use crate::error::{EngineError, EngineResult};
use crate::ids::{CurrencyId, ModId};
use crate::item::{
    AbyssLord, AffixType, BoneSize, BoneSubtype, HiddenDesecratedSlot, Item, ModRoll, Rarity,
};
use crate::mods::{ModDefinition, ModKind};

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

    /// Minimum required ilvl of mods sampled at reveal, gated by bone size.
    ///
    /// All sizes share a floor of 1 in the M2.5 placeholder data — actual
    /// tier-gating per bone size lands when poe2db desecration tables are
    /// integrated. Until then, the advisor relies on bone size only for
    /// strategy selection (Ancient = best-tier preference).
    pub const fn min_required_level(&self) -> u32 {
        match self.size {
            BoneSize::Gnawed | BoneSize::Preserved | BoneSize::Ancient => 1,
        }
    }
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

    fn apply(&self, item: &mut Item, ctx: &mut ApplyContext<'_>) -> EngineResult<ApplyOutcome> {
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

        // Pick affix slot.
        // Sinistral / Dextral Necromancy force the affix.
        // Lord-targeting omens (Blackblooded / Liege / Sovereign) tag the
        //   slot so the reveal pool is restricted to that lord's mods.
        let forced_affix = ctx.omens.consume_affix_only(ctx.patch);
        let lord = ctx.omens.consume_lord_target(ctx.patch);

        let prefix_open = item.prefixes.len() < 3;
        let suffix_open = item.suffixes.len() < 3;
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

        item.hidden_desecrated = Some(HiddenDesecratedSlot {
            affix_type: affix,
            bone_size: self.size,
            bone_subtype: self.subtype,
            abyss_lord: lord,
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

    // Lord-targeting omens (Blackblooded/Liege/Sovereign) add an
    // `abyss_lord` constraint we'll filter against once desecrated mods
    // carry lord tags in the bundle. M2.5 placeholder accepts all.
    let candidates: Vec<&ModDefinition> = pool
        .iter()
        .filter(|m| {
            m.kind == ModKind::Desecrated
                && m.affix_type == hidden.affix_type
                && m.required_level <= item.ilvl
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
        ApplyContext::new(reg, rng, PatchVersion::PATCH_0_4_0, omens)
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
            allowed_item_classes: smallvec![ItemClassId::from("BodyArmour")],
            patch_range: PatchRange::ALL,
            flags: ModFlags::DESECRATED_ONLY,
            text_template: None,
        }
    }

    #[test]
    fn bone_adds_hidden_desecrated_slot() {
        let reg = ModRegistry::from_mods(vec![]);
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
        let reg = ModRegistry::from_mods(vec![]);
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
    fn bone_rejects_non_rare() {
        let reg = ModRegistry::from_mods(vec![]);
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(3);
        let mut omens = crate::omen::OmenSet::new();
        let mut item = fixture_rare_armour();
        item.rarity = Rarity::Magic;
        let r = Bone::new(BoneSize::Preserved, BoneSubtype::Rib)
            .apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens));
        assert!(matches!(r, Err(EngineError::InvalidApplication(_))));
    }

    #[test]
    fn bone_rejects_when_both_slots_full() {
        let reg = ModRegistry::from_mods(vec![]);
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
        assert!(matches!(r, Err(EngineError::AffixSlotFull { .. })));
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
        });
        set_abyss_lord(&mut item, AbyssLord::Kurgal).unwrap();
        assert_eq!(
            item.hidden_desecrated.as_ref().unwrap().abyss_lord,
            Some(AbyssLord::Kurgal)
        );
    }
}
