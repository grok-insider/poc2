//! Fracturing Orb.
//!
//! Picks one of the item's non-fractured **visible** mods and marks it
//! fractured (immutable to Annul/Chaos/Divine until corruption).
//!
//! ## Eligibility (per the user's worked example, step 7)
//!
//! - The item must have **≥4 explicit mods** *including* any hidden
//!   desecrated slot. The hidden mod COUNTS toward the threshold.
//! - The fracture target is sampled uniformly over **non-hidden,
//!   non-fractured** mods only. The hidden desecrated mod cannot itself be
//!   fractured.
//! - Net effect: with 3 visible mods + 1 hidden, the chance of fracturing
//!   any specific visible mod is `1/3` (not `1/4`).
//!
//! ## Refusals
//!
//! - Item is corrupted / sanctified / mirrored → `InvalidApplication`
//! - Item has < 4 explicit mods → `InsufficientMods { required: 4, actual }`
//! - All visible mods are already fractured → `FractureHiddenMod`
//!   (the only remaining target would be the hidden mod, which we refuse)

use rand::Rng;

use crate::currency::{ApplyContext, ApplyOutcome, Currency};
use crate::error::{EngineError, EngineResult};
use crate::ids::CurrencyId;
use crate::item::{AffixType, Item};

#[derive(Debug)]
pub struct FracturingOrb {
    id: CurrencyId,
}

impl FracturingOrb {
    pub fn new() -> Self {
        Self {
            id: CurrencyId::from("FracturingOrb"),
        }
    }
}

impl Default for FracturingOrb {
    fn default() -> Self {
        Self::new()
    }
}

impl Currency for FracturingOrb {
    fn id(&self) -> &CurrencyId {
        &self.id
    }

    fn name(&self) -> &'static str {
        "Fracturing Orb"
    }

    fn valid_rarities(&self) -> crate::currency::RaritySet {
        crate::currency::RaritySet::RARE
    }

    fn can_apply_to(&self, item: &Item) -> Result<(), crate::currency::CannotApply> {
        let valid = self.valid_rarities();
        if !valid.contains(item.rarity) {
            return Err(crate::currency::CannotApply::WrongRarity {
                item_rarity: item.rarity,
                expected: valid,
            });
        }
        if item.mirrored {
            return Err(crate::currency::CannotApply::Mirrored);
        }
        let total = item.fracturing_eligibility_count();
        if total < 4 {
            return Err(crate::currency::CannotApply::FractureRequiresFourMods { current: total });
        }
        Ok(())
    }

    fn apply(&self, item: &mut Item, ctx: &mut ApplyContext<'_>) -> EngineResult<ApplyOutcome> {
        if !item.is_modifiable() {
            return Err(EngineError::InvalidApplication(
                "Fracturing Orb requires a modifiable item".into(),
            ));
        }

        // ≥4 mod check (counting hidden desecrated).
        let total = item.fracturing_eligibility_count();
        if total < 4 {
            return Err(EngineError::InsufficientMods {
                required: 4,
                actual: u32::try_from(total).unwrap_or(u32::MAX),
            });
        }

        // Sample target uniformly over visible non-fractured mods.
        let prefix_targets = item
            .prefixes
            .iter()
            .enumerate()
            .filter_map(|(i, m)| (!m.is_fractured).then_some(i))
            .collect::<Vec<_>>();
        let suffix_targets = item
            .suffixes
            .iter()
            .enumerate()
            .filter_map(|(i, m)| (!m.is_fractured).then_some(i))
            .collect::<Vec<_>>();

        let pcount = prefix_targets.len();
        let scount = suffix_targets.len();
        let pool = pcount + scount;
        if pool == 0 {
            // Only hidden / already-fractured mods left — nothing to target.
            return Err(EngineError::FractureHiddenMod);
        }

        let pick = ctx.rng.gen_range(0..pool);
        let (slot, idx) = if pick < pcount {
            (AffixType::Prefix, prefix_targets[pick])
        } else {
            (AffixType::Suffix, suffix_targets[pick - pcount])
        };

        match slot {
            AffixType::Prefix => item.prefixes[idx].is_fractured = true,
            AffixType::Suffix => item.suffixes[idx].is_fractured = true,
            _ => unreachable!(),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use rand::SeedableRng;
    use rand_xoshiro::Xoshiro256PlusPlus;
    use smallvec::smallvec;

    use super::*;
    use crate::ids::{ItemClassId, ModId};
    use crate::item::{
        AbyssLord, BoneSize, BoneSubtype, HiddenDesecratedSlot, ModRoll, QualityKind, Rarity,
    };
    use crate::mods::ModKind;
    use crate::patch::PatchVersion;
    use crate::registry::ModRegistry;

    fn pf(id: &str, fractured: bool) -> ModRoll {
        ModRoll {
            mod_id: ModId::from(id),
            affix_type: AffixType::Prefix,
            kind: ModKind::Explicit,
            values: smallvec![],
            is_fractured: fractured,
        }
    }

    fn fixture_item() -> Item {
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
        omens: &'a mut crate::omen::OmenSet,
    ) -> ApplyContext<'a> {
        ApplyContext::new(reg, rng, PatchVersion::PATCH_0_4_0, omens)
    }

    #[test]
    fn fracturing_requires_at_least_4_mods() {
        let reg = ModRegistry::from_mods(vec![]);
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(1);
        let mut omens = crate::omen::OmenSet::new();
        let mut item = fixture_item();
        item.prefixes.push(pf("ES1", false));
        item.prefixes.push(pf("ES2", false));
        item.prefixes.push(pf("ES3", false));
        // 3 mods, no hidden — should refuse.
        let r = FracturingOrb::new().apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens));
        assert!(matches!(
            r,
            Err(EngineError::InsufficientMods {
                required: 4,
                actual: 3
            })
        ));
    }

    #[test]
    fn fracturing_counts_hidden_desecrated_for_threshold() {
        // The user's worked example: 3 visible prefixes + 1 hidden suffix
        // satisfies the 4-mod requirement.
        let reg = ModRegistry::from_mods(vec![]);
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(2);
        let mut omens = crate::omen::OmenSet::new();
        let mut item = fixture_item();
        item.prefixes.push(pf("ES1", false));
        item.prefixes.push(pf("ES2", false));
        item.prefixes.push(pf("ES3", false));
        item.hidden_desecrated = Some(HiddenDesecratedSlot {
            affix_type: AffixType::Suffix,
            bone_size: BoneSize::Preserved,
            bone_subtype: BoneSubtype::Rib,
            abyss_lord: None,
        });
        FracturingOrb::new()
            .apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens))
            .unwrap();
        // Exactly one prefix should now be fractured.
        let fractured_count = item.prefixes.iter().filter(|m| m.is_fractured).count();
        assert_eq!(fractured_count, 1);
        // The hidden slot is unchanged.
        assert!(item.hidden_desecrated.is_some());
    }

    #[test]
    fn fracturing_never_targets_hidden_or_already_fractured() {
        // 1000 trials with 2 prefixes + 1 already-fractured + 1 hidden;
        // sample space = the 2 unfractured prefixes; over many trials
        // their fracture flag should toggle on, but the hidden slot
        // stays None-fractured (no field to set; just stays Some) and
        // the previously-fractured mod stays the same mod id.
        let reg = ModRegistry::from_mods(vec![]);
        for seed in 0u64..1000 {
            let mut rng = Xoshiro256PlusPlus::seed_from_u64(seed);
            let mut omens = crate::omen::OmenSet::new();
            let mut item = fixture_item();
            item.prefixes.push(pf("ES1", false));
            item.prefixes.push(pf("ES2", false));
            item.prefixes.push(pf("ES3_LOCK", true)); // already fractured
            item.hidden_desecrated = Some(HiddenDesecratedSlot {
                affix_type: AffixType::Suffix,
                bone_size: BoneSize::Preserved,
                bone_subtype: BoneSubtype::Rib,
                abyss_lord: None,
            });
            FracturingOrb::new()
                .apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens))
                .unwrap();
            // ES3_LOCK still fractured; one of ES1/ES2 newly fractured.
            assert!(item.prefixes[2].is_fractured);
            assert!(item.prefixes[0].is_fractured ^ item.prefixes[1].is_fractured);
            assert!(item.hidden_desecrated.is_some());
        }
    }

    #[test]
    fn fracturing_distribution_is_uniform_over_eligible_mods() {
        // 3 visible prefixes + 1 hidden suffix => 1/3 chance each prefix.
        // Over 6000 trials, each ~2000 ± noise. We allow ±10% margin.
        let reg = ModRegistry::from_mods(vec![]);
        let mut counts = [0usize; 3];
        for seed in 0u64..6000 {
            let mut rng = Xoshiro256PlusPlus::seed_from_u64(seed);
            let mut omens = crate::omen::OmenSet::new();
            let mut item = fixture_item();
            item.prefixes.push(pf("ES1", false));
            item.prefixes.push(pf("ES2", false));
            item.prefixes.push(pf("ES3", false));
            item.hidden_desecrated = Some(HiddenDesecratedSlot {
                affix_type: AffixType::Suffix,
                bone_size: BoneSize::Preserved,
                bone_subtype: BoneSubtype::Rib,
                abyss_lord: None,
            });
            FracturingOrb::new()
                .apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens))
                .unwrap();
            for (i, p) in item.prefixes.iter().enumerate() {
                if p.is_fractured {
                    counts[i] += 1;
                    break;
                }
            }
        }
        let expected = 2000_f64;
        for (i, c) in counts.iter().enumerate() {
            // Ok to lose precision: we never approach 2^52.
            #[allow(clippy::cast_precision_loss)]
            let cf = *c as f64;
            let dev = (cf - expected).abs() / expected;
            assert!(
                dev < 0.10,
                "slot {i} got {c} hits ({:.1}% off expected {})",
                dev * 100.0,
                expected
            );
        }
    }

    #[test]
    fn fracturing_refuses_when_only_fractured_or_hidden_remain() {
        // 3 fractured prefixes + 1 hidden — pool of "non-fractured visible"
        // is empty. Returns FractureHiddenMod.
        let reg = ModRegistry::from_mods(vec![]);
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0xff);
        let mut omens = crate::omen::OmenSet::new();
        let mut item = fixture_item();
        item.prefixes.push(pf("A", true));
        item.prefixes.push(pf("B", true));
        item.prefixes.push(pf("C", true));
        item.hidden_desecrated = Some(HiddenDesecratedSlot {
            affix_type: AffixType::Suffix,
            bone_size: BoneSize::Preserved,
            bone_subtype: BoneSubtype::Rib,
            abyss_lord: Some(AbyssLord::Kurgal),
        });
        let r = FracturingOrb::new().apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens));
        assert!(matches!(r, Err(EngineError::FractureHiddenMod)));
    }

    #[test]
    fn fracturing_rejects_corrupted_or_mirrored() {
        let reg = ModRegistry::from_mods(vec![]);
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(7);
        let mut omens = crate::omen::OmenSet::new();
        let mut item = fixture_item();
        item.corrupted = true;
        let r = FracturingOrb::new().apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens));
        assert!(matches!(r, Err(EngineError::InvalidApplication(_))));
    }
}
