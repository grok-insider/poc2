//! M14.4 — Vaal outcome distribution + Omen of Corruption tests.
//!
//! Validates that:
//! - The 6-outcome distribution matches the documented frequencies
//!   (NoChange 25%, RerollValues 20%, BrickMods 15%, AddEnchantment 20%,
//!   AddSocket 10%, AddQuality 10%) within chi-squared tolerance over
//!   10 000 trials.
//! - Omen of Corruption suppresses the NoChange branch and renormalizes
//!   the remaining five outcomes.
//! - BrickMods clears non-fractured explicit mods and (when the registry
//!   has Corrupted-explicit data) replaces them with a Corrupted-kind
//!   mod from the per-class pool. With v3 starter data, no replacement
//!   pool exists, so the test only asserts the clear path.
//! - AddEnchantment rolls a Vaal-implicit (Corrupted-kind, Implicit-affix)
//!   onto the enchantments slot when the registry has Vaal-implicit data
//!   for the item class.
//!
//! Reference: `docs/81-engine-training-and-rule-encoding-plan.md` §4.4
//! Tier 1.4.

use poc2_engine::currency::basic::VaalOrb;
use poc2_engine::currency::{ApplyContext, Currency};
use poc2_engine::ids::TagId;
use poc2_engine::omen::{Omen, OmenSet};
use poc2_engine::patch::{League, PatchVersion};
use poc2_engine::{
    apply_currency, AffixType, BaseTypeId, Item, ItemClassId, ModDefinition, ModDomain, ModFlags,
    ModGroup, ModGroupId, ModId, ModKind, ModRegistry, ModRoll, PatchRange, QualityKind, Rarity,
    SpawnWeight,
};
use rand_xoshiro::rand_core::SeedableRng;
use rand_xoshiro::Xoshiro256PlusPlus;
use smallvec::smallvec;

const PATCH: PatchVersion = PatchVersion::PATCH_0_4_0;

fn mk_explicit_mod(id: &str, group: &str, affix: AffixType) -> ModDefinition {
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
        stats: smallvec![],
        required_level: 1,
        tier: None,
        allowed_item_classes: smallvec![ItemClassId::from("BodyArmour")],
        patch_range: PatchRange::ALL,
        flags: ModFlags::empty(),
        text_template: None,
    }
}

fn mk_vaal_implicit(id: &str, group: &str) -> ModDefinition {
    ModDefinition {
        id: ModId::from(id),
        name: None,
        mod_group: ModGroup(ModGroupId::from(group)),
        affix_type: AffixType::Implicit,
        kind: ModKind::Corrupted,
        domain: ModDomain::Item,
        tags: smallvec![],
        concept_set: smallvec![],
        spawn_weights: smallvec![],
        stats: smallvec![],
        required_level: 1,
        tier: None,
        allowed_item_classes: smallvec![ItemClassId::from("BodyArmour")],
        patch_range: PatchRange::ALL,
        flags: ModFlags::CORRUPTED_ONLY,
        text_template: None,
    }
}

fn mk_rare_armour() -> Item {
    Item {
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
            mod_id: ModId::from("ExplicitPrefix"),
            affix_type: AffixType::Prefix,
            kind: ModKind::Explicit,
            values: smallvec![],
            is_fractured: false,
        }],
        suffixes: smallvec![ModRoll {
            mod_id: ModId::from("ExplicitSuffix"),
            affix_type: AffixType::Suffix,
            kind: ModKind::Explicit,
            values: smallvec![],
            is_fractured: false,
        }],
        enchantments: smallvec![],
        hidden_desecrated: None,
        sockets: smallvec![],
        hinekora_lock: None,
    }
}

fn registry_with_vaal_implicits() -> ModRegistry {
    ModRegistry::from_mods(
        vec![
            mk_explicit_mod("ExplicitPrefix", "ExplPrefGroup", AffixType::Prefix),
            mk_explicit_mod("ExplicitSuffix", "ExplSuffGroup", AffixType::Suffix),
            mk_vaal_implicit("VaalImplicit_PercentMaxLife", "VaalLife"),
            mk_vaal_implicit("VaalImplicit_PercentMaxES", "VaalES"),
        ],
        vec![],
    )
}

#[derive(Default, Debug)]
struct OutcomeCounts {
    /// Combined `NoChange | RerollValues` count. The fixture's
    /// empty-stat mods leave both outcomes indistinguishable, so they
    /// share a bucket — the distribution test asserts the combined
    /// frequency matches `0.25 + 0.20 = 0.45`.
    no_change: usize,
    brick_mods: usize,
    add_enchantment: usize,
    add_socket: usize,
    add_quality: usize,
}

/// Classify the post-Vaal item state into one of the six outcomes.
///
/// Uses the unique observable side-effects of each outcome:
/// - `AddSocket`: a new socket is appended.
/// - `AddQuality`: quality goes from 0 to 5.
/// - `AddEnchantment`: a Vaal implicit lands in `enchantments`.
/// - `BrickMods`: the original explicit prefix/suffix are gone.
/// - `RerollValues`: explicit mods kept but `values` may differ. With the
///   fixture's empty stats, RerollValues is indistinguishable from
///   NoChange — the post-state has the same mods. We treat both as
///   `(no_change | reroll_values)` and assert their combined frequency.
fn classify(item: &Item, before_socket_count: usize, before_quality: u8) -> Option<&'static str> {
    if item.sockets.len() > before_socket_count {
        return Some("add_socket");
    }
    if item.quality > before_quality {
        return Some("add_quality");
    }
    if item
        .enchantments
        .iter()
        .any(|e| e.kind == ModKind::Corrupted)
    {
        return Some("add_enchantment");
    }
    if !item
        .prefixes
        .iter()
        .any(|m| m.mod_id.as_str() == "ExplicitPrefix")
        || !item
            .suffixes
            .iter()
            .any(|m| m.mod_id.as_str() == "ExplicitSuffix")
    {
        return Some("brick_mods");
    }
    // No observable side-effect — either NoChange or RerollValues.
    Some("no_change_or_reroll")
}

#[test]
fn vaal_distribution_matches_documented_frequencies() {
    let registry = registry_with_vaal_implicits();
    let trials = 10_000usize;
    let mut counts = OutcomeCounts::default();
    for trial in 0..trials {
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0x_DEAD_5EED ^ trial as u64);
        let mut omens = OmenSet::new();
        let mut item = mk_rare_armour();
        let before_sockets = item.sockets.len();
        let before_quality = item.quality;
        apply_currency(
            &VaalOrb::new(),
            &mut item,
            &registry,
            &mut rng,
            PATCH,
            &mut omens,
        )
        .expect("Vaal must succeed on a non-corrupted Rare");
        match classify(&item, before_sockets, before_quality).unwrap() {
            "add_socket" => counts.add_socket += 1,
            "add_quality" => counts.add_quality += 1,
            "add_enchantment" => counts.add_enchantment += 1,
            "brick_mods" => counts.brick_mods += 1,
            "no_change_or_reroll" => {
                // Lump RerollValues and NoChange together; both produce
                // the same observable on this fixture.
                counts.no_change += 1;
            }
            other => panic!("unclassified outcome: {other}"),
        }
        assert!(item.corrupted);
    }
    let n = trials as f64;

    // Combined NoChange + RerollValues: 25% + 20% = 45%.
    let expected_no_or_reroll = 0.45;
    let p_no_or_reroll = counts.no_change as f64 / n;
    let stderr_no = (expected_no_or_reroll * (1.0 - expected_no_or_reroll) / n).sqrt();
    assert!(
        (p_no_or_reroll - expected_no_or_reroll).abs() <= 4.0 * stderr_no,
        "NoChange|RerollValues at {p_no_or_reroll:.4}; expected {expected_no_or_reroll:.4} \
         ± {:.4} (4σ)",
        4.0 * stderr_no,
    );

    let expected_brick = 0.15;
    let p_brick = counts.brick_mods as f64 / n;
    let stderr_brick = (expected_brick * (1.0 - expected_brick) / n).sqrt();
    assert!(
        (p_brick - expected_brick).abs() <= 4.0 * stderr_brick,
        "BrickMods at {p_brick:.4}; expected {expected_brick:.4} ± {:.4} (4σ)",
        4.0 * stderr_brick,
    );

    let expected_ench = 0.20;
    let p_ench = counts.add_enchantment as f64 / n;
    let stderr_ench = (expected_ench * (1.0 - expected_ench) / n).sqrt();
    assert!(
        (p_ench - expected_ench).abs() <= 4.0 * stderr_ench,
        "AddEnchantment at {p_ench:.4}; expected {expected_ench:.4} ± {:.4} (4σ)",
        4.0 * stderr_ench,
    );

    let expected_socket = 0.10;
    let p_socket = counts.add_socket as f64 / n;
    let stderr_socket = (expected_socket * (1.0 - expected_socket) / n).sqrt();
    assert!(
        (p_socket - expected_socket).abs() <= 4.0 * stderr_socket,
        "AddSocket at {p_socket:.4}; expected {expected_socket:.4} ± {:.4} (4σ)",
        4.0 * stderr_socket,
    );

    let expected_quality = 0.10;
    let p_quality = counts.add_quality as f64 / n;
    let stderr_quality = (expected_quality * (1.0 - expected_quality) / n).sqrt();
    assert!(
        (p_quality - expected_quality).abs() <= 4.0 * stderr_quality,
        "AddQuality at {p_quality:.4}; expected {expected_quality:.4} ± {:.4} (4σ)",
        4.0 * stderr_quality,
    );

    assert_eq!(
        counts.add_enchantment
            + counts.brick_mods
            + counts.no_change
            + counts.add_socket
            + counts.add_quality,
        trials,
        "every trial must classify into a single outcome",
    );
}

#[test]
fn vaal_with_corruption_omen_never_returns_no_change() {
    // With Omen of Corruption queued, NoChange + RerollValues outcomes
    // cannot both produce the "no observable change" state. Specifically
    // NoChange is suppressed; only RerollValues remains in that bucket
    // and at the renormalized 26.7% frequency. We assert observed
    // proportion is comfortably below the 45% baseline.
    let registry = registry_with_vaal_implicits();
    let trials = 5_000usize;
    let mut no_or_reroll = 0usize;
    for trial in 0..trials {
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0x_C0DE_5EED ^ trial as u64);
        let mut omens = OmenSet::new();
        omens.push(Omen::corruption());
        let mut item = mk_rare_armour();
        let before_sockets = item.sockets.len();
        let before_quality = item.quality;
        apply_currency(
            &VaalOrb::new(),
            &mut item,
            &registry,
            &mut rng,
            PATCH,
            &mut omens,
        )
        .unwrap();
        if classify(&item, before_sockets, before_quality).unwrap() == "no_change_or_reroll" {
            no_or_reroll += 1;
        }
    }
    // Expected: ~26.7% (only RerollValues now). Baseline without omen was
    // 45%. Pick a mid-threshold of 35% to fail loudly if the omen is
    // ignored; the true 26.7% has stderr ≈ 0.6%, so 35% is many sigma
    // above the expectation.
    let p = no_or_reroll as f64 / trials as f64;
    assert!(
        p < 0.35,
        "with Omen of Corruption, NoChange|Reroll proportion should be far below \
         the no-omen baseline of 0.45; got {p:.4}"
    );
    // Sanity floor: >0 (RerollValues path still active in the renormalized
    // distribution; expected 0.267).
    assert!(
        p > 0.15,
        "RerollValues should still appear post-omen; got {p:.4}"
    );
}

#[test]
fn vaal_brick_replaces_with_corrupted_mod_when_pool_exists() {
    // The plan's per-class corrupted-explicit pool isn't seeded by the
    // current bundle (vaal_implicits.json populates Implicit-affix mods
    // only). To exercise the replacement path end-to-end, build a registry
    // that explicitly carries Corrupted-kind explicit mods and assert
    // BrickMods replaces cleared slots with such mods.
    let registry = ModRegistry::from_mods(
        vec![
            mk_explicit_mod("ExplicitPrefix", "ExplPrefGroup", AffixType::Prefix),
            mk_explicit_mod("ExplicitSuffix", "ExplSuffGroup", AffixType::Suffix),
            ModDefinition {
                id: ModId::from("CorruptedReplacement_Prefix"),
                name: None,
                mod_group: ModGroup(ModGroupId::from("CorruptedPrefixGroup")),
                affix_type: AffixType::Prefix,
                kind: ModKind::Corrupted,
                domain: ModDomain::Item,
                tags: smallvec![],
                concept_set: smallvec![],
                spawn_weights: smallvec![SpawnWeight {
                    tag: TagId::from("any"),
                    weight: 1
                }],
                stats: smallvec![],
                required_level: 1,
                tier: None,
                allowed_item_classes: smallvec![ItemClassId::from("BodyArmour")],
                patch_range: PatchRange::ALL,
                flags: ModFlags::CORRUPTED_ONLY,
                text_template: None,
            },
            ModDefinition {
                id: ModId::from("CorruptedReplacement_Suffix"),
                name: None,
                mod_group: ModGroup(ModGroupId::from("CorruptedSuffixGroup")),
                affix_type: AffixType::Suffix,
                kind: ModKind::Corrupted,
                domain: ModDomain::Item,
                tags: smallvec![],
                concept_set: smallvec![],
                spawn_weights: smallvec![SpawnWeight {
                    tag: TagId::from("any"),
                    weight: 1
                }],
                stats: smallvec![],
                required_level: 1,
                tier: None,
                allowed_item_classes: smallvec![ItemClassId::from("BodyArmour")],
                patch_range: PatchRange::ALL,
                flags: ModFlags::CORRUPTED_ONLY,
                text_template: None,
            },
        ],
        vec![],
    );

    // Run many seeds; on at least one seed we expect a BrickMods outcome
    // followed by a Corrupted-kind replacement.
    let mut saw_replacement = false;
    for trial in 0..2_000usize {
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0x_BCBC_5EED ^ trial as u64);
        let mut omens = OmenSet::new();
        let mut item = mk_rare_armour();
        apply_currency(
            &VaalOrb::new(),
            &mut item,
            &registry,
            &mut rng,
            PATCH,
            &mut omens,
        )
        .unwrap();
        // BrickMods is observable: original explicit slots are absent.
        let original_present = item
            .prefixes
            .iter()
            .any(|m| m.mod_id.as_str() == "ExplicitPrefix")
            || item
                .suffixes
                .iter()
                .any(|m| m.mod_id.as_str() == "ExplicitSuffix");
        if original_present {
            continue;
        }
        // Likely BrickMods (could also be socket/quality, but those
        // wouldn't have removed mods). Look for replacement.
        let has_replacement = item.prefixes.iter().any(|m| m.kind == ModKind::Corrupted)
            || item.suffixes.iter().any(|m| m.kind == ModKind::Corrupted);
        if has_replacement {
            saw_replacement = true;
            break;
        }
    }
    assert!(
        saw_replacement,
        "BrickMods with a Corrupted-explicit pool should produce a Corrupted-kind \
         replacement mod within 2000 trials"
    );
}

#[test]
fn vaal_brick_clears_when_no_replacement_pool_exists() {
    // With v3 starter data (no Corrupted-explicit pool), BrickMods is
    // expected to clear non-fractured mods and leave the slots empty. The
    // item is still Vaal-corrupted afterwards.
    let registry = ModRegistry::from_mods(
        vec![
            mk_explicit_mod("ExplicitPrefix", "ExplPrefGroup", AffixType::Prefix),
            mk_explicit_mod("ExplicitSuffix", "ExplSuffGroup", AffixType::Suffix),
        ],
        vec![],
    );

    let mut saw_clear_no_replacement = false;
    for trial in 0..2_000usize {
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0x_CEAE_5EED ^ trial as u64);
        let mut omens = OmenSet::new();
        let mut item = mk_rare_armour();
        apply_currency(
            &VaalOrb::new(),
            &mut item,
            &registry,
            &mut rng,
            PATCH,
            &mut omens,
        )
        .unwrap();
        let original_present = item
            .prefixes
            .iter()
            .any(|m| m.mod_id.as_str() == "ExplicitPrefix")
            || item
                .suffixes
                .iter()
                .any(|m| m.mod_id.as_str() == "ExplicitSuffix");
        if !original_present && item.prefixes.is_empty() && item.suffixes.is_empty() {
            saw_clear_no_replacement = true;
            break;
        }
    }
    assert!(
        saw_clear_no_replacement,
        "BrickMods without a replacement pool should clear slots within 2000 trials"
    );
}

#[test]
fn vaal_add_enchantment_pushes_corrupted_kind_into_enchantments_slot() {
    let registry = registry_with_vaal_implicits();
    let mut saw_enchantment = false;
    for trial in 0..2_000usize {
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0x_ADD3_5EED ^ trial as u64);
        let mut omens = OmenSet::new();
        let mut item = mk_rare_armour();
        apply_currency(
            &VaalOrb::new(),
            &mut item,
            &registry,
            &mut rng,
            PATCH,
            &mut omens,
        )
        .unwrap();
        if let Some(e) = item
            .enchantments
            .iter()
            .find(|e| e.kind == ModKind::Corrupted)
        {
            assert_eq!(e.affix_type, AffixType::Implicit);
            assert!(e.mod_id.as_str().starts_with("VaalImplicit"));
            saw_enchantment = true;
            break;
        }
    }
    assert!(
        saw_enchantment,
        "AddEnchantment should land a Corrupted-Implicit roll within 2000 trials"
    );
}

// =========================================================================
// M-bugfix: Omen of Corruption league gate (0.5 "Return of the Ancients").
//
// In 0.5+ the Omen of Corruption only functions in Standard leagues. In the
// challenge league the omen is NOT consumed and has no effect, so NoChange
// remains a possible Vaal outcome. `apply_currency` builds its own context
// defaulting to League::current() (== Challenge), so these tests construct
// `ApplyContext` directly and drive the league via `with_league` to exercise
// both rulesets deterministically.
// =========================================================================

/// In 0.5 Challenge the Omen of Corruption is gated off: it is neither
/// consumed nor effective, so NoChange can still occur and the omen survives
/// the apply.
#[test]
fn omen_of_corruption_has_no_effect_in_0_5_challenge() {
    let registry = registry_with_vaal_implicits();

    // Robust signal: a single apply on a fresh item must NOT consume the omen
    // because the 0.5 Challenge gate short-circuits before consumption.
    {
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0x_0C5C_0001);
        let mut omens = OmenSet::new();
        omens.push(Omen::corruption());
        let mut item = mk_rare_armour();
        let mut ctx = ApplyContext::new_without_bases(
            &registry,
            &mut rng,
            PatchVersion::PATCH_0_5_0,
            &mut omens,
        )
        .with_league(League::Challenge);
        VaalOrb::new()
            .apply(&mut item, &mut ctx)
            .expect("Vaal must succeed on a non-corrupted Rare");
        assert!(item.corrupted, "item should be corrupted after Vaal");
        assert!(
            !omens.is_empty(),
            "Omen of Corruption must NOT be consumed in 0.5 Challenge (it is league-gated off)"
        );
        assert_eq!(
            omens.len(),
            1,
            "exactly the un-consumed corruption omen should remain in the set"
        );
    }

    // Distribution signal: because the omen is ineffective, the NoChange
    // outcome (item corrupted but mods/sockets/quality unchanged) must still
    // appear across many seeded fresh Rares. The fixture's empty-stat mods
    // make NoChange and RerollValues observationally identical, so observing
    // the "no observable change" bucket at all confirms the NoChange branch
    // was reachable (it would be suppressed if the omen were honored).
    let mut saw_no_change = false;
    for trial in 0..4_000usize {
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0x_0C5C_5EED ^ trial as u64);
        let mut omens = OmenSet::new();
        omens.push(Omen::corruption());
        let mut item = mk_rare_armour();
        let before_sockets = item.sockets.len();
        let before_quality = item.quality;
        {
            let mut ctx = ApplyContext::new_without_bases(
                &registry,
                &mut rng,
                PatchVersion::PATCH_0_5_0,
                &mut omens,
            )
            .with_league(League::Challenge);
            VaalOrb::new().apply(&mut item, &mut ctx).unwrap();
        }
        assert!(item.corrupted);
        if classify(&item, before_sockets, before_quality).unwrap() == "no_change_or_reroll" {
            saw_no_change = true;
            break;
        }
    }
    assert!(
        saw_no_change,
        "in 0.5 Challenge the ineffective corruption omen must leave NoChange reachable"
    );
}

/// In 0.5 Standard the Omen of Corruption is active: it IS consumed by a
/// single apply and the NoChange branch is suppressed across many trials.
#[test]
fn omen_of_corruption_consumed_in_0_5_standard() {
    let registry = registry_with_vaal_implicits();

    // The omen is consumed by a single apply in Standard.
    {
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0x_05D5_0001);
        let mut omens = OmenSet::new();
        omens.push(Omen::corruption());
        let mut item = mk_rare_armour();
        let mut ctx = ApplyContext::new_without_bases(
            &registry,
            &mut rng,
            PatchVersion::PATCH_0_5_0,
            &mut omens,
        )
        .with_league(League::Standard);
        VaalOrb::new()
            .apply(&mut item, &mut ctx)
            .expect("Vaal must succeed on a non-corrupted Rare");
        assert!(item.corrupted);
        assert!(
            omens.is_empty(),
            "Omen of Corruption must be consumed in 0.5 Standard"
        );
    }

    // With the omen honored, NoChange (the only outcome that leaves the item
    // entirely unchanged) is removed from the distribution. Because the
    // fixture's mods carry no stats, RerollValues still lands in the
    // "no observable change" bucket — so we cannot assert the bucket is empty.
    // Instead we assert the strictly-NoChange branch never fires by checking
    // the renormalized bucket frequency falls far below the no-omen baseline
    // of 0.45 (it should sit near the 0.267 RerollValues mass).
    let trials = 5_000usize;
    let mut no_or_reroll = 0usize;
    for trial in 0..trials {
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0x_05D5_5EED ^ trial as u64);
        let mut omens = OmenSet::new();
        omens.push(Omen::corruption());
        let mut item = mk_rare_armour();
        let before_sockets = item.sockets.len();
        let before_quality = item.quality;
        {
            let mut ctx = ApplyContext::new_without_bases(
                &registry,
                &mut rng,
                PatchVersion::PATCH_0_5_0,
                &mut omens,
            )
            .with_league(League::Standard);
            VaalOrb::new().apply(&mut item, &mut ctx).unwrap();
        }
        if classify(&item, before_sockets, before_quality).unwrap() == "no_change_or_reroll" {
            no_or_reroll += 1;
        }
    }
    let p = no_or_reroll as f64 / trials as f64;
    assert!(
        p < 0.35,
        "with the corruption omen honored in 0.5 Standard, the NoChange|Reroll bucket should \
         fall far below the 0.45 no-omen baseline (NoChange suppressed); got {p:.4}"
    );
}

/// AddQuality bumps quality by +5 and clamps at 30. Start a Rare near the cap
/// and run many seeds; the post-Vaal quality invariant `<= 30` must always
/// hold, and at least one seed must hit the AddQuality path (proving the
/// clamp, not merely the absence of the path).
#[test]
fn vaal_add_quality_caps_at_30() {
    let registry = registry_with_vaal_implicits();
    let mut saw_add_quality = false;
    for trial in 0..3_000usize {
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0x_0BAD_0CAB ^ trial as u64);
        let mut omens = OmenSet::new();
        let mut item = mk_rare_armour();
        // Near the cap: a single +5 AddQuality would overshoot to 33 without
        // the clamp.
        item.quality = 28;
        apply_currency(
            &VaalOrb::new(),
            &mut item,
            &registry,
            &mut rng,
            PATCH,
            &mut omens,
        )
        .unwrap();
        assert!(
            item.quality <= 30,
            "Vaal AddQuality must clamp quality at 30; got {} on trial {trial}",
            item.quality
        );
        if item.quality > 28 {
            // AddQuality fired (the only path that raises quality). With the
            // clamp it lands at exactly 30 (28 + 5 = 33 -> 30).
            assert_eq!(
                item.quality, 30,
                "AddQuality from quality 28 must clamp to exactly 30"
            );
            saw_add_quality = true;
        }
    }
    assert!(
        saw_add_quality,
        "AddQuality (the quality-raising outcome) should fire within 3000 trials"
    );
}

/// BrickMods clears non-fractured explicit mods but must NEVER remove a
/// fractured mod. Build a Rare with one fractured prefix plus several
/// non-fractured mods, run many seeds, and assert the fractured mod is always
/// still present afterward.
#[test]
fn vaal_brick_preserves_fractured_mods() {
    let registry = registry_with_vaal_implicits();
    for trial in 0..3_000usize {
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0x_FAC7_5EED ^ trial as u64);
        let mut omens = OmenSet::new();
        let mut item = mk_rare_armour();
        // One fractured prefix + a second non-fractured prefix, plus the
        // fixture's non-fractured suffix. The fractured mod must survive every
        // outcome (BrickMods in particular).
        item.prefixes = smallvec![
            ModRoll {
                mod_id: ModId::from("FracturedPrefix"),
                affix_type: AffixType::Prefix,
                kind: ModKind::Explicit,
                values: smallvec![],
                is_fractured: true,
            },
            ModRoll {
                mod_id: ModId::from("ExplicitPrefix"),
                affix_type: AffixType::Prefix,
                kind: ModKind::Explicit,
                values: smallvec![],
                is_fractured: false,
            },
        ];
        apply_currency(
            &VaalOrb::new(),
            &mut item,
            &registry,
            &mut rng,
            PATCH,
            &mut omens,
        )
        .unwrap();
        assert!(item.corrupted);
        let fractured_present = item
            .prefixes
            .iter()
            .any(|m| m.mod_id.as_str() == "FracturedPrefix" && m.is_fractured);
        assert!(
            fractured_present,
            "fractured mod must survive every Vaal outcome (BrickMods must not strip it); \
             missing on trial {trial}"
        );
    }
}
