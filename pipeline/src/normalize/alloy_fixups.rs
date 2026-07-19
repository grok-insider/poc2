//! Verisium Alloy mod affix / required-level fixups (PoE2 0.5).
//!
//! RePoE-fork has shipped the 50 `Alloy*` mods with an empty
//! `generation_type` and no `required_level` (serde defaults: `""` / `0`).
//! In [`super::repoe_to_bundle`] an empty `generation_type` makes
//! `parse_generation_type_to_affix` return `None`, which silently drops
//! the mod — and a `0` `required_level` defeats the advisor's level
//! gating. poe2db is explicit about both: every alloy page
//! (`/us/<name>_Alloy`) carries a per-class Pre/Suf + Required Level
//! table, and its ModifiersCalc rows tag the same mods with
//! `"IsAlloy":true`, `ModGenerationTypeID` (1 = Prefix, 2 = Suffix) and
//! `reqlvl`.
//!
//! [`ALLOY_AFFIX_TABLE`] is curated from both cached poe2db shapes
//! (`*_modsview*.json` / `rings_modcalc.json` IsAlloy rows, cross-checked
//! against the alloy pages' Pre/Suf + Required Level columns). Required
//! levels follow poe2db's convention: the character requirement,
//! `floor(0.8 × mod spawn level)` — the alloy families gate at
//! 10 / 20 / 36 / 52.
//!
//! Three alloy-named mods exported by RePoE-fork are absent from every
//! cached poe2db table (not granted by any of the 13 alloys):
//! `AlloyLocalWardIncreasePercent2`, `AlloyManaNearbyAllyAttackSpeedHybrid1`,
//! `AlloySpiritPresenceAreaOfEffectHybrid1`. They are intentionally not in
//! the table and pass through unchanged.

use poc2_data::Bundle;
use poc2_engine::item::AffixType;
use tracing::{info, warn};

/// Curated `engine mod id → (affix, poe2db required level)` table.
///
/// Grouped by granting alloy; the `/N` comments are the family's poe2db
/// "Required Level" column values.
#[rustfmt::skip]
const ALLOY_AFFIX_TABLE: &[(&str, AffixType, u32)] = &[
    // Runic Alloy /10
    ("AlloyMaximumRunicWard1", AffixType::Prefix, 10),
    ("AlloyMaximumRunicWardPercent1", AffixType::Prefix, 10),
    ("AlloyRunicWardRechargeRate1", AffixType::Prefix, 10),
    // Protective Alloy /10 (shields) + /20 (weapons, belts)
    ("AlloyRunicWardOnBlock1", AffixType::Suffix, 10),
    ("AlloyMaximumRunicWardWeapon1", AffixType::Suffix, 20),
    ("AlloyRecoverRunicWardOnCharmUse1", AffixType::Prefix, 20),
    // Adaptive Alloy /20
    ("AlloyAttackSpeedIfMissingWardRecently1", AffixType::Suffix, 20),
    ("AlloyDamageAsExtraFireTwoHandWhileMissingRunicWard1", AffixType::Prefix, 20),
    ("AlloyDamageAsExtraFireWhileMissingRunicWard1", AffixType::Prefix, 20),
    ("AlloyPuppetMasterChance1", AffixType::Prefix, 20),
    // Expansive Alloy /20
    ("AlloyManaCostEfficiency1", AffixType::Prefix, 20),
    ("AlloyPresenceAreaOfEffect1", AffixType::Suffix, 20),
    ("AlloyRemnantPickupRange1", AffixType::Suffix, 20),
    ("AlloyTemporaryMinionSkillLimit1", AffixType::Suffix, 20),
    // Sovereign Alloy /20 (armours) + /52 (weapons, jewellery)
    ("AlloyLocalWardIncreasePercent1", AffixType::Prefix, 20),
    ("AlloyEffectOfResistanceMods1", AffixType::Prefix, 52),
    ("AlloyEffectOfSocketedAugments1", AffixType::Suffix, 52),
    // Swift Alloy /36
    ("AlloyAttackSpeedRing1", AffixType::Suffix, 36),
    ("AlloyCastSpeedGloves1", AffixType::Suffix, 36),
    ("AlloyFlaskChargesPerSecond1", AffixType::Suffix, 36),
    ("AlloyTotemPlacementSpeed1", AffixType::Suffix, 36),
    // Cyclonic Alloy /36
    ("AlloyArchonDuration1", AffixType::Suffix, 36),
    ("AlloyDamagingAilmentDuration1", AffixType::Suffix, 36),
    ("AlloyReducedSlowPotency1", AffixType::Suffix, 36),
    ("AlloySkillEffectDuration1", AffixType::Suffix, 36),
    // Prismatic Alloy /36
    ("AlloyAilmentMagnitude1", AffixType::Suffix, 36),
    ("AlloyElementalPenetration1", AffixType::Prefix, 36),
    ("AlloyExposureEffect1", AffixType::Suffix, 36),
    ("AlloyMinionDamagingAilmentMagnitude1", AffixType::Suffix, 36),
    // Mystic Alloy /36
    ("AlloyAttackAreaOfEffect1", AffixType::Suffix, 36),
    ("AlloyChanceToChain1", AffixType::Suffix, 36),
    ("AlloyMaximumElementalInfusions1", AffixType::Suffix, 36),
    ("AlloySpellAreaOfEffect1", AffixType::Suffix, 36),
    ("AlloySpiritOnBoots1", AffixType::Suffix, 36),
    // Celestial Alloy /52
    ("AlloyAccuracyAttackSpeedHybrid1", AffixType::Prefix, 52),
    ("AlloySpellLevelManaHybrid1", AffixType::Prefix, 52),
    // Transcendent Alloy /52
    ("AlloyAttributeIncreasedLocalPhysicalDamageHybrid1", AffixType::Suffix, 52),
    ("AlloyCastSpeedDamageAsExtraColdHybrid1", AffixType::Suffix, 52),
    // The Runebinder's Alloy /52
    ("AlloyBallistaLimit1", AffixType::Suffix, 52),
    ("AlloyElementalSkillLimit1", AffixType::Suffix, 52),
    ("AlloyMarkEffect", AffixType::Suffix, 52),
    ("AlloyNaturesArchon1", AffixType::Suffix, 52),
    ("AlloyPuppeteerStacks1", AffixType::Suffix, 52),
    // The Runefather's Alloy /52
    ("AlloyBellLimit1", AffixType::Suffix, 52),
    ("AlloyLightningDamageIgnites1", AffixType::Suffix, 52),
    ("AlloyMeleeStrikeRange1", AffixType::Suffix, 52),
    ("AlloyRetainGlory1", AffixType::Suffix, 52),
];

/// Curated poe2db `(affix, required level)` for an alloy engine mod id.
///
/// Also consumed by `normalize_repoe`'s affix fallback so alloy mods with
/// an empty RePoE `generation_type` are kept instead of dropped.
pub fn alloy_affix_lookup(mod_id: &str) -> Option<(AffixType, u32)> {
    ALLOY_AFFIX_TABLE
        .iter()
        .find(|(id, _, _)| *id == mod_id)
        .map(|&(_, affix, lvl)| (affix, lvl))
}

/// Apply the curated alloy fixups to an already-normalized bundle.
///
/// Runs after `normalize_repoe`. Only fills holes: `required_level` is set
/// when the bundle value is `0`. `AffixType` has no empty variant, so the
/// affix can only be verified, never back-filled here (the back-fill for
/// affixless RePoE exports happens in `normalize_repoe` via
/// [`alloy_affix_lookup`]). Disagreements with nonempty upstream values are
/// logged, not overwritten.
///
/// Returns the number of mods whose `required_level` was set.
pub fn apply_alloy_fixups(bundle: &mut Bundle) -> usize {
    let mut fixed = 0usize;
    for m in &mut bundle.mods {
        let Some((affix, reqlvl)) = alloy_affix_lookup(m.id.as_str()) else {
            continue;
        };
        if m.affix_type != affix {
            warn!(
                mod_id = %m.id,
                bundle = ?m.affix_type,
                poe2db = ?affix,
                "alloy affix disagrees with poe2db table; keeping bundle value"
            );
        }
        if m.required_level == 0 {
            m.required_level = reqlvl;
            fixed += 1;
        } else if m.required_level != reqlvl && m.required_level * 4 / 5 != reqlvl {
            // poe2db's "Required Level" is the character requirement,
            // floor(0.8 × mod spawn level); RePoE ships the spawn level.
            // Either encoding of the same gate counts as agreement.
            warn!(
                mod_id = %m.id,
                bundle = m.required_level,
                poe2db = reqlvl,
                "alloy required_level disagrees with poe2db table; keeping bundle value"
            );
        }
    }
    if fixed > 0 {
        info!(fixed, "alloy required_level fixups applied (poe2db table)");
    }
    fixed
}

#[cfg(test)]
mod tests {
    use super::*;
    use poc2_engine::mods::{ModDomain, ModFlags, ModGroup, ModKind};
    use poc2_engine::{ModDefinition, ModGroupId, ModId, PatchRange, PatchVersion};
    use smallvec::smallvec;

    fn mk_mod(id: &str, affix: AffixType, required_level: u32) -> ModDefinition {
        ModDefinition {
            id: ModId::from(id),
            name: None,
            mod_group: ModGroup(ModGroupId::from(id)),
            affix_type: affix,
            kind: ModKind::Explicit,
            domain: ModDomain::Item,
            tags: smallvec![],
            concept_set: smallvec![],
            spawn_weights: smallvec![],
            stats: smallvec![],
            required_level,
            tier: None,
            allowed_item_classes: smallvec![],
            patch_range: PatchRange::ALL,
            flags: ModFlags::empty(),
            text_template: None,
        }
    }

    fn mk_bundle(mods: Vec<ModDefinition>) -> Bundle {
        let mut bundle = Bundle::empty(PatchVersion::PATCH_0_5_0, "test");
        bundle.mods = mods;
        bundle
    }

    #[test]
    fn lookup_covers_curated_alloys_only() {
        assert_eq!(
            alloy_affix_lookup("AlloyMaximumRunicWard1"),
            Some((AffixType::Prefix, 10))
        );
        assert_eq!(
            alloy_affix_lookup("AlloyMarkEffect"),
            Some((AffixType::Suffix, 52))
        );
        // The three alloy-named mods unsourceable from poe2db stay out.
        assert_eq!(alloy_affix_lookup("AlloyLocalWardIncreasePercent2"), None);
        assert_eq!(alloy_affix_lookup("FireResistance1"), None);
        assert_eq!(ALLOY_AFFIX_TABLE.len(), 47);
    }

    #[test]
    fn fixup_sets_required_level_only_when_zero() {
        let mut bundle = mk_bundle(vec![
            // Hole: RePoE shipped required_level 0 → poe2db value applies.
            mk_mod("AlloyMaximumRunicWard1", AffixType::Prefix, 0),
            // RePoE spawn level (13 → floor(0.8×13)=10): already agrees,
            // must not be rewritten to the poe2db character requirement.
            mk_mod("AlloyRunicWardRechargeRate1", AffixType::Prefix, 13),
        ]);
        let fixed = apply_alloy_fixups(&mut bundle);
        assert_eq!(fixed, 1);
        assert_eq!(bundle.mods[0].required_level, 10);
        assert_eq!(bundle.mods[1].required_level, 13);
    }

    #[test]
    fn fixup_leaves_foreign_and_unsourced_mods_alone() {
        let mut bundle = mk_bundle(vec![
            mk_mod("FireResistance1", AffixType::Suffix, 0),
            mk_mod("AlloyLocalWardIncreasePercent2", AffixType::Prefix, 0),
        ]);
        let fixed = apply_alloy_fixups(&mut bundle);
        assert_eq!(fixed, 0);
        assert_eq!(bundle.mods[0].required_level, 0);
        assert_eq!(bundle.mods[1].required_level, 0);
    }

    #[test]
    fn fixup_never_overwrites_disagreeing_affix() {
        // Suffix per poe2db, deliberately wrong Prefix in the bundle:
        // the pass logs and keeps the bundle value (upstream is the
        // source of truth for mods it actually shipped an affix for).
        let mut bundle = mk_bundle(vec![mk_mod("AlloyMarkEffect", AffixType::Prefix, 0)]);
        apply_alloy_fixups(&mut bundle);
        assert_eq!(bundle.mods[0].affix_type, AffixType::Prefix);
        assert_eq!(bundle.mods[0].required_level, 52);
    }
}
