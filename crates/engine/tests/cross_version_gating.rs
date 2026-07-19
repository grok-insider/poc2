//! P4 — cross-version (patch + league) gating.
//!
//! The same engine binary must evaluate 0.3 / 0.4 / 0.5 correctly. Items and
//! omens that are disabled in one (patch, league) must be gated:
//!
//! - **Recombinator**: removed in 0.5; available in 0.5 only in Standard.
//! - **Omen of Corruption**: Standard-only in 0.5 (Runes of Aldur disables
//!   it).
//! - **Homogenising omens**: 0.3.x-only (legacy stockpile semantics in
//!   0.4+).
//! - **Min-Modifier-Level floors**: 0.5 lowered Greater Transmute/Aug.

use poc2_engine::currency::basic::{ExaltedOrb, GreaterExaltedOrb, PerfectExaltedOrb};
use poc2_engine::currency::recombinator_available;
use poc2_engine::omen::{Omen, OmenSet};
use poc2_engine::patch::{League, PatchVersion};
use poc2_engine::{Currency, RaritySet};

const P03: PatchVersion = PatchVersion::new(0, 3, 0);
const P04: PatchVersion = PatchVersion::PATCH_0_4_0;
const P05: PatchVersion = PatchVersion::PATCH_0_5_0;

// -------------------------------------------------------------------------
// Recombinator availability
// -------------------------------------------------------------------------

#[test]
fn recombinator_available_pre_0_5_all_leagues() {
    for patch in [P03, P04] {
        for league in [League::Standard, League::Challenge] {
            assert!(
                recombinator_available(patch, league),
                "recombinator should be available at {patch} / {league:?}"
            );
        }
    }
}

#[test]
fn recombinator_disabled_in_0_5_challenge() {
    assert!(
        !recombinator_available(P05, League::Challenge),
        "Recombinator must be disabled in 0.5 Runes of Aldur (Challenge)"
    );
}

#[test]
fn recombinator_still_available_in_0_5_standard() {
    assert!(
        recombinator_available(P05, League::Standard),
        "Recombinator must still work in 0.5 Standard (legacy items)"
    );
}

#[test]
fn recombine_gated_errors_in_0_5_challenge() {
    use poc2_engine::currency::recombine_gated;
    use poc2_engine::{
        AffixType, BaseTypeId, Item, ModId, ModKind, ModRegistry, ModRoll, QualityKind, Rarity,
    };
    use rand::SeedableRng;
    use rand_xoshiro::Xoshiro256PlusPlus;
    use smallvec::smallvec;

    let reg = ModRegistry::from_mods(vec![], vec![]);
    let mut rng = Xoshiro256PlusPlus::seed_from_u64(1);
    let mk = || Item {
        base: BaseTypeId::from("Bow"),
        ilvl: 82,
        rarity: Rarity::Rare,
        corrupted: false,
        sanctified: false,
        mirrored: false,
        quality: 0,
        quality_kind: QualityKind::Untagged,
        implicits: smallvec![],
        prefixes: smallvec![ModRoll {
            mod_id: ModId::from("M"),
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
    };
    let a = mk();
    let b = mk();
    // 0.5 Challenge → error.
    let r = recombine_gated(&a, &b, &reg, &mut rng, P05, League::Challenge);
    assert!(r.is_err(), "recombine_gated must error in 0.5 Challenge");
    // 0.5 Standard → ok.
    let r2 = recombine_gated(&a, &b, &reg, &mut rng, P05, League::Standard);
    assert!(r2.is_ok(), "recombine_gated must succeed in 0.5 Standard");
    // 0.4 Challenge → ok.
    let r3 = recombine_gated(&a, &b, &reg, &mut rng, P04, League::Challenge);
    assert!(r3.is_ok(), "recombine_gated must succeed in 0.4");
}

// -------------------------------------------------------------------------
// Omen of Corruption Standard-only in 0.5
// -------------------------------------------------------------------------

#[test]
fn corruption_omen_consumed_in_0_4() {
    let mut s = OmenSet::from_iter_omens([Omen::corruption()]);
    assert!(
        s.consume_prevent_no_change(P04, League::Challenge),
        "Corruption omen should be consumable in 0.4 Challenge"
    );
}

#[test]
fn corruption_omen_standard_only_in_0_5() {
    // Challenge 0.5: NOT consumed.
    let mut challenge = OmenSet::from_iter_omens([Omen::corruption()]);
    assert!(
        !challenge.consume_prevent_no_change(P05, League::Challenge),
        "Corruption omen must NOT be consumed in 0.5 Runes of Aldur"
    );
    // Standard 0.5: still consumed (legacy).
    let mut standard = OmenSet::from_iter_omens([Omen::corruption()]);
    assert!(
        standard.consume_prevent_no_change(P05, League::Standard),
        "Corruption omen must still work in 0.5 Standard"
    );
}

// -------------------------------------------------------------------------
// Homogenising omens 0.3-only
// -------------------------------------------------------------------------

#[test]
fn homogenising_omen_0_3_only() {
    let mut s03 = OmenSet::from_iter_omens([Omen::homogenising_exaltation()]);
    assert!(
        s03.consume_homogenising(P03),
        "Homogenising omen should be consumable in 0.3"
    );
    let mut s04 = OmenSet::from_iter_omens([Omen::homogenising_exaltation()]);
    assert!(
        !s04.consume_homogenising(P04),
        "Homogenising omen must be disabled in 0.4+"
    );
    let mut s05 = OmenSet::from_iter_omens([Omen::homogenising_coronation()]);
    assert!(
        !s05.consume_homogenising(P05),
        "Homogenising coronation must be disabled in 0.5"
    );
}

// -------------------------------------------------------------------------
// Currency rarity gates are stable across patches (sanity)
// -------------------------------------------------------------------------

#[test]
fn exalt_variants_are_rare_only_all_patches() {
    assert_eq!(ExaltedOrb::new().valid_rarities(), RaritySet::RARE);
    assert_eq!(GreaterExaltedOrb::new().valid_rarities(), RaritySet::RARE);
    assert_eq!(PerfectExaltedOrb::new().valid_rarities(), RaritySet::RARE);
}

// -------------------------------------------------------------------------
// Verisium Alloy patch gating (0.5+ only; no league restriction)
// -------------------------------------------------------------------------

#[test]
fn alloy_rejected_in_0_4_accepted_in_0_5() {
    use poc2_engine::currency::ApplyContext;
    use poc2_engine::error::EngineError;
    use poc2_engine::ids::{ItemClassId, ModGroupId, StatId, TagId};
    use poc2_engine::mods::{ModDefinition, ModDomain, ModFlags, ModGroup, ModStat, SpawnWeight};
    use poc2_engine::omen::OmenSet;
    use poc2_engine::patch::PatchRange;
    use poc2_engine::{
        AffixType, Alloy, BaseTypeId, Item, ModId, ModKind, ModRegistry, ModRoll, QualityKind,
        Rarity,
    };
    use rand::SeedableRng;
    use rand_xoshiro::Xoshiro256PlusPlus;
    use smallvec::smallvec;

    // Explicit prefix target mod (mirrors alloy.rs `alloy_mod()` — 16 fields).
    let target_mod = ModDefinition {
        id: ModId::from("RunicWardCrafted"),
        name: Some("Verisium Runic Ward".into()),
        mod_group: ModGroup(ModGroupId::from("RunicWard")),
        affix_type: AffixType::Prefix,
        kind: ModKind::Explicit,
        domain: ModDomain::Item,
        tags: smallvec![TagId::from("runic_ward")],
        concept_set: smallvec![],
        spawn_weights: smallvec![SpawnWeight {
            tag: TagId::from("runic_ward"),
            weight: 1,
        }],
        stats: smallvec![ModStat {
            stat_id: StatId::from("runic_ward"),
            min: 20.0,
            max: 40.0,
        }],
        required_level: 1,
        tier: None,
        allowed_item_classes: smallvec![ItemClassId::from("BodyArmour")],
        patch_range: PatchRange::ALL,
        flags: ModFlags::empty(),
        text_template: None,
    };
    let reg = ModRegistry::from_mods(vec![target_mod], vec![]);

    // Rare item with one removable non-fractured mod.
    let mk = || Item {
        base: BaseTypeId::from("BodyArmour"),
        ilvl: 82,
        rarity: Rarity::Rare,
        corrupted: false,
        sanctified: false,
        mirrored: false,
        quality: 0,
        quality_kind: QualityKind::Untagged,
        implicits: smallvec![],
        prefixes: smallvec![ModRoll {
            mod_id: ModId::from("OldPrefix"),
            affix_type: AffixType::Prefix,
            kind: ModKind::Explicit,
            values: smallvec![1.0],
            is_fractured: false,
        }],
        suffixes: smallvec![],
        enchantments: smallvec![],
        hidden_desecrated: None,
        sockets: smallvec![],
        hinekora_lock: None,
    };

    let alloy = poc2_engine::currency::Alloy::new("AlloyX", "Verisium Alloy X", "RunicWardCrafted");

    // 0.4 → rejected (Alloys are a 0.5 system).
    {
        let mut item = mk();
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(7);
        let mut omens = OmenSet::new();
        let mut ctx = ApplyContext::new_without_bases(&reg, &mut rng, P04, &mut omens);
        let r = alloy.apply(&mut item, &mut ctx);
        assert!(
            matches!(r, Err(EngineError::InvalidApplication(_))),
            "Alloy must be rejected in 0.4; got {r:?}"
        );
    }

    // 0.5 → ok, and the crafted mod is present (net count unchanged: remove 1, add 1).
    {
        let mut item = mk();
        let before = item.prefixes.len() + item.suffixes.len();
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(7);
        let mut omens = OmenSet::new();
        let mut ctx = ApplyContext::new_without_bases(&reg, &mut rng, P05, &mut omens);
        alloy
            .apply(&mut item, &mut ctx)
            .expect("Alloy must succeed in 0.5");
        assert_eq!(
            item.prefixes.len() + item.suffixes.len(),
            before,
            "Alloy replaces (remove 1, add 1) — net count unchanged"
        );
        assert!(
            item.prefixes
                .iter()
                .chain(item.suffixes.iter())
                .any(|m| m.mod_id.as_str() == "RunicWardCrafted"),
            "crafted mod must be present after a 0.5 Alloy apply"
        );

        // Sanity: the freshly-constructed `Alloy` type at the crate root is the
        // same currency the `currency::Alloy` path produced.
        let _: Alloy = Alloy::new("AlloyX", "Verisium Alloy X", "RunicWardCrafted");
    }
}

#[test]
fn alloy_patch_range_constant() {
    use poc2_engine::Alloy;
    assert!(
        Alloy::PATCH_RANGE.contains(P05),
        "Alloy::PATCH_RANGE must include 0.5.0"
    );
    assert!(
        !Alloy::PATCH_RANGE.contains(P04),
        "Alloy::PATCH_RANGE must exclude 0.4.0"
    );
}

// -------------------------------------------------------------------------
// 0.5 value-shift semantics: Vaal "unpredictable values" + Sanctification
// multiply current values instead of randomising (patch notes 0.5)
// -------------------------------------------------------------------------

mod value_shift_0_5 {
    use super::{P04, P05};
    use poc2_engine::currency::basic::{DivineOrb, VaalOrb};
    use poc2_engine::currency::ApplyContext;
    use poc2_engine::ids::{ItemClassId, ModGroupId, StatId, TagId};
    use poc2_engine::mods::{ModDefinition, ModDomain, ModFlags, ModGroup, ModStat, SpawnWeight};
    use poc2_engine::omen::{Omen, OmenSet};
    use poc2_engine::patch::{PatchRange, PatchVersion};
    use poc2_engine::{
        AffixType, BaseTypeId, Currency, Item, ModId, ModKind, ModRegistry, ModRoll, QualityKind,
        Rarity,
    };
    use rand::SeedableRng;
    use rand_xoshiro::Xoshiro256PlusPlus;
    use smallvec::smallvec;

    /// A fixed-value mod (min == max == 100) so a within-range reroll always
    /// lands back on exactly 100.0, while a multiplicative shift does not.
    fn fixed_mod(id: &str, affix: AffixType, kind: ModKind, lo: f64, hi: f64) -> ModDefinition {
        ModDefinition {
            id: ModId::from(id),
            name: Some(id.into()),
            mod_group: ModGroup(ModGroupId::from(id)),
            affix_type: affix,
            kind,
            domain: ModDomain::Item,
            tags: smallvec![TagId::from("t")],
            concept_set: smallvec![],
            spawn_weights: smallvec![SpawnWeight {
                tag: TagId::from("t"),
                weight: 1,
            }],
            stats: smallvec![ModStat {
                stat_id: StatId::from(id),
                min: lo,
                max: hi,
            }],
            required_level: 1,
            tier: None,
            allowed_item_classes: smallvec![ItemClassId::from("BodyArmour")],
            patch_range: PatchRange::ALL,
            flags: ModFlags::empty(),
            text_template: None,
        }
    }

    fn registry() -> ModRegistry {
        ModRegistry::from_mods(
            vec![
                fixed_mod("M100", AffixType::Prefix, ModKind::Explicit, 100.0, 100.0),
                fixed_mod("Imp", AffixType::Implicit, ModKind::Implicit, 50.0, 60.0),
            ],
            vec![],
        )
    }

    fn item() -> Item {
        Item {
            base: BaseTypeId::from("BodyArmour"),
            ilvl: 82,
            rarity: Rarity::Rare,
            corrupted: false,
            sanctified: false,
            mirrored: false,
            quality: 0,
            quality_kind: QualityKind::Untagged,
            implicits: smallvec![ModRoll {
                mod_id: ModId::from("Imp"),
                affix_type: AffixType::Implicit,
                kind: ModKind::Implicit,
                values: smallvec![55.0],
                is_fractured: false,
            }],
            prefixes: smallvec![ModRoll {
                mod_id: ModId::from("M100"),
                affix_type: AffixType::Prefix,
                kind: ModKind::Explicit,
                values: smallvec![100.0],
                is_fractured: false,
            }],
            suffixes: smallvec![],
            enchantments: smallvec![],
            hidden_desecrated: None,
            sockets: smallvec![],
            hinekora_lock: None,
        }
    }

    #[test]
    fn vaal_value_outcome_multiplies_in_0_5() {
        let reg = registry();
        let vaal = VaalOrb::new();
        let mut shifted = 0u32;
        for seed in 0..400u64 {
            let mut it = item();
            let mut rng = Xoshiro256PlusPlus::seed_from_u64(seed);
            let mut omens = OmenSet::new();
            let mut ctx = ApplyContext::new_without_bases(&reg, &mut rng, P05, &mut omens);
            let _ = vaal.apply(&mut it, &mut ctx);
            // Identify the value-shift outcome: the original prefix survived
            // but its value moved off the fixed 100.0 point.
            if it.prefixes.len() == 1 && it.prefixes[0].mod_id.as_str() == "M100" {
                let v = it.prefixes[0].values[0];
                if (v - 100.0).abs() > 1e-9 {
                    shifted += 1;
                    assert!(
                        (80.0..=125.0).contains(&v),
                        "0.5 corruption shift must stay within the modelled \
                         [0.8, 1.25] factor band; got {v}"
                    );
                }
            }
        }
        assert!(
            shifted > 0,
            "expected at least one RerollValues outcome across 400 seeds"
        );
    }

    #[test]
    fn vaal_value_outcome_rerolls_within_range_in_0_4() {
        let reg = registry();
        let vaal = VaalOrb::new();
        for seed in 0..400u64 {
            let mut it = item();
            let mut rng = Xoshiro256PlusPlus::seed_from_u64(seed);
            let mut omens = OmenSet::new();
            let mut ctx = ApplyContext::new_without_bases(&reg, &mut rng, P04, &mut omens);
            let _ = vaal.apply(&mut it, &mut ctx);
            if it.prefixes.len() == 1 && it.prefixes[0].mod_id.as_str() == "M100" {
                assert!(
                    (it.prefixes[0].values[0] - 100.0).abs() < 1e-9,
                    "0.4 reroll of a fixed-range mod must land back on 100.0"
                );
            }
        }
    }

    #[test]
    fn sanctification_multiplies_and_locks_in_0_5() {
        let reg = registry();
        let divine = DivineOrb::new();
        let mut it = item();
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(11);
        let mut omens = OmenSet::from_iter_omens([Omen::sanctification()]);
        let mut ctx = ApplyContext::new_without_bases(&reg, &mut rng, P05, &mut omens);
        divine.apply(&mut it, &mut ctx).unwrap();
        assert!(it.sanctified, "Sanctification must lock the item");
        let v = it.prefixes[0].values[0];
        assert!(
            (80.0..=120.0).contains(&v),
            "0.5 sanctification multiplies by [0.8, 1.2]; got {v}"
        );
    }

    #[test]
    fn sanctification_extended_reroll_in_0_4() {
        let reg = registry();
        let divine = DivineOrb::new();
        let mut it = item();
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(13);
        let mut omens = OmenSet::from_iter_omens([Omen::sanctification()]);
        let mut ctx = ApplyContext::new_without_bases(&reg, &mut rng, P04, &mut omens);
        divine.apply(&mut it, &mut ctx).unwrap();
        assert!(it.sanctified, "Sanctification must lock the item");
        let v = it.prefixes[0].values[0];
        assert!(
            (80.0..=120.0).contains(&v),
            "0.4 sanctification rolls in 80-120% of the normal range; got {v}"
        );
    }

    #[test]
    fn blessed_rerolls_only_implicits() {
        let reg = registry();
        let divine = DivineOrb::new();
        let mut it = item();
        it.implicits[0].values[0] = 0.0; // sentinel outside [50, 60]
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(17);
        let mut omens = OmenSet::from_iter_omens([Omen::blessed()]);
        let mut ctx = ApplyContext::new_without_bases(&reg, &mut rng, P05, &mut omens);
        divine.apply(&mut it, &mut ctx).unwrap();
        let imp = it.implicits[0].values[0];
        assert!(
            (50.0..=60.0).contains(&imp),
            "Blessed must reroll the implicit within its range; got {imp}"
        );
        assert!(
            (it.prefixes[0].values[0] - 100.0).abs() < 1e-9,
            "Blessed must not touch explicit mods"
        );
        assert!(!it.sanctified, "Blessed does not sanctify");
    }

    #[test]
    fn bone_blocked_by_revealed_desecrated_mod_in_0_5_only() {
        use poc2_engine::currency::bone::Bone;
        use poc2_engine::item::{BoneSize, BoneSubtype};
        let reg = registry();
        let bone = Bone::new(BoneSize::Preserved, BoneSubtype::Rib);
        let mk = || {
            let mut it = item();
            it.suffixes.push(ModRoll {
                mod_id: ModId::from("Desec"),
                affix_type: AffixType::Suffix,
                kind: ModKind::Desecrated,
                values: smallvec![1.0],
                is_fractured: false,
            });
            it
        };
        // 0.5: hard-rejected by the 1-desecrated-mod cap.
        {
            let mut it = mk();
            let mut rng = Xoshiro256PlusPlus::seed_from_u64(19);
            let mut omens = OmenSet::new();
            let mut ctx = ApplyContext::new_without_bases(&reg, &mut rng, P05, &mut omens);
            let r = bone.apply(&mut it, &mut ctx);
            let msg = format!("{r:?}");
            assert!(r.is_err() && msg.contains("limit 1 in 0.5"), "got {msg}");
        }
        // 0.4: the cap does not fire (any failure must be for another reason).
        {
            let mut it = mk();
            let mut rng = Xoshiro256PlusPlus::seed_from_u64(19);
            let mut omens = OmenSet::new();
            let mut ctx = ApplyContext::new_without_bases(&reg, &mut rng, P04, &mut omens);
            let r = bone.apply(&mut it, &mut ctx);
            let msg = format!("{r:?}");
            assert!(
                !msg.contains("limit 1 in 0.5"),
                "0.4 must not enforce the 0.5 desecrated cap; got {msg}"
            );
        }
    }

    // Keep PatchVersion referenced so the import stays used if tests shrink.
    #[allow(dead_code)]
    const _P: PatchVersion = PatchVersion::PATCH_0_5_0;
}
