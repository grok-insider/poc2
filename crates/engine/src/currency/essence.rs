//! Essences (Lesser / Normal / Greater / Perfect / Corrupted).
//!
//! ## Behavior
//!
//! - **Lesser / Normal / Greater** essences apply to a **Magic** item:
//!   the item is promoted to Rare and gains the essence's specific mod
//!   (plus the existing magic mods are preserved, just like a Regal Orb
//!   but with a guaranteed mod).
//!
//! - **Perfect / Corrupted** essences apply to a **Rare** item: a random
//!   non-fractured mod is removed, then the essence's specific mod is
//!   added. Sinistral / Dextral Crystallisation force the *removal* to
//!   target only Prefix or only Suffix.
//!
//! ## Determinism
//!
//! The essence carries the exact `ModId` it adds (the data binding lives
//! in the bundle, populated by the poe2db pipeline pass once data is
//! integrated). For the M2.5 milestone, callers construct an essence with
//! a [`ModId`] they've looked up themselves; the engine just enforces the
//! mechanic.

use rand::Rng;

use crate::currency::{ApplyContext, ApplyOutcome, Currency};
use crate::error::{EngineError, EngineResult};
use crate::ids::{CurrencyId, ModId};
use crate::item::{AffixType, Item, ModRoll, Rarity};
use crate::mods::{ModDefinition, ModKind};

/// Quality tier of an essence — controls its apply behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EssenceQuality {
    Lesser,
    Normal,
    Greater,
    /// Operates on Rare; removes one random mod and adds the essence's mod.
    Perfect,
    /// Vaal-corrupted Perfect essence; same removal+add semantics but the
    /// added mod comes from the corrupted pool. Operates on Rare even when
    /// the item is already corrupted.
    Corrupted,
}

impl EssenceQuality {
    /// Does this essence variant promote Magic to Rare (the regal-style
    /// path)? True for Lesser/Normal/Greater.
    pub fn is_promoting(self) -> bool {
        matches!(self, Self::Lesser | Self::Normal | Self::Greater)
    }
    /// Does this essence variant remove + add on a Rare? True for
    /// Perfect/Corrupted.
    pub fn is_remove_add(self) -> bool {
        matches!(self, Self::Perfect | Self::Corrupted)
    }
}

/// One essence — characterized by quality and the specific mod it grants.
///
/// In production, the full Essence catalogue (19 types × 4 tiers + Corrupted)
/// ships in the data bundle; this engine type is the currency-trait wrapper
/// that consumes a bundle entry at apply time.
#[derive(Debug, Clone)]
pub struct Essence {
    /// Display id, e.g. `"PerfectEssenceOfSeeking"` or
    /// `"GreaterEssenceOfBattle"`.
    pub id: CurrencyId,
    /// Display name, e.g. `"Perfect Essence of Seeking"`.
    pub display_name: &'static str,
    /// Quality tier (drives apply semantics).
    pub quality: EssenceQuality,
    /// The mod this essence guarantees to add. The mod's `affix_type`
    /// dictates which slot it occupies; Crystallisation's affix-only filter
    /// is independent of this and applies to the *removal* step on
    /// Perfect/Corrupted essences.
    pub target_mod: ModId,
}

impl Essence {
    pub fn new(
        id: impl Into<CurrencyId>,
        display_name: &'static str,
        quality: EssenceQuality,
        target_mod: impl Into<ModId>,
    ) -> Self {
        Self {
            id: id.into(),
            display_name,
            quality,
            target_mod: target_mod.into(),
        }
    }
}

impl Currency for Essence {
    fn id(&self) -> &CurrencyId {
        &self.id
    }

    fn name(&self) -> &'static str {
        self.display_name
    }

    fn valid_rarities(&self) -> crate::currency::RaritySet {
        match self.quality {
            EssenceQuality::Lesser | EssenceQuality::Normal => crate::currency::RaritySet::NORMAL,
            EssenceQuality::Greater => crate::currency::RaritySet::MAGIC,
            EssenceQuality::Perfect | EssenceQuality::Corrupted => crate::currency::RaritySet::RARE,
        }
    }

    fn apply(&self, item: &mut Item, ctx: &mut ApplyContext<'_>) -> EngineResult<ApplyOutcome> {
        // Sanctified items reject all essences.
        if item.sanctified {
            return Err(EngineError::ItemSanctified);
        }
        // Mirrored items reject everything.
        if item.mirrored {
            return Err(EngineError::InvalidApplication(
                "Essence cannot be applied to a mirrored item".into(),
            ));
        }
        // Corrupted items: only Corrupted essences are allowed.
        if item.corrupted && self.quality != EssenceQuality::Corrupted {
            return Err(EngineError::ItemCorrupted);
        }

        let target_def = ctx.registry.get(&self.target_mod).ok_or_else(|| {
            EngineError::Data(format!(
                "Essence: target mod `{}` not in registry",
                self.target_mod
            ))
        })?;

        if self.quality.is_promoting() {
            apply_promoting(self, item, ctx, target_def)
        } else {
            apply_remove_add(self, item, ctx, target_def)
        }
    }
}

/// Lesser/Normal/Greater path: Magic → Rare with the specific mod added,
/// existing mods preserved. Like a Regal Orb but with a guaranteed mod.
fn apply_promoting(
    essence: &Essence,
    item: &mut Item,
    ctx: &mut ApplyContext<'_>,
    target_def: &ModDefinition,
) -> EngineResult<ApplyOutcome> {
    if item.rarity != Rarity::Magic {
        return Err(EngineError::InvalidApplication(format!(
            "{}: requires a Magic-rarity item",
            essence.display_name
        )));
    }
    // Refuse if the slot of the target mod's affix type is already full.
    let slot_full = match target_def.affix_type {
        AffixType::Prefix => item.prefixes.len() >= 3,
        AffixType::Suffix => item.suffixes.len() >= 3,
        _ => true,
    };
    if slot_full {
        return Err(EngineError::AffixSlotFull {
            affix_type: "Essence's target affix slot is full",
        });
    }
    // Refuse if the target mod's group is already occupied.
    if let Some(g) = ctx.registry.group_of(&essence.target_mod) {
        let occupied = item
            .prefixes
            .iter()
            .chain(item.suffixes.iter())
            .any(|m| ctx.registry.group_of(&m.mod_id) == Some(g));
        if occupied {
            return Err(EngineError::ModGroupExclusive(format!(
                "Essence's target mod-group `{g}` is already on the item"
            )));
        }
    }

    item.rarity = Rarity::Rare;
    push_essence_roll(item, essence, target_def, ctx);
    Ok(())
}

/// Perfect/Corrupted path: remove one random non-fractured mod, then add
/// the essence's specific mod. Sinistral/Dextral Crystallisation constrain
/// which affix the removal targets.
fn apply_remove_add(
    essence: &Essence,
    item: &mut Item,
    ctx: &mut ApplyContext<'_>,
    target_def: &ModDefinition,
) -> EngineResult<ApplyOutcome> {
    if item.rarity != Rarity::Rare {
        return Err(EngineError::InvalidApplication(format!(
            "{}: requires a Rare-rarity item",
            essence.display_name
        )));
    }

    let affix_filter = ctx.omens.consume_affix_only(ctx.patch);

    // Build removable list per filter.
    let mut removables: smallvec::SmallVec<[(AffixType, usize); 8]> = smallvec::SmallVec::new();
    if affix_filter != Some(AffixType::Suffix) {
        for (i, m) in item.prefixes.iter().enumerate() {
            if !m.is_fractured {
                removables.push((AffixType::Prefix, i));
            }
        }
    }
    if affix_filter != Some(AffixType::Prefix) {
        for (i, m) in item.suffixes.iter().enumerate() {
            if !m.is_fractured {
                removables.push((AffixType::Suffix, i));
            }
        }
    }
    if removables.is_empty() {
        return Err(EngineError::InvalidApplication(format!(
            "{}: no eligible mod to remove given Crystallisation/fractures",
            essence.display_name
        )));
    }

    // Mod-group exclusivity: target_def's group must not collide with a
    // surviving mod. If it would, refuse upfront. (Real game behavior:
    // the engine already removed a mod, so the slot is open. We mirror
    // that by removing FIRST then checking.)
    let pick = ctx.rng.gen_range(0..removables.len());
    let (rm_affix, rm_idx) = removables[pick];
    let _removed = match rm_affix {
        AffixType::Prefix => item.prefixes.remove(rm_idx),
        AffixType::Suffix => item.suffixes.remove(rm_idx),
        _ => unreachable!(),
    };

    // Now check group exclusivity against survivors.
    if let Some(g) = ctx.registry.group_of(&essence.target_mod) {
        let occupied = item
            .prefixes
            .iter()
            .chain(item.suffixes.iter())
            .any(|m| ctx.registry.group_of(&m.mod_id) == Some(g));
        if occupied {
            return Err(EngineError::ModGroupExclusive(format!(
                "Essence's target mod-group `{g}` already on item after removal"
            )));
        }
    }

    push_essence_roll(item, essence, target_def, ctx);
    Ok(())
}

fn push_essence_roll(
    item: &mut Item,
    essence: &Essence,
    target_def: &ModDefinition,
    ctx: &mut ApplyContext<'_>,
) {
    let kind = match essence.quality {
        EssenceQuality::Corrupted => ModKind::Corrupted,
        _ => ModKind::Explicit,
    };
    let values = target_def
        .stats
        .iter()
        .map(|s| s.roll(ctx.rng.gen::<f64>()))
        .collect();
    let roll = ModRoll {
        mod_id: essence.target_mod.clone(),
        affix_type: target_def.affix_type,
        kind,
        values,
        is_fractured: false,
    };
    match target_def.affix_type {
        AffixType::Prefix => item.prefixes.push(roll),
        AffixType::Suffix => item.suffixes.push(roll),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use rand::SeedableRng;
    use rand_xoshiro::Xoshiro256PlusPlus;
    use smallvec::smallvec;

    use super::*;
    use crate::ids::{ItemClassId, ModGroupId, StatId, TagId};
    use crate::item::QualityKind;
    use crate::mods::{ModDomain, ModFlags, ModGroup, ModStat, SpawnWeight};
    use crate::omen::{Omen, OmenSet};
    use crate::patch::{PatchRange, PatchVersion};
    use crate::registry::ModRegistry;

    fn mk_target_mod(id: &str, group: &str, affix: AffixType) -> ModDefinition {
        ModDefinition {
            id: ModId::from(id),
            name: None,
            mod_group: ModGroup(ModGroupId::from(group)),
            affix_type: affix,
            kind: ModKind::Explicit,
            domain: ModDomain::Item,
            tags: smallvec![],
            concept_set: smallvec![],
            spawn_weights: smallvec![SpawnWeight {
                tag: TagId::from("BodyArmour"),
                weight: 1
            }],
            stats: smallvec![ModStat {
                stat_id: StatId::from("test_stat"),
                min: 40.0,
                max: 50.0,
            }],
            required_level: 1,
            allowed_item_classes: smallvec![ItemClassId::from("BodyArmour")],
            patch_range: PatchRange::ALL,
            flags: ModFlags::ESSENCE_ONLY,
            text_template: None,
        }
    }

    fn fixture_armour() -> Item {
        Item {
            base: ItemClassId::from("BodyArmour").as_str().into(),
            ilvl: 82,
            rarity: Rarity::Magic,
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

    #[test]
    fn greater_essence_promotes_magic_to_rare_with_target_mod() {
        let target = mk_target_mod("EssMod_Life_Greater", "Life", AffixType::Prefix);
        let reg = ModRegistry::from_mods(vec![target]);
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0x1);
        let mut omens = OmenSet::new();

        let mut item = fixture_armour();
        // Existing Magic mod (suffix) should be preserved.
        item.suffixes.push(ModRoll {
            mod_id: ModId::from("ExistingSuffix"),
            affix_type: AffixType::Suffix,
            kind: ModKind::Explicit,
            values: smallvec![],
            is_fractured: false,
        });

        let ess = Essence::new(
            "GreaterEssenceOfBody",
            "Greater Essence of Body",
            EssenceQuality::Greater,
            "EssMod_Life_Greater",
        );

        ess.apply(
            &mut item,
            &mut ApplyContext::new(&reg, &mut rng, PatchVersion::PATCH_0_4_0, &mut omens),
        )
        .unwrap();

        assert_eq!(item.rarity, Rarity::Rare);
        assert_eq!(item.prefixes.len(), 1);
        assert_eq!(item.prefixes[0].mod_id, ModId::from("EssMod_Life_Greater"));
        assert_eq!(item.suffixes.len(), 1, "existing suffix preserved");
    }

    #[test]
    fn perfect_essence_with_dextral_crystallisation_removes_only_suffix() {
        // The user's worked example: Perfect Essence of Seeking +
        // Omen of Dextral Crystallisation = removes a suffix, adds Seeking
        // (Body Armour: reduced Critical Damage Bonus).
        let target = mk_target_mod(
            "EssMod_Seeking_Perfect",
            "ReducedCritDmg",
            AffixType::Suffix,
        );
        let life = mk_target_mod("LifeMod", "Life", AffixType::Prefix);
        let res = mk_target_mod("FireResMod", "FireRes", AffixType::Suffix);
        let reg = ModRegistry::from_mods(vec![target, life, res]);
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0x2);
        let mut omens = OmenSet::new();
        omens.push(Omen::dextral_crystallisation());

        let mut item = fixture_armour();
        item.rarity = Rarity::Rare;
        // 2 prefixes + 1 suffix
        item.prefixes.push(ModRoll {
            mod_id: ModId::from("LifeMod"),
            affix_type: AffixType::Prefix,
            kind: ModKind::Explicit,
            values: smallvec![],
            is_fractured: false,
        });
        item.prefixes.push(ModRoll {
            mod_id: ModId::from("ESMod"),
            affix_type: AffixType::Prefix,
            kind: ModKind::Explicit,
            values: smallvec![],
            is_fractured: false,
        });
        item.suffixes.push(ModRoll {
            mod_id: ModId::from("FireResMod"),
            affix_type: AffixType::Suffix,
            kind: ModKind::Explicit,
            values: smallvec![],
            is_fractured: false,
        });

        let ess = Essence::new(
            "PerfectEssenceOfSeeking",
            "Perfect Essence of Seeking",
            EssenceQuality::Perfect,
            "EssMod_Seeking_Perfect",
        );

        ess.apply(
            &mut item,
            &mut ApplyContext::new(&reg, &mut rng, PatchVersion::PATCH_0_4_0, &mut omens),
        )
        .unwrap();

        // Both prefixes survive (Crystallisation forced suffix removal).
        assert_eq!(item.prefixes.len(), 2);
        // The original suffix was removed; the essence-added suffix replaces it.
        assert_eq!(item.suffixes.len(), 1);
        assert_eq!(
            item.suffixes[0].mod_id,
            ModId::from("EssMod_Seeking_Perfect")
        );
    }

    #[test]
    fn perfect_essence_rejects_when_no_removable_mods() {
        // Crystallisation forces suffix removal but there are no suffixes.
        let target = mk_target_mod("EssMod_X", "GA", AffixType::Suffix);
        let reg = ModRegistry::from_mods(vec![target]);
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0x3);
        let mut omens = OmenSet::new();
        omens.push(Omen::dextral_crystallisation());

        let mut item = fixture_armour();
        item.rarity = Rarity::Rare;
        item.prefixes.push(ModRoll {
            mod_id: ModId::from("LifeMod"),
            affix_type: AffixType::Prefix,
            kind: ModKind::Explicit,
            values: smallvec![],
            is_fractured: false,
        });

        let ess = Essence::new(
            "PerfectEssenceOfX",
            "Perfect X",
            EssenceQuality::Perfect,
            "EssMod_X",
        );
        let r = ess.apply(
            &mut item,
            &mut ApplyContext::new(&reg, &mut rng, PatchVersion::PATCH_0_4_0, &mut omens),
        );
        assert!(matches!(r, Err(EngineError::InvalidApplication(_))));
    }

    #[test]
    fn essence_rejects_corrupted_unless_corrupted_essence() {
        let target = mk_target_mod("EssMod_X", "GA", AffixType::Suffix);
        let reg = ModRegistry::from_mods(vec![target]);
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0x4);
        let mut omens = OmenSet::new();

        let mut item = fixture_armour();
        item.rarity = Rarity::Rare;
        item.corrupted = true;

        let perfect = Essence::new("X", "X", EssenceQuality::Perfect, "EssMod_X");
        let r = perfect.apply(
            &mut item,
            &mut ApplyContext::new(&reg, &mut rng, PatchVersion::PATCH_0_4_0, &mut omens),
        );
        assert!(matches!(r, Err(EngineError::ItemCorrupted)));

        // Corrupted essence accepts corrupted item (assuming a removable mod
        // exists; we add one).
        item.suffixes.push(ModRoll {
            mod_id: ModId::from("OldSuffix"),
            affix_type: AffixType::Suffix,
            kind: ModKind::Explicit,
            values: smallvec![],
            is_fractured: false,
        });
        let corrupted = Essence::new("X", "X", EssenceQuality::Corrupted, "EssMod_X");
        corrupted
            .apply(
                &mut item,
                &mut ApplyContext::new(&reg, &mut rng, PatchVersion::PATCH_0_4_0, &mut omens),
            )
            .unwrap();
        assert_eq!(item.suffixes.len(), 1);
        assert_eq!(item.suffixes[0].kind, ModKind::Corrupted);
    }
}
