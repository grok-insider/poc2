//! P3 — desecration / bone mechanics fidelity (0.3+; stable 0.4/0.5).
//!
//! Covers the mechanics raised during research that the earlier engine did
//! not model precisely:
//!
//! 1. **Bone size → item level semantics.**
//!    - Gnawed: cannot desecrate items above ilvl 64.
//!    - Preserved: any ilvl, no modifier-level floor.
//!    - Ancient: any ilvl, but guarantees Min Modifier Level 40 on reveal.
//! 2. **Lord-targeting omens (Liege/Sovereign/Blackblooded) are
//!    Weapon/Jewellery only.** They are rejected on armour (Rib) and jewels
//!    (Cranium).
//! 3. **Lord omens brick the Ancient bone's Min-Mod-Level-40 floor.** When a
//!    lord omen is consumed with an Ancient bone, low-level mods may appear
//!    on reveal.
//! 4. **Already-desecrated rejection**, **full-mods**, and reveal floor
//!    filtering.

use poc2_engine::currency::{reveal_at_well_of_souls, sample_reveal_options, Bone, Currency};
use poc2_engine::ids::{ItemClassId, ModGroupId, ModId, StatId, TagId};
use poc2_engine::item::{AbyssLord, BoneSize, BoneSubtype, HiddenDesecratedSlot};
use poc2_engine::omen::{Omen, OmenSet};
use poc2_engine::patch::PatchVersion;
use poc2_engine::{
    AffixType, ApplyContext, BaseTypeId, EngineError, Item, ModDefinition, ModDomain, ModFlags,
    ModGroup, ModKind, ModRegistry, ModRoll, ModStat, PatchRange, QualityKind, Rarity,
};
use rand::SeedableRng;
use rand_xoshiro::Xoshiro256PlusPlus;
use smallvec::smallvec;

const PATCH: PatchVersion = PatchVersion::PATCH_0_4_0;

fn rare_item(class: &str, ilvl: u32) -> Item {
    Item {
        base: BaseTypeId::from(class),
        ilvl,
        rarity: Rarity::Rare,
        corrupted: false,
        sanctified: false,
        mirrored: false,
        quality: 0,
        quality_kind: QualityKind::Untagged,
        implicits: smallvec![],
        prefixes: smallvec![ModRoll {
            mod_id: ModId::from("Existing"),
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
    omens: &'a mut OmenSet,
) -> ApplyContext<'a> {
    ApplyContext::new_without_bases(reg, rng, PATCH, omens)
}

fn desecrated_mod(id: &str, affix: AffixType, group: &str, required_level: u32) -> ModDefinition {
    ModDefinition {
        id: ModId::from(id),
        name: None,
        mod_group: ModGroup(ModGroupId::from(group)),
        affix_type: affix,
        kind: ModKind::Desecrated,
        domain: ModDomain::Item,
        tags: smallvec![],
        concept_set: smallvec![],
        spawn_weights: smallvec![poc2_engine::SpawnWeight {
            tag: TagId::from("any"),
            weight: 1
        }],
        stats: smallvec![ModStat {
            stat_id: StatId::from("s"),
            min: 1.0,
            max: 10.0,
        }],
        required_level,
        tier: None,
        allowed_item_classes: smallvec![ItemClassId::from("BodyArmour")],
        patch_range: PatchRange::ALL,
        flags: ModFlags::DESECRATED_ONLY,
        text_template: None,
    }
}

// -------------------------------------------------------------------------
// Bone size → ilvl gating
// -------------------------------------------------------------------------

#[test]
fn gnawed_bone_rejects_items_above_ilvl_64() {
    let reg = ModRegistry::from_mods(vec![], vec![]);
    let mut rng = Xoshiro256PlusPlus::seed_from_u64(1);
    let mut omens = OmenSet::new();
    let mut item = rare_item("BodyArmour", 65);
    let r = Bone::new(BoneSize::Gnawed, BoneSubtype::Rib)
        .apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens));
    assert!(
        matches!(r, Err(EngineError::InvalidApplication(_))),
        "Gnawed bone must reject ilvl 65; got {r:?}"
    );
    // can_apply_to also rejects pre-flight.
    assert!(Bone::new(BoneSize::Gnawed, BoneSubtype::Rib)
        .can_apply_to(&item)
        .is_err());
}

#[test]
fn gnawed_bone_accepts_ilvl_64_boundary() {
    let reg = ModRegistry::from_mods(vec![], vec![]);
    let mut rng = Xoshiro256PlusPlus::seed_from_u64(2);
    let mut omens = OmenSet::new();
    let mut item = rare_item("BodyArmour", 64);
    Bone::new(BoneSize::Gnawed, BoneSubtype::Rib)
        .apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens))
        .expect("Gnawed must accept exactly ilvl 64");
    assert!(item.hidden_desecrated.is_some());
}

#[test]
fn preserved_and_ancient_accept_high_ilvl() {
    let reg = ModRegistry::from_mods(vec![], vec![]);
    for size in [BoneSize::Preserved, BoneSize::Ancient] {
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(3);
        let mut omens = OmenSet::new();
        let mut item = rare_item("BodyArmour", 84);
        Bone::new(size, BoneSubtype::Rib)
            .apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens))
            .unwrap_or_else(|e| panic!("{size:?} must accept ilvl 84: {e:?}"));
    }
}

#[test]
fn ancient_bone_sets_min_mod_level_40_on_slot() {
    let reg = ModRegistry::from_mods(vec![], vec![]);
    let mut rng = Xoshiro256PlusPlus::seed_from_u64(4);
    let mut omens = OmenSet::new();
    let mut item = rare_item("BodyArmour", 82);
    Bone::new(BoneSize::Ancient, BoneSubtype::Rib)
        .apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens))
        .unwrap();
    assert_eq!(
        item.hidden_desecrated.as_ref().unwrap().min_mod_level,
        40,
        "Ancient bone must guarantee Min Modifier Level 40"
    );
}

#[test]
fn preserved_bone_has_no_floor() {
    let reg = ModRegistry::from_mods(vec![], vec![]);
    let mut rng = Xoshiro256PlusPlus::seed_from_u64(5);
    let mut omens = OmenSet::new();
    let mut item = rare_item("BodyArmour", 82);
    Bone::new(BoneSize::Preserved, BoneSubtype::Rib)
        .apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens))
        .unwrap();
    assert_eq!(item.hidden_desecrated.as_ref().unwrap().min_mod_level, 0);
}

// -------------------------------------------------------------------------
// Ancient floor enforced at reveal
// -------------------------------------------------------------------------

#[test]
fn ancient_floor_filters_sub_40_mods_at_reveal() {
    // Hidden slot from an Ancient bone (floor 40). The reveal pool has a
    // low-level mod (req 10) and a high-level one (req 75). Only the high
    // one is offered.
    let mut rng = Xoshiro256PlusPlus::seed_from_u64(6);
    let mut item = rare_item("BodyArmour", 82);
    item.hidden_desecrated = Some(HiddenDesecratedSlot {
        affix_type: AffixType::Suffix,
        bone_size: BoneSize::Ancient,
        bone_subtype: BoneSubtype::Rib,
        abyss_lord: None,
        min_mod_level: 40,
        otherworldly: false,
    });
    let pool = vec![
        desecrated_mod("Low", AffixType::Suffix, "GA", 10),
        desecrated_mod("High", AffixType::Suffix, "GB", 75),
    ];
    let opts = sample_reveal_options(&item, &pool, 3, &mut rng);
    assert!(
        opts.iter().all(|id| id.as_str() == "High"),
        "Ancient floor must exclude the sub-40 mod; got {opts:?}"
    );
    assert!(
        !opts.is_empty(),
        "the high-level mod should still be offered"
    );
}

#[test]
fn bricked_floor_allows_sub_40_mods_at_reveal() {
    // Same pool, but the slot's floor was bricked to 0 by a lord omen.
    let mut rng = Xoshiro256PlusPlus::seed_from_u64(7);
    let mut item = rare_item("Bow", 82);
    item.hidden_desecrated = Some(HiddenDesecratedSlot {
        affix_type: AffixType::Suffix,
        bone_size: BoneSize::Ancient,
        bone_subtype: BoneSubtype::Jawbone,
        abyss_lord: Some(AbyssLord::Amanamu),
        min_mod_level: 0, // bricked,
        otherworldly: false,
    });
    let pool = vec![
        desecrated_mod("Low", AffixType::Suffix, "GA", 10),
        desecrated_mod("High", AffixType::Suffix, "GB", 75),
    ];
    let opts = sample_reveal_options(&item, &pool, 3, &mut rng);
    assert!(
        opts.iter().any(|id| id.as_str() == "Low"),
        "a bricked Ancient floor must allow the sub-40 mod; got {opts:?}"
    );
}

// -------------------------------------------------------------------------
// Lord-omen scope (Weapon/Jewellery only) + Ancient-floor brick
// -------------------------------------------------------------------------

#[test]
fn lord_omen_rejected_on_armour_rib() {
    let reg = ModRegistry::from_mods(vec![], vec![]);
    let mut rng = Xoshiro256PlusPlus::seed_from_u64(8);
    let mut omens = OmenSet::new();
    omens.push(Omen::liege()); // Amanamu, weapon/jewellery only
    let mut item = rare_item("BodyArmour", 82);
    let r = Bone::new(BoneSize::Preserved, BoneSubtype::Rib)
        .apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens));
    assert!(
        matches!(r, Err(EngineError::InvalidApplication(_))),
        "lord omen on a Rib (armour) bone must be rejected; got {r:?}"
    );
}

#[test]
fn lord_omen_accepted_on_weapon_jawbone() {
    let reg = ModRegistry::from_mods(vec![], vec![]);
    let mut rng = Xoshiro256PlusPlus::seed_from_u64(9);
    let mut omens = OmenSet::new();
    omens.push(Omen::sovereign()); // Ulaman
    let mut item = rare_item("Bow", 82);
    Bone::new(BoneSize::Preserved, BoneSubtype::Jawbone)
        .apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens))
        .expect("lord omen on a Jawbone (weapon) bone must succeed");
    assert_eq!(
        item.hidden_desecrated.as_ref().unwrap().abyss_lord,
        Some(AbyssLord::Ulaman)
    );
}

#[test]
fn lord_omen_bricks_ancient_floor() {
    // Ancient Jawbone normally guarantees Min Mod Level 40, but a lord omen
    // bricks it to 0.
    let reg = ModRegistry::from_mods(vec![], vec![]);
    let mut rng = Xoshiro256PlusPlus::seed_from_u64(10);
    let mut omens = OmenSet::new();
    omens.push(Omen::blackblooded()); // Kurgal
    let mut item = rare_item("Bow", 82);
    Bone::new(BoneSize::Ancient, BoneSubtype::Jawbone)
        .apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens))
        .unwrap();
    let slot = item.hidden_desecrated.as_ref().unwrap();
    assert_eq!(
        slot.min_mod_level, 0,
        "lord omen must brick the Ancient floor to 0"
    );
    assert_eq!(slot.abyss_lord, Some(AbyssLord::Kurgal));
}

#[test]
fn ancient_floor_kept_without_lord_omen() {
    let reg = ModRegistry::from_mods(vec![], vec![]);
    let mut rng = Xoshiro256PlusPlus::seed_from_u64(11);
    let mut omens = OmenSet::new();
    let mut item = rare_item("Bow", 82);
    Bone::new(BoneSize::Ancient, BoneSubtype::Jawbone)
        .apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens))
        .unwrap();
    assert_eq!(item.hidden_desecrated.as_ref().unwrap().min_mod_level, 40);
}

// -------------------------------------------------------------------------
// Already-desecrated + reveal round trip
// -------------------------------------------------------------------------

#[test]
fn cannot_desecrate_already_desecrated_item() {
    let reg = ModRegistry::from_mods(vec![], vec![]);
    let mut rng = Xoshiro256PlusPlus::seed_from_u64(12);
    let mut omens = OmenSet::new();
    let mut item = rare_item("BodyArmour", 82);
    Bone::new(BoneSize::Preserved, BoneSubtype::Rib)
        .apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens))
        .unwrap();
    let r = Bone::new(BoneSize::Ancient, BoneSubtype::Rib)
        .apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens));
    assert!(
        matches!(r, Err(EngineError::InvalidApplication(_))),
        "second desecration must be rejected; got {r:?}"
    );
}

#[test]
fn reveal_round_trip_after_ancient_bone() {
    let mut rng = Xoshiro256PlusPlus::seed_from_u64(13);
    let mut item = rare_item("BodyArmour", 82);
    item.hidden_desecrated = Some(HiddenDesecratedSlot {
        affix_type: AffixType::Suffix,
        bone_size: BoneSize::Ancient,
        bone_subtype: BoneSubtype::Rib,
        abyss_lord: None,
        min_mod_level: 40,
        otherworldly: false,
    });
    let pool = vec![desecrated_mod("High", AffixType::Suffix, "GB", 75)];
    let opts = sample_reveal_options(&item, &pool, 3, &mut rng);
    assert_eq!(opts.len(), 1);
    reveal_at_well_of_souls(&mut item, &pool, &opts[0], &mut rng).unwrap();
    assert!(item.hidden_desecrated.is_none());
    assert_eq!(item.suffixes.len(), 1);
    assert_eq!(item.suffixes[0].kind, ModKind::Desecrated);
}
