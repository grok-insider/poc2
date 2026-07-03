//! Greater / Perfect tier variants of the basic orbs.
//!
//! Greater and Perfect variants of Transmute / Aug / Regal / Exalt / Chaos
//! behave identically to their base counterparts EXCEPT that the added mod
//! is constrained to `required_level >= min_mod_level`. This raises the
//! expected tier of the added mod (the 'rules out the lower tiers' effect
//! described in the apprentice blueprint).
//!
//! Min mod-level gates (patch-versioned; see `MinModLevelVariant::floor`):
//! - Greater Transmute / Aug:  55 pre-0.5, 44 in 0.5+ (0.5.0 patch notes
//!   lowered both from 55 → 44; confirmed on poe2db).
//! - Greater Regal / Chaos: 50.  Greater Exalt: 35 (wiki).
//! - Perfect Exalt: 50 (wiki).  Perfect Transmute/Aug/Regal/Chaos: 70.

use crate::currency::basic::chaos_apply;
use crate::currency::common::{
    affix_label, pick_open_affix, push_mod, roll_mod, sample_eligible_mod, BASIC_ORB_EXCLUDES,
};
use crate::currency::{ApplyContext, ApplyOutcome, Currency};
use crate::error::{EngineError, EngineResult};
use crate::ids::CurrencyId;
use crate::item::{Item, Rarity};

/// Identifies a Greater/Perfect currency variant for Minimum-Modifier-Level
/// floor resolution. The floor is **patch-versioned** (P2): 0.5 "Return of
/// the Ancients" lowered the Greater Transmutation/Augmentation floors, and
/// the live wiki documents Greater Exalted = 35 / Perfect Exalted = 50,
/// which differs from the historical pre-0.5 engine constants. Resolving the
/// floor at apply-time from `ctx.patch` lets one engine binary serve all
/// patches correctly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MinModLevelVariant {
    GreaterTransmute,
    GreaterAugment,
    GreaterRegal,
    GreaterExalt,
    GreaterChaos,
    PerfectTransmute,
    PerfectAugment,
    PerfectRegal,
    PerfectExalt,
    PerfectChaos,
}

impl MinModLevelVariant {
    /// Resolve the Minimum Modifier Level floor for this variant at a given
    /// patch.
    ///
    /// Sources:
    /// - 0.3 / 0.4 baseline: Greater Transmute / Aug 55, Greater Regal/Chaos
    ///   50, Greater Exalt 35 (wiki), Perfect Exalt 50 (wiki), other Perfect
    ///   variants 70.
    /// - 0.5 "Return of the Ancients": Greater Transmutation/Augmentation
    ///   lowered from 55 → 44 ("Greater Orbs of Transmutation and Greater
    ///   Orbs of Augmentation now have a minimum Modifier Level of 44
    ///   (previously 55)", 0.5.0 patch notes; confirmed on poe2db's
    ///   per-currency Minimum Modifier Level).
    pub(crate) fn floor(self, patch: crate::patch::PatchVersion) -> u32 {
        let is_0_5_plus = patch >= crate::patch::PatchVersion::PATCH_0_5_0;
        // Several variants currently share a floor value (e.g. 50 for Greater
        // Regal / Greater Chaos / Perfect Exalt). They are kept as distinct
        // arms on purpose: they are semantically different currencies sourced
        // from different wiki entries, and a future patch (or the P6 data
        // refresh) may diverge them independently.
        #[allow(clippy::match_same_arms)]
        match self {
            Self::GreaterTransmute => {
                // 0.5 "Return of the Ancients" lowered Greater Transmute from
                // 55 to 44: "Greater Orbs of Transmutation and Greater Orbs of
                // Augmentation now have a minimum Modifier Level of 44
                // (previously 55)" (0.5.0 patch notes; confirmed against
                // poe2db's per-currency Minimum Modifier Level = 44).
                if is_0_5_plus {
                    44
                } else {
                    55
                }
            }
            Self::GreaterAugment => {
                // Same 0.5 change as Greater Transmute: 55 → 44 (0.5.0 patch
                // notes; poe2db Minimum Modifier Level = 44).
                if is_0_5_plus {
                    44
                } else {
                    55
                }
            }
            Self::GreaterRegal => 50,
            // Wiki: Greater Exalted = 35 (not the legacy engine's 50). Apply
            // the wiki value for all patches since it is the documented
            // mechanic, not a 0.5 change.
            Self::GreaterExalt => 35,
            Self::GreaterChaos => 50,
            // Wiki: Perfect Exalted = 50. Other Perfect variants stay at 70
            // (the historical engine floor) until reconciled from data.
            Self::PerfectExalt => 50,
            Self::PerfectTransmute
            | Self::PerfectAugment
            | Self::PerfectRegal
            | Self::PerfectChaos => 70,
        }
    }
}

/// Resolve the Minimum-Modifier-Level floor for a currency id at a patch.
///
/// Returns `0` (no floor) for base-tier orbs and any unknown id. Public so
/// the advisor's analytic transition model can enumerate the exact pool a
/// Greater/Perfect orb draws from (via
/// [`crate::currency::enumerate_eligible_mods`]) without applying the orb.
/// The id strings match the `Currency::id` values defined in this module.
pub fn min_mod_level_floor(id: &CurrencyId, patch: crate::patch::PatchVersion) -> u32 {
    let variant = match id.as_str() {
        "GreaterOrbOfTransmutation" => MinModLevelVariant::GreaterTransmute,
        "PerfectOrbOfTransmutation" => MinModLevelVariant::PerfectTransmute,
        "GreaterOrbOfAugmentation" => MinModLevelVariant::GreaterAugment,
        "PerfectOrbOfAugmentation" => MinModLevelVariant::PerfectAugment,
        "GreaterRegalOrb" => MinModLevelVariant::GreaterRegal,
        "PerfectRegalOrb" => MinModLevelVariant::PerfectRegal,
        "GreaterExaltedOrb" => MinModLevelVariant::GreaterExalt,
        "PerfectExaltedOrb" => MinModLevelVariant::PerfectExalt,
        "GreaterChaosOrb" => MinModLevelVariant::GreaterChaos,
        "PerfectChaosOrb" => MinModLevelVariant::PerfectChaos,
        _ => return 0,
    };
    variant.floor(patch)
}

/// Generic implementation of "promote rarity, add 1 mod ≥ min_level".
/// Shared by Transmute / Greater / Perfect Transmutation variants.
fn add_one_mod_with_min(
    item: &mut Item,
    ctx: &mut ApplyContext<'_>,
    require_rarity: Rarity,
    promote_to: Option<Rarity>,
    max_slots: u8,
    min_level: u32,
    name: &'static str,
) -> EngineResult<()> {
    if !item.is_modifiable() {
        return Err(EngineError::InvalidApplication(format!(
            "{name} requires a modifiable item"
        )));
    }
    if item.rarity != require_rarity {
        return Err(EngineError::InvalidApplication(format!(
            "{name} requires a {require_rarity:?}-rarity item"
        )));
    }
    let affix = pick_open_affix(item, ctx.rng, max_slots)
        .ok_or(EngineError::AffixSlotFull { affix_type: name })?;
    let m = sample_eligible_mod(
        ctx.registry,
        ctx.base_registry,
        item,
        affix,
        ctx.rng,
        ctx.patch,
        min_level,
        BASIC_ORB_EXCLUDES,
    )
    .ok_or_else(|| EngineError::NoEligibleMods {
        base: item.base.to_string(),
        ilvl: item.ilvl,
        affix_type: affix_label(affix),
    })?;
    if let Some(rar) = promote_to {
        item.rarity = rar;
    }
    push_mod(item, roll_mod(m, ctx.rng));
    Ok(())
}

/// Generic Chaos-with-min-level used by Greater/Perfect Chaos variants.
/// Delegates to [`chaos_apply`] which already handles omens.
fn chaos_with_min(
    item: &mut Item,
    ctx: &mut ApplyContext<'_>,
    min_level: u32,
    _name: &'static str,
) -> EngineResult<()> {
    chaos_apply(item, ctx, min_level)
}

/// Defines a Greater/Perfect tier currency that wraps `add_one_mod_with_min`.
macro_rules! greater_perfect_add_currency {
    (
        $struct:ident,
        $id:literal,
        $disp:literal,
        $require:expr,
        $promote:expr,
        $max_slots:expr,
        $variant:expr,
        $valid_rarities:expr
    ) => {
        #[derive(Debug)]
        pub struct $struct {
            id: CurrencyId,
        }
        impl $struct {
            pub fn new() -> Self {
                Self {
                    id: CurrencyId::from($id),
                }
            }
        }
        impl Default for $struct {
            fn default() -> Self {
                Self::new()
            }
        }
        impl Currency for $struct {
            fn id(&self) -> &CurrencyId {
                &self.id
            }
            fn name(&self) -> &'static str {
                $disp
            }
            fn valid_rarities(&self) -> crate::currency::RaritySet {
                $valid_rarities
            }
            fn apply(
                &self,
                item: &mut Item,
                ctx: &mut ApplyContext<'_>,
            ) -> EngineResult<ApplyOutcome> {
                let min_level = $variant.floor(ctx.patch);
                add_one_mod_with_min(item, ctx, $require, $promote, $max_slots, min_level, $disp)
            }
        }
    };
}

/// Defines a Greater/Perfect tier Chaos.
macro_rules! greater_perfect_chaos {
    ($struct:ident, $id:literal, $disp:literal, $variant:expr) => {
        #[derive(Debug)]
        pub struct $struct {
            id: CurrencyId,
        }
        impl $struct {
            pub fn new() -> Self {
                Self {
                    id: CurrencyId::from($id),
                }
            }
        }
        impl Default for $struct {
            fn default() -> Self {
                Self::new()
            }
        }
        impl Currency for $struct {
            fn id(&self) -> &CurrencyId {
                &self.id
            }
            fn name(&self) -> &'static str {
                $disp
            }
            fn valid_rarities(&self) -> crate::currency::RaritySet {
                crate::currency::RaritySet::RARE
            }
            fn apply(
                &self,
                item: &mut Item,
                ctx: &mut ApplyContext<'_>,
            ) -> EngineResult<ApplyOutcome> {
                let min_level = $variant.floor(ctx.patch);
                chaos_with_min(item, ctx, min_level, $disp)
            }
        }
    };
}

// Transmutation -----------------------------------------------------------
greater_perfect_add_currency!(
    GreaterOrbOfTransmutation,
    "GreaterOrbOfTransmutation",
    "Greater Orb of Transmutation",
    Rarity::Normal,
    Some(Rarity::Magic),
    1,
    MinModLevelVariant::GreaterTransmute,
    crate::currency::RaritySet::NORMAL
);
greater_perfect_add_currency!(
    PerfectOrbOfTransmutation,
    "PerfectOrbOfTransmutation",
    "Perfect Orb of Transmutation",
    Rarity::Normal,
    Some(Rarity::Magic),
    1,
    MinModLevelVariant::PerfectTransmute,
    crate::currency::RaritySet::NORMAL
);

// Augmentation ------------------------------------------------------------
greater_perfect_add_currency!(
    GreaterOrbOfAugmentation,
    "GreaterOrbOfAugmentation",
    "Greater Orb of Augmentation",
    Rarity::Magic,
    None,
    1,
    MinModLevelVariant::GreaterAugment,
    crate::currency::RaritySet::MAGIC
);
greater_perfect_add_currency!(
    PerfectOrbOfAugmentation,
    "PerfectOrbOfAugmentation",
    "Perfect Orb of Augmentation",
    Rarity::Magic,
    None,
    1,
    MinModLevelVariant::PerfectAugment,
    crate::currency::RaritySet::MAGIC
);

// Regal -------------------------------------------------------------------
greater_perfect_add_currency!(
    GreaterRegalOrb,
    "GreaterRegalOrb",
    "Greater Regal Orb",
    Rarity::Magic,
    Some(Rarity::Rare),
    3,
    MinModLevelVariant::GreaterRegal,
    crate::currency::RaritySet::MAGIC
);
greater_perfect_add_currency!(
    PerfectRegalOrb,
    "PerfectRegalOrb",
    "Perfect Regal Orb",
    Rarity::Magic,
    Some(Rarity::Rare),
    3,
    MinModLevelVariant::PerfectRegal,
    crate::currency::RaritySet::MAGIC
);

// Exalted -----------------------------------------------------------------
greater_perfect_add_currency!(
    GreaterExaltedOrb,
    "GreaterExaltedOrb",
    "Greater Exalted Orb",
    Rarity::Rare,
    None,
    3,
    MinModLevelVariant::GreaterExalt,
    crate::currency::RaritySet::RARE
);
greater_perfect_add_currency!(
    PerfectExaltedOrb,
    "PerfectExaltedOrb",
    "Perfect Exalted Orb",
    Rarity::Rare,
    None,
    3,
    MinModLevelVariant::PerfectExalt,
    crate::currency::RaritySet::RARE
);

// Chaos -------------------------------------------------------------------
greater_perfect_chaos!(
    GreaterChaosOrb,
    "GreaterChaosOrb",
    "Greater Chaos Orb",
    MinModLevelVariant::GreaterChaos
);
greater_perfect_chaos!(
    PerfectChaosOrb,
    "PerfectChaosOrb",
    "Perfect Chaos Orb",
    MinModLevelVariant::PerfectChaos
);

#[cfg(test)]
mod tests {
    use rand::SeedableRng;
    use rand_xoshiro::Xoshiro256PlusPlus;
    use smallvec::smallvec;

    use super::*;
    use crate::currency::basic::OrbOfTransmutation;
    use crate::currency::common::test_fixtures::{
        ctx, fixture_normal_boots, fixture_tiered_registry,
    };
    use crate::ids::ModId;
    use crate::item::{AffixType, ModRoll};
    use crate::mods::ModKind;

    #[test]
    fn perfect_transmute_only_rolls_high_required_level_mods() {
        // Perfect demands required_level >= 70. With our fixture, the only
        // Life mod that qualifies is Life_T1 (req 75); Life_T2 (40) and
        // Life_T3 (1) are filtered out.
        let reg = fixture_tiered_registry();
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0x9001);
        let mut omens = crate::omen::OmenSet::new();
        let mut item = fixture_normal_boots();
        PerfectOrbOfTransmutation::new()
            .apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens))
            .unwrap();
        // The single rolled mod must be one of the T1 (req >= 70) candidates.
        let roll = item
            .prefixes
            .first()
            .or_else(|| item.suffixes.first())
            .unwrap();
        assert!(roll.mod_id.as_str().ends_with("_T1"), "got {}", roll.mod_id);
    }

    #[test]
    fn min_mod_level_floors_are_patch_versioned() {
        use crate::patch::PatchVersion;
        let p04 = PatchVersion::PATCH_0_4_0;
        let p05 = PatchVersion::PATCH_0_5_0;

        // 0.5 "Return of the Ancients" lowered the Greater Transmute/Aug
        // floors; the others are unchanged across patches.
        assert!(
            MinModLevelVariant::GreaterTransmute.floor(p05)
                < MinModLevelVariant::GreaterTransmute.floor(p04),
            "Greater Transmute floor should be lower in 0.5"
        );
        assert!(
            MinModLevelVariant::GreaterAugment.floor(p05)
                < MinModLevelVariant::GreaterAugment.floor(p04),
            "Greater Augment floor should be lower in 0.5"
        );

        // Wiki-sourced values (patch-invariant): Greater Exalt 35, Perfect
        // Exalt 50, Perfect (non-exalt) 70.
        assert_eq!(MinModLevelVariant::GreaterExalt.floor(p04), 35);
        assert_eq!(MinModLevelVariant::GreaterExalt.floor(p05), 35);
        assert_eq!(MinModLevelVariant::PerfectExalt.floor(p04), 50);
        assert_eq!(MinModLevelVariant::PerfectRegal.floor(p05), 70);
        assert_eq!(MinModLevelVariant::GreaterRegal.floor(p04), 50);
        assert_eq!(MinModLevelVariant::GreaterChaos.floor(p05), 50);
    }

    #[test]
    fn all_min_mod_level_arms_have_documented_floors_across_patches() {
        // Full matrix of every MinModLevelVariant arm × {0.4, 0.5}, pinned to
        // the documented source values (variants.rs::MinModLevelVariant::floor).
        // Guards against silent drift when the 0.5 TODO(0.5-data) numbers are
        // reconciled or a new variant is added. Each row is (variant, 0.4, 0.5).
        use crate::patch::PatchVersion;
        let p04 = PatchVersion::PATCH_0_4_0;
        let p05 = PatchVersion::PATCH_0_5_0;
        let matrix = [
            // 0.5 "Return of the Ancients" lowered Greater Transmute/Aug 55 → 44.
            (MinModLevelVariant::GreaterTransmute, 55u32, 44u32),
            (MinModLevelVariant::GreaterAugment, 55, 44),
            // Patch-invariant variants.
            (MinModLevelVariant::GreaterRegal, 50, 50),
            (MinModLevelVariant::GreaterExalt, 35, 35),
            (MinModLevelVariant::GreaterChaos, 50, 50),
            (MinModLevelVariant::PerfectTransmute, 70, 70),
            (MinModLevelVariant::PerfectAugment, 70, 70),
            (MinModLevelVariant::PerfectRegal, 70, 70),
            (MinModLevelVariant::PerfectExalt, 50, 50),
            (MinModLevelVariant::PerfectChaos, 70, 70),
        ];
        for (variant, want_04, want_05) in matrix {
            assert_eq!(
                variant.floor(p04),
                want_04,
                "{variant:?} floor at 0.4 should be {want_04}"
            );
            assert_eq!(
                variant.floor(p05),
                want_05,
                "{variant:?} floor at 0.5 should be {want_05}"
            );
            // Greater variants are never stricter than the Perfect arm of the
            // same family is permitted to be looser (a Perfect floor is always
            // >= the Greater floor of the same family — sanity on monotonicity
            // is family-specific, so we only assert per-arm values here).
        }
        // 0.5 only ever LOWERS a floor relative to 0.4, never raises it.
        for (variant, want_04, want_05) in matrix {
            assert!(
                want_05 <= want_04,
                "{variant:?}: 0.5 floor ({want_05}) must not exceed 0.4 floor ({want_04})"
            );
        }
    }

    #[test]
    fn greater_regal_filters_below_min_level() {
        // Greater Regal: min level 50. Life_T1 (75) and FireRes_T1 (75)
        // qualify; nothing else does. With the seed below, we should still
        // land on a high-tier mod.
        let reg = fixture_tiered_registry();
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0x9002);
        let mut omens = crate::omen::OmenSet::new();
        let mut item = fixture_normal_boots();
        OrbOfTransmutation::new()
            .apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens))
            .unwrap();
        GreaterRegalOrb::new()
            .apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens))
            .unwrap();
        assert_eq!(item.rarity, Rarity::Rare);
        // Among the up-to-3 mods on the Rare, the Regal-added one must be a
        // _T1. Since the Transmute step had no min-level constraint we can
        // only assert *at least one* mod is a T1.
        let any_t1 = item
            .prefixes
            .iter()
            .chain(item.suffixes.iter())
            .any(|m| m.mod_id.as_str().ends_with("_T1"));
        assert!(any_t1, "expected at least one T1 mod after Greater Regal");
    }

    #[test]
    fn perfect_exalt_filters_below_floor() {
        // Set up a Rare with one mod, then Perfect Exalt — the new mod
        // must be at/above the Perfect Exalt floor (50 per wiki). The
        // fixture's only qualifying tier is _T1 (req 75).
        let reg = fixture_tiered_registry();
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0x9003);
        let mut omens = crate::omen::OmenSet::new();
        let mut item = fixture_normal_boots();
        item.rarity = Rarity::Rare;
        item.prefixes.push(ModRoll {
            mod_id: ModId::from("Life_T3"),
            affix_type: AffixType::Prefix,
            kind: ModKind::Explicit,
            values: smallvec![],
            is_fractured: false,
        });
        PerfectExaltedOrb::new()
            .apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens))
            .unwrap();
        // The newly added mod (the LAST one in either prefixes or suffixes)
        // must end with _T1 since that's the only required_level>=70 mod
        // available given mod-group exclusivity (Life is occupied by T3).
        let last = item
            .suffixes
            .last()
            .or_else(|| item.prefixes.last())
            .unwrap();
        assert!(last.mod_id.as_str().ends_with("_T1"), "got {}", last.mod_id);
    }

    #[test]
    fn perfect_chaos_replacement_is_high_tier() {
        // Build a Rare with a T3 mod, then Perfect Chaos: the replacement
        // mod must be required_level >= 70.
        let reg = fixture_tiered_registry();
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0x9004);
        let mut omens = crate::omen::OmenSet::new();
        let mut item = fixture_normal_boots();
        item.rarity = Rarity::Rare;
        item.prefixes.push(ModRoll {
            mod_id: ModId::from("Life_T3"),
            affix_type: AffixType::Prefix,
            kind: ModKind::Explicit,
            values: smallvec![],
            is_fractured: false,
        });
        PerfectChaosOrb::new()
            .apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens))
            .unwrap();
        // After Chaos, the T3 mod is removed and a new high-level mod is added.
        let any_t3 = item
            .prefixes
            .iter()
            .chain(item.suffixes.iter())
            .any(|m| m.mod_id.as_str().ends_with("_T3"));
        let any_t1 = item
            .prefixes
            .iter()
            .chain(item.suffixes.iter())
            .any(|m| m.mod_id.as_str().ends_with("_T1"));
        assert!(!any_t3, "Perfect Chaos should not leave a T3 mod");
        assert!(any_t1, "Perfect Chaos should add a T1");
    }
}
