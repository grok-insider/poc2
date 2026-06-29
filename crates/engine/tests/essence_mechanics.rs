//! P3 — essence mechanics edge cases (0.3 rework; stable 0.4/0.5).
//!
//! The essence model lives in `currency/essence.rs` and already has unit
//! tests for the happy paths. This file adds the edge cases surfaced during
//! research:
//!
//! - **Tier-rarity split.** Lesser/Normal/Greater promote Magic→Rare (add);
//!   Perfect/Corrupted remove-then-add on Rare.
//! - **Family collision.** A Perfect essence whose mod-group already exists
//!   on the item (after the forced removal) is rejected.
//! - **Affix-full forcing + Crystallisation.** When the essence mod is a
//!   suffix and a Dextral Crystallisation omen forces suffix removal, the
//!   removed mod is a suffix.
//! - **Corrupted-item gating.** Only Corrupted essences apply to corrupted
//!   items.

use poc2_engine::currency::{Essence, EssenceQuality, RaritySet};
use poc2_engine::ids::{ItemClassId, ModGroupId, ModId, StatId, TagId};
use poc2_engine::omen::{Omen, OmenSet};
use poc2_engine::patch::PatchVersion;
use poc2_engine::{
    AffixType, ApplyContext, BaseTypeId, Currency, EngineError, Item, ModDefinition, ModDomain,
    ModFlags, ModGroup, ModKind, ModRegistry, ModRoll, ModStat, PatchRange, QualityKind, Rarity,
    SpawnWeight,
};
use rand::SeedableRng;
use rand_xoshiro::Xoshiro256PlusPlus;
use smallvec::smallvec;

const PATCH: PatchVersion = PatchVersion::PATCH_0_4_0;

fn mk_mod(id: &str, group: &str, affix: AffixType, req: u32) -> ModDefinition {
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
            tag: TagId::from("any"),
            weight: 1
        }],
        stats: smallvec![ModStat {
            stat_id: StatId::from("s"),
            min: 1.0,
            max: 10.0,
        }],
        required_level: req,
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
            mk_mod("EssLife", "Life", AffixType::Prefix, 1),
            mk_mod("EssSuffix", "Resist", AffixType::Suffix, 1),
            mk_mod("OtherPrefix", "Armour", AffixType::Prefix, 1),
            mk_mod("OtherSuffix", "Stun", AffixType::Suffix, 1),
            mk_mod("LifeExisting", "Life", AffixType::Prefix, 1),
            // Extra fresh-group fillers used by the overflow/crystallisation
            // regression tests (3-prefix Rare scenarios). Each is its own
            // group so no exclusivity collision occurs.
            mk_mod("PrefixA", "GroupA", AffixType::Prefix, 1),
            mk_mod("PrefixB", "GroupB", AffixType::Prefix, 1),
            mk_mod("PrefixC", "GroupC", AffixType::Prefix, 1),
            // A prefix essence mod in a brand-new group (no collision with the
            // fillers above) used to prove the >=3 capacity / overflow logic.
            mk_mod("EssPrefixFresh", "FreshPrefix", AffixType::Prefix, 1),
        ],
        vec![],
    )
}

fn ctx<'a>(
    reg: &'a ModRegistry,
    rng: &'a mut Xoshiro256PlusPlus,
    omens: &'a mut OmenSet,
) -> ApplyContext<'a> {
    ApplyContext::new_without_bases(reg, rng, PATCH, omens)
}

fn magic_item() -> Item {
    Item {
        base: BaseTypeId::from("BodyArmour"),
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

fn roll(id: &str, affix: AffixType) -> ModRoll {
    ModRoll {
        mod_id: ModId::from(id),
        affix_type: affix,
        kind: ModKind::Explicit,
        values: smallvec![5.0],
        is_fractured: false,
    }
}

#[test]
fn greater_essence_promotes_magic_to_rare_and_adds_mod() {
    let reg = registry();
    let mut rng = Xoshiro256PlusPlus::seed_from_u64(1);
    let mut omens = OmenSet::new();
    let mut item = magic_item();
    item.prefixes.push(roll("OtherPrefix", AffixType::Prefix));
    Essence::new("E", "Greater Essence", EssenceQuality::Greater, "EssSuffix")
        .apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens))
        .unwrap();
    assert_eq!(item.rarity, Rarity::Rare);
    assert!(item
        .suffixes
        .iter()
        .any(|m| m.mod_id.as_str() == "EssSuffix"));
    // Existing mod preserved.
    assert!(item
        .prefixes
        .iter()
        .any(|m| m.mod_id.as_str() == "OtherPrefix"));
}

#[test]
fn greater_essence_rejects_non_magic() {
    let reg = registry();
    let mut rng = Xoshiro256PlusPlus::seed_from_u64(2);
    let mut omens = OmenSet::new();
    let mut item = magic_item();
    item.rarity = Rarity::Rare;
    let r = Essence::new("E", "Greater Essence", EssenceQuality::Greater, "EssSuffix")
        .apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens));
    assert!(matches!(r, Err(EngineError::InvalidApplication(_))));
}

#[test]
fn perfect_essence_family_collision_is_rejected() {
    // Item already has a Life-group mod; a Perfect essence adding another
    // Life-group mod must be rejected (after the forced removal there's
    // still a Life mod surviving).
    let reg = registry();
    let mut rng = Xoshiro256PlusPlus::seed_from_u64(3);
    let mut omens = OmenSet::new();
    let mut item = magic_item();
    item.rarity = Rarity::Rare;
    // Two prefixes, both occupying groups; one is Life.
    item.prefixes.push(roll("LifeExisting", AffixType::Prefix));
    item.prefixes.push(roll("OtherPrefix", AffixType::Prefix));
    item.suffixes.push(roll("OtherSuffix", AffixType::Suffix));
    // Force removal to a suffix via Dextral Crystallisation so the Life
    // prefix survives, guaranteeing the family collision.
    omens.push(Omen::dextral_crystallisation());
    let r = Essence::new(
        "E",
        "Perfect Essence of Life",
        EssenceQuality::Perfect,
        "EssLife",
    )
    .apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens));
    assert!(
        matches!(r, Err(EngineError::ModGroupExclusive(_))),
        "Perfect essence adding a 2nd Life-group mod must be rejected; got {r:?}"
    );
}

#[test]
fn dextral_crystallisation_forces_suffix_removal() {
    // Perfect essence adds a prefix (EssLife). With Dextral Crystallisation,
    // the removed mod must be a SUFFIX, leaving both prefixes intact +
    // adding the new prefix would exceed 3... so use 2 prefixes + 1 suffix.
    let reg = registry();
    let mut rng = Xoshiro256PlusPlus::seed_from_u64(4);
    let mut omens = OmenSet::new();
    let mut item = magic_item();
    item.rarity = Rarity::Rare;
    item.prefixes.push(roll("OtherPrefix", AffixType::Prefix));
    item.suffixes.push(roll("OtherSuffix", AffixType::Suffix));
    omens.push(Omen::dextral_crystallisation());
    Essence::new(
        "E",
        "Perfect Essence of Life",
        EssenceQuality::Perfect,
        "EssLife",
    )
    .apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens))
    .unwrap();
    // The suffix was removed; the prefix essence mod was added.
    assert!(
        item.suffixes.is_empty(),
        "Dextral Crystallisation must have removed the suffix"
    );
    assert!(item.prefixes.iter().any(|m| m.mod_id.as_str() == "EssLife"));
}

#[test]
fn corrupted_item_only_accepts_corrupted_essence() {
    let reg = registry();
    let mut rng = Xoshiro256PlusPlus::seed_from_u64(5);
    let mut omens = OmenSet::new();
    let mut item = magic_item();
    item.rarity = Rarity::Rare;
    item.corrupted = true;
    item.prefixes.push(roll("OtherPrefix", AffixType::Prefix));

    // Perfect essence rejected on corrupted item.
    let r = Essence::new("E", "Perfect", EssenceQuality::Perfect, "EssSuffix")
        .apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens));
    assert!(matches!(r, Err(EngineError::ItemCorrupted)));

    // Corrupted essence accepted.
    let mut omens2 = OmenSet::new();
    Essence::new("E", "Corrupted", EssenceQuality::Corrupted, "EssSuffix")
        .apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens2))
        .expect("Corrupted essence must apply to a corrupted item");
    assert_eq!(
        item.suffixes
            .iter()
            .find(|m| m.mod_id.as_str() == "EssSuffix")
            .map(|m| m.kind),
        Some(ModKind::Corrupted),
        "Corrupted essence's mod must be tagged Corrupted"
    );
}

// ---------------------------------------------------------------------------
// Extension: capacity / overflow regression + valid_rarities coverage.
// ---------------------------------------------------------------------------

#[test]
fn greater_essence_on_magic_with_existing_prefix_adds_second_prefix() {
    // Magic item with one existing prefix (Armour group), Greater essence
    // whose target is a PREFIX of a fresh group (Life). The promoting path
    // checks `prefixes.len() >= 3`; with only 1 prefix this is well under
    // capacity, so the add succeeds and the item becomes Rare with 2
    // prefixes. Proves the >=3 capacity check is correct (NOT off-by-one).
    let reg = registry();
    let mut rng = Xoshiro256PlusPlus::seed_from_u64(10);
    let mut omens = OmenSet::new();
    let mut item = magic_item();
    item.prefixes.push(roll("OtherPrefix", AffixType::Prefix));

    Essence::new(
        "E",
        "Greater Essence of Life",
        EssenceQuality::Greater,
        "EssLife",
    )
    .apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens))
    .expect("Greater essence on a 1-prefix Magic item must succeed");

    assert_eq!(item.rarity, Rarity::Rare);
    assert_eq!(item.prefixes.len(), 2, "existing prefix + essence prefix");
    assert!(item
        .prefixes
        .iter()
        .any(|m| m.mod_id.as_str() == "OtherPrefix"));
    assert!(item.prefixes.iter().any(|m| m.mod_id.as_str() == "EssLife"));
}

#[test]
fn promoting_essence_rejects_target_group_already_present() {
    // Greater essence whose target mod's group (Life) is already present on
    // the Magic item must be rejected with ModGroupExclusive.
    let reg = registry();
    let mut rng = Xoshiro256PlusPlus::seed_from_u64(11);
    let mut omens = OmenSet::new();
    let mut item = magic_item();
    // Existing Life-group prefix already on the item.
    item.prefixes.push(roll("LifeExisting", AffixType::Prefix));

    let r = Essence::new(
        "E",
        "Greater Essence of Life",
        EssenceQuality::Greater,
        "EssLife",
    )
    .apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens));

    assert!(
        matches!(r, Err(EngineError::ModGroupExclusive(_))),
        "Greater essence into an occupied group must be rejected; got {r:?}"
    );
}

#[test]
fn perfect_essence_sinistral_crystallisation_removes_only_prefix() {
    // Mirror of the dextral test, flipped: Sinistral Crystallisation forces
    // the removal to a PREFIX. The essence's target is also a prefix
    // (EssLife). Start with 2 prefixes + 1 suffix so the suffix survives and
    // a prefix is removed/replaced.
    let reg = registry();
    let mut rng = Xoshiro256PlusPlus::seed_from_u64(12);
    let mut omens = OmenSet::new();
    let mut item = magic_item();
    item.rarity = Rarity::Rare;
    item.prefixes.push(roll("OtherPrefix", AffixType::Prefix));
    item.prefixes.push(roll("PrefixA", AffixType::Prefix));
    item.suffixes.push(roll("OtherSuffix", AffixType::Suffix));
    omens.push(Omen::sinistral_crystallisation());

    Essence::new(
        "E",
        "Perfect Essence of Life",
        EssenceQuality::Perfect,
        "EssLife",
    )
    .apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens))
    .expect("Perfect essence with Sinistral Crystallisation must succeed");

    // The suffix survives untouched.
    assert_eq!(
        item.suffixes.len(),
        1,
        "Sinistral Crystallisation must leave the suffix alone"
    );
    assert!(item
        .suffixes
        .iter()
        .any(|m| m.mod_id.as_str() == "OtherSuffix"));
    // A prefix was removed and the essence prefix added: net prefix count is
    // unchanged at 2, and the essence mod is present.
    assert_eq!(item.prefixes.len(), 2);
    assert!(item.prefixes.iter().any(|m| m.mod_id.as_str() == "EssLife"));
}

#[test]
fn perfect_essence_no_overflow_when_target_side_full() {
    // Regression for the overflow fix: a Rare with 3 prefixes + 1 suffix,
    // no omen, and a Perfect essence whose target is a PREFIX. With the
    // prefix side already full and no Crystallisation, the removal is
    // constrained to the prefix side so the new prefix has room. The result
    // must never reach 4 prefixes, and the net mod count is unchanged.
    let reg = registry();
    let mut rng = Xoshiro256PlusPlus::seed_from_u64(13);
    let mut omens = OmenSet::new();
    let mut item = magic_item();
    item.rarity = Rarity::Rare;
    item.prefixes.push(roll("PrefixA", AffixType::Prefix));
    item.prefixes.push(roll("PrefixB", AffixType::Prefix));
    item.prefixes.push(roll("PrefixC", AffixType::Prefix));
    item.suffixes.push(roll("OtherSuffix", AffixType::Suffix));

    Essence::new(
        "E",
        "Perfect Essence (fresh prefix)",
        EssenceQuality::Perfect,
        "EssPrefixFresh",
    )
    .apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens))
    .expect("Perfect essence on a full-prefix Rare must succeed without overflow");

    assert!(
        item.prefixes.len() <= 3,
        "prefix side must never overflow to 4; got {}",
        item.prefixes.len()
    );
    assert_eq!(
        item.prefixes.len() + item.suffixes.len(),
        4,
        "net mod count is unchanged (remove 1, add 1)"
    );
    assert!(item
        .prefixes
        .iter()
        .any(|m| m.mod_id.as_str() == "EssPrefixFresh"));
    // The suffix was never a candidate (target side was full), so it survives.
    assert!(item
        .suffixes
        .iter()
        .any(|m| m.mod_id.as_str() == "OtherSuffix"));
}

#[test]
fn perfect_essence_crystallisation_contradiction_errors() {
    // Rare with 3 prefixes + 1 suffix. Dextral Crystallisation forces the
    // removal onto the SUFFIX side, but the essence's target is a PREFIX and
    // the prefix side is already full. The removal cannot free a prefix
    // slot, so apply() must return AffixSlotFull rather than overflow.
    let reg = registry();
    let mut rng = Xoshiro256PlusPlus::seed_from_u64(14);
    let mut omens = OmenSet::new();
    let mut item = magic_item();
    item.rarity = Rarity::Rare;
    item.prefixes.push(roll("PrefixA", AffixType::Prefix));
    item.prefixes.push(roll("PrefixB", AffixType::Prefix));
    item.prefixes.push(roll("PrefixC", AffixType::Prefix));
    item.suffixes.push(roll("OtherSuffix", AffixType::Suffix));
    omens.push(Omen::dextral_crystallisation());

    let r = Essence::new(
        "E",
        "Perfect Essence (fresh prefix)",
        EssenceQuality::Perfect,
        "EssPrefixFresh",
    )
    .apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens));

    assert!(
        matches!(r, Err(EngineError::AffixSlotFull { .. })),
        "full prefix side + suffix-forcing Crystallisation must error; got {r:?}"
    );
}

#[test]
fn essences_reject_sanctified_item() {
    let reg = registry();
    let mut rng = Xoshiro256PlusPlus::seed_from_u64(15);

    // Sanctified Rare → ItemSanctified.
    let mut omens = OmenSet::new();
    let mut item = magic_item();
    item.rarity = Rarity::Rare;
    item.sanctified = true;
    item.prefixes.push(roll("OtherPrefix", AffixType::Prefix));
    let r = Essence::new("E", "Perfect", EssenceQuality::Perfect, "EssSuffix")
        .apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens));
    assert!(
        matches!(r, Err(EngineError::ItemSanctified)),
        "sanctified item must reject essences; got {r:?}"
    );

    // Mirrored Rare → InvalidApplication.
    let mut omens2 = OmenSet::new();
    let mut mirrored = magic_item();
    mirrored.rarity = Rarity::Rare;
    mirrored.mirrored = true;
    mirrored
        .prefixes
        .push(roll("OtherPrefix", AffixType::Prefix));
    let r2 = Essence::new("E", "Perfect", EssenceQuality::Perfect, "EssSuffix")
        .apply(&mut mirrored, &mut ctx(&reg, &mut rng, &mut omens2));
    assert!(
        matches!(r2, Err(EngineError::InvalidApplication(_))),
        "mirrored item must reject essences; got {r2:?}"
    );
}

#[test]
fn essence_quality_valid_rarities() {
    // Lesser / Normal / Greater all "upgrade a Magic item to a Rare item,
    // adding a guaranteed modifier" (PoE2 0.3 essence rework, stable 0.5;
    // wiki/poe2db), so all three apply to MAGIC. Perfect / Corrupted apply to
    // Rare (remove-then-add). (valid_rarities is independent of the registry.)
    assert_eq!(
        Essence::new("E", "L", EssenceQuality::Lesser, "EssSuffix").valid_rarities(),
        RaritySet::MAGIC
    );
    assert_eq!(
        Essence::new("E", "N", EssenceQuality::Normal, "EssSuffix").valid_rarities(),
        RaritySet::MAGIC
    );
    assert_eq!(
        Essence::new("E", "G", EssenceQuality::Greater, "EssSuffix").valid_rarities(),
        RaritySet::MAGIC
    );
    assert_eq!(
        Essence::new("E", "P", EssenceQuality::Perfect, "EssSuffix").valid_rarities(),
        RaritySet::RARE
    );
    assert_eq!(
        Essence::new("E", "C", EssenceQuality::Corrupted, "EssSuffix").valid_rarities(),
        RaritySet::RARE
    );
}

#[test]
fn lesser_and_normal_essences_upgrade_magic_to_rare() {
    // The previously-unreachable success path: Lesser/Normal essences (like
    // Greater) apply to a MAGIC item and upgrade it to Rare with the
    // guaranteed mod, preserving the existing magic mods. Verified against the
    // wiki ("Upgrades a Magic item to a Rare item, adding a guaranteed
    // modifier"). Before the valid_rarities fix this could never succeed.
    for quality in [EssenceQuality::Lesser, EssenceQuality::Normal] {
        let reg = registry();
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0x1e55e);
        let mut omens = OmenSet::new();
        let mut item = magic_item();
        item.suffixes.push(roll("OtherSuffix", AffixType::Suffix));

        Essence::new("E", "X", quality, "EssLife")
            .apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens))
            .unwrap_or_else(|e| panic!("{quality:?} essence on a Magic item must succeed: {e:?}"));

        assert_eq!(
            item.rarity,
            Rarity::Rare,
            "{quality:?} must upgrade Magic → Rare"
        );
        assert!(
            item.prefixes.iter().any(|m| m.mod_id.as_str() == "EssLife"),
            "{quality:?} must add its guaranteed mod"
        );
        assert!(
            item.suffixes
                .iter()
                .any(|m| m.mod_id.as_str() == "OtherSuffix"),
            "{quality:?} must preserve the existing magic mod"
        );
    }
}

#[test]
fn lesser_essence_rejects_non_magic_item() {
    // The rarity gate (can_apply_to → valid_rarities) and apply() both require
    // Magic now: a Lesser essence on a Normal or Rare item is refused both
    // pre-flight and at apply time.
    let reg = registry();
    for rarity in [Rarity::Normal, Rarity::Rare] {
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0x2);
        let mut omens = OmenSet::new();
        let mut item = magic_item();
        item.rarity = rarity;
        let ess = Essence::new("E", "X", EssenceQuality::Lesser, "EssLife");
        assert!(
            ess.can_apply_to(&item).is_err(),
            "Lesser must reject {rarity:?} pre-flight (can_apply_to)"
        );
        let r = ess.apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens));
        assert!(
            matches!(r, Err(EngineError::InvalidApplication(_))),
            "Lesser must reject {rarity:?} at apply; got {r:?}"
        );
    }
}
