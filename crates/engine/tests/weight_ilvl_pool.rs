//! P1 — ilvl-dependent pools + inclusive higher-tier weighting.
//!
//! Validates the two mechanics the user called out:
//!
//! 1. **The pool changes with item level.** A modifier tier is eligible only
//!    when `required_level <= ilvl`; raising ilvl unlocks higher tiers, so
//!    the inclusive weight of a *lower* tier grows once a new top tier
//!    unlocks. This is the "Tyrannical %phys success ~doubles at ilvl 82
//!    once Merciless unlocks" mechanic.
//!
//! 2. **Inclusive higher-tier weighting.** A tier inherits the spawn weight
//!    of the same-group, same-affix higher tiers rollable at the current
//!    ilvl: `effective_weight(m_i) = Σ_{j=m_i}^{m_t0} weight_j`.
//!
//! These assert against [`ModRegistry::inclusive_weight_for`] directly, which
//! is deterministic (no sampling noise) and is exactly what the runtime
//! sampler consumes.

use poc2_engine::ids::TagId;
use poc2_engine::weights::{Confidence, WeightObservation, WeightScope};
use poc2_engine::{
    AffixType, BaseTypeId, ItemClassId, ModDefinition, ModDomain, ModFlags, ModGroup, ModGroupId,
    ModId, ModKind, ModRegistry, PatchRange, SpawnWeight,
};
use smallvec::smallvec;

const CLASS: &str = "Spear";
const BASE: &str = "Spear";

fn prefix_at(id: &str, group: &str, required_level: u32) -> ModDefinition {
    ModDefinition {
        id: ModId::from(id),
        name: None,
        mod_group: ModGroup(ModGroupId::from(group)),
        affix_type: AffixType::Prefix,
        kind: ModKind::Explicit,
        domain: ModDomain::Item,
        tags: smallvec![],
        concept_set: smallvec![],
        spawn_weights: smallvec![SpawnWeight {
            tag: TagId::from("any"),
            weight: 1
        }],
        stats: smallvec![],
        required_level,
        tier: None,
        allowed_item_classes: smallvec![ItemClassId::from(CLASS)],
        patch_range: PatchRange::ALL,
        flags: ModFlags::empty(),
        text_template: None,
    }
}

fn obs(mod_id: &str, weight: f64) -> WeightObservation {
    WeightObservation {
        mod_id: ModId::from(mod_id),
        scope: WeightScope::Base {
            base: BaseTypeId::from(BASE),
        },
        primary_weight: weight,
        secondary_weight: None,
        confidence: Confidence::Community,
        note: None,
    }
}

/// %increased physical damage ladder modeled on the wiki's Spear example,
/// plus a couple of lower tiers so the group is realistic:
/// - Tyrannical: (155-169)% — required_level 73, weight 120
/// - Merciless:  (170-179)% — required_level 82, weight 100
fn phys_ladder() -> ModRegistry {
    let mods = vec![
        prefix_at("Phys_T3", "PhysIncr", 1),
        prefix_at("Phys_T2", "PhysIncr", 45),
        prefix_at("Phys_Tyrannical", "PhysIncr", 73),
        prefix_at("Phys_Merciless", "PhysIncr", 82),
    ];
    let weights = vec![
        obs("Phys_T3", 1000.0),
        obs("Phys_T2", 400.0),
        obs("Phys_Tyrannical", 120.0),
        obs("Phys_Merciless", 100.0),
    ];
    ModRegistry::from_mods(mods, weights)
}

fn incl(registry: &ModRegistry, mod_id: &str, ilvl: u32) -> f64 {
    let m = registry.get(&ModId::from(mod_id)).expect("mod exists");
    registry.inclusive_weight_for(m, &BaseTypeId::from(BASE), ilvl, &ItemClassId::from(CLASS))
}

#[test]
fn tyrannical_inclusive_weight_jumps_when_merciless_unlocks() {
    let r = phys_ladder();

    // At ilvl 73–81, Merciless (82) is not rollable, so Tyrannical's
    // inclusive weight is just its own weight (it is the top rollable tier).
    let at_73 = incl(&r, "Phys_Tyrannical", 73);
    let at_81 = incl(&r, "Phys_Tyrannical", 81);
    assert!(
        (at_73 - 120.0).abs() < 1e-9,
        "Tyrannical inclusive weight at ilvl 73 should be its own 120; got {at_73}"
    );
    assert!(
        (at_81 - 120.0).abs() < 1e-9,
        "Tyrannical inclusive weight should be unchanged at ilvl 81; got {at_81}"
    );

    // At ilvl 82, Merciless unlocks and is a higher tier, so Tyrannical's
    // inclusive weight becomes 120 + 100 = 220 — nearly doubling, exactly the
    // wiki's described behavior.
    let at_82 = incl(&r, "Phys_Tyrannical", 82);
    assert!(
        (at_82 - 220.0).abs() < 1e-9,
        "Tyrannical inclusive weight at ilvl 82 should include Merciless \
         (120+100=220); got {at_82}"
    );
    assert!(
        at_82 > at_73,
        "inclusive weight must increase when a higher tier unlocks"
    );
}

#[test]
fn inclusive_weight_is_monotonic_non_decreasing_in_ilvl() {
    let r = phys_ladder();
    let mut prev = 0.0;
    for ilvl in [1u32, 44, 45, 72, 73, 81, 82, 100] {
        let w = incl(&r, "Phys_T3", ilvl);
        assert!(
            w + 1e-9 >= prev,
            "inclusive weight of the bottom tier must be non-decreasing in \
             ilvl; at ilvl {ilvl} got {w}, previous {prev}"
        );
        prev = w;
    }
}

#[test]
fn bottom_tier_inclusive_weight_sums_whole_rollable_ladder() {
    let r = phys_ladder();
    // At ilvl 100 every tier is rollable; the bottom tier's inclusive weight
    // is the sum of the entire ladder.
    let full = incl(&r, "Phys_T3", 100);
    let expected = 1000.0 + 400.0 + 120.0 + 100.0;
    assert!(
        (full - expected).abs() < 1e-9,
        "bottom tier inclusive weight at high ilvl should equal the full \
         ladder sum {expected}; got {full}"
    );
}

#[test]
fn top_tier_inclusive_weight_is_just_itself() {
    let r = phys_ladder();
    // The strongest tier has no higher peers, so its inclusive weight equals
    // its own weight at any ilvl where it is rollable.
    let top = incl(&r, "Phys_Merciless", 100);
    assert!(
        (top - 100.0).abs() < 1e-9,
        "top tier inclusive weight should be its own weight; got {top}"
    );
}

#[test]
fn tier_required_level_boundary_is_inclusive() {
    let r = phys_ladder();
    // A tier at exactly ilvl == required_level is rollable (so it contributes
    // to lower-tier inclusive weight); one ilvl below it is not.
    // Merciless requires 82.
    let just_below = incl(&r, "Phys_Tyrannical", 81); // Merciless excluded
    let exactly = incl(&r, "Phys_Tyrannical", 82); // Merciless included
    assert!(
        (just_below - 120.0).abs() < 1e-9,
        "at ilvl 81 Merciless must be excluded; got {just_below}"
    );
    assert!(
        (exactly - 220.0).abs() < 1e-9,
        "at ilvl 82 (== Merciless required_level) it must be included; got {exactly}"
    );
}

// ---------------------------------------------------------------------------
// P2 — spawn_weight_for_tags + tier_strength_key (mods.rs public API) and the
// tag-resolved inclusive-weight path (registry.rs on-base variants).
// ---------------------------------------------------------------------------

/// Build a prefix tier whose spawn_weights are a custom ordered tag list,
/// keeping every other field consistent with `prefix_at`.
fn prefix_tagged(
    id: &str,
    group: &str,
    required_level: u32,
    spawn_weights: Vec<SpawnWeight>,
) -> ModDefinition {
    let mut m = prefix_at(id, group, required_level);
    m.spawn_weights = spawn_weights.into_iter().collect();
    m
}

/// `spawn_weight_for_tags` scans the MOD's `spawn_weights` in order and the
/// first entry whose tag is present in `base_tags` wins — "leftmost" is the
/// mod's own ordering, NOT the order of the base's tag list (per the rustdoc
/// on `ModDefinition::spawn_weight_for_tags`). So tagA (first in the mod's
/// list) wins regardless of how the base lists its tags.
#[test]
fn spawn_weight_for_tags_leftmost_tag_wins() {
    let m = prefix_tagged(
        "TagOrder",
        "G",
        1,
        vec![
            SpawnWeight {
                tag: TagId::from("tagA"),
                weight: 1000,
            },
            SpawnWeight {
                tag: TagId::from("tagB"),
                weight: 50,
            },
        ],
    );

    // Base lists tagB before tagA — mod-order still decides, so tagA (1000) wins.
    let base_b_first = m.spawn_weight_for_tags(&[TagId::from("tagB"), TagId::from("tagA")]);
    assert_eq!(
        base_b_first,
        Some(1000),
        "mod-order leftmost (tagA=1000) must win even when the base lists tagB first"
    );

    // Base lists tagA before tagB — same result, confirming base order is irrelevant.
    let base_a_first = m.spawn_weight_for_tags(&[TagId::from("tagA"), TagId::from("tagB")]);
    assert_eq!(
        base_a_first,
        Some(1000),
        "mod-order leftmost (tagA=1000) must win regardless of base tag ordering"
    );

    // Only tagB present → the second mod entry is the first to match → 50.
    let only_b = m.spawn_weight_for_tags(&[TagId::from("tagB")]);
    assert_eq!(
        only_b,
        Some(50),
        "with only tagB present the tagB entry (50) is the leftmost match"
    );
}

/// No overlap between the base's tags and the mod's spawn_weights tags ⇒ the
/// mod cannot roll on this base ⇒ `None` (distinct from an explicit zero).
#[test]
fn spawn_weight_for_tags_no_match_returns_none() {
    let m = prefix_tagged(
        "NoMatch",
        "G",
        1,
        vec![
            SpawnWeight {
                tag: TagId::from("tagA"),
                weight: 1000,
            },
            SpawnWeight {
                tag: TagId::from("tagB"),
                weight: 50,
            },
        ],
    );
    let none = m.spawn_weight_for_tags(&[TagId::from("tagZ"), TagId::from("tagY")]);
    assert_eq!(
        none, None,
        "a base sharing no tag with the mod's spawn_weights yields None"
    );
}

/// A matching tag whose weight is 0 is an *explicit exclusion* and must return
/// `Some(0)` — semantically different from `None` (tag not present at all).
#[test]
fn spawn_weight_for_tags_zero_weight_is_explicit_exclusion() {
    let m = prefix_tagged(
        "Excluded",
        "G",
        1,
        vec![SpawnWeight {
            tag: TagId::from("blocked"),
            weight: 0,
        }],
    );
    let excluded = m.spawn_weight_for_tags(&[TagId::from("blocked")]);
    assert_eq!(
        excluded,
        Some(0),
        "a matching zero-weight tag is an explicit exclusion (Some(0)), not None"
    );
    // And a base without that tag is the "missing" case → None.
    let missing = m.spawn_weight_for_tags(&[TagId::from("other")]);
    assert_eq!(missing, None, "absent tag is None, distinct from Some(0)");
}

/// With `tier == None`, `tier_strength_key` falls back to `required_level`, so
/// the higher-required-level tier has the larger (stronger) key.
#[test]
fn tier_strength_key_falls_back_to_required_level_when_tier_none() {
    let weak = prefix_at("LowReq", "G", 10);
    let strong = prefix_at("HighReq", "G", 80);
    assert!(weak.tier.is_none() && strong.tier.is_none());
    assert_eq!(weak.tier_strength_key(), 10);
    assert_eq!(strong.tier_strength_key(), 80);
    assert!(
        strong.tier_strength_key() > weak.tier_strength_key(),
        "higher required_level must yield the stronger key when tier is None"
    );
}

/// With explicit tier ordinals (1 = strongest), tier 1 must yield a strictly
/// larger key than tier 2 — the ordinal is inverted to `u16::MAX - t`.
#[test]
fn tier_strength_key_uses_explicit_tier_ordinal() {
    let mut t1 = prefix_at("Tier1", "G", 1);
    let mut t2 = prefix_at("Tier2", "G", 1);
    t1.tier = Some(1);
    t2.tier = Some(2);

    let k1 = t1.tier_strength_key();
    let k2 = t2.tier_strength_key();
    assert!(
        k1 > k2,
        "tier 1 (strongest) must yield a stronger key than tier 2; got {k1} vs {k2}"
    );
    // Sanity-check the documented inversion: u16::MAX - ordinal.
    assert_eq!(k1, u32::from(u16::MAX - 1));
    assert_eq!(k2, u32::from(u16::MAX - 2));
}

/// A 3-tier group with NO numeric weight observations (empty weights vec) but
/// with spawn_weights matching a base tag: `inclusive_weight_for_on_base` must
/// fall through to the tag-resolved weights and the weakest tier's inclusive
/// weight equals the sum of the tag-resolved weights of every same-or-stronger
/// rollable tier. tier=None ⇒ strength key = required_level, so the weakest
/// (lowest required_level) tier sums all three.
#[test]
fn inclusive_weight_tag_intersection_no_numeric_weights() {
    let tag = TagId::from("spear_tag");
    let sw = |w: u32| {
        vec![SpawnWeight {
            tag: tag.clone(),
            weight: w,
        }]
    };
    let mods = vec![
        prefix_tagged("TagT3", "TagGroup", 1, sw(300)),
        prefix_tagged("TagT2", "TagGroup", 45, sw(200)),
        prefix_tagged("TagT1", "TagGroup", 73, sw(100)),
    ];
    // Empty weights vec → no numeric scope, forcing tag-intersection resolution.
    let r = ModRegistry::from_mods(mods, vec![]);

    let base = BaseTypeId::from(BASE);
    let class = ItemClassId::from(CLASS);
    let base_tags = [tag.clone()];

    // At an ilvl where all three are rollable, the weakest tier's inclusive
    // weight sums every tier's tag-resolved weight (100+200+300).
    let weakest = r.get(&ModId::from("TagT3")).expect("mod exists");
    let incl_weak = r.inclusive_weight_for_on_base(weakest, &base, 100, &class, &base_tags);
    assert!(
        (incl_weak - 600.0).abs() < 1e-9,
        "weakest tier inclusive weight should sum all tag-resolved weights \
         (100+200+300=600); got {incl_weak}"
    );

    // The strongest rollable tier inherits only itself.
    let strongest = r.get(&ModId::from("TagT1")).expect("mod exists");
    let incl_strong = r.inclusive_weight_for_on_base(strongest, &base, 100, &class, &base_tags);
    assert!(
        (incl_strong - 100.0).abs() < 1e-9,
        "strongest tier inclusive weight should be just its own tag weight; got {incl_strong}"
    );

    // Without base tags the tag path is unreachable and the eligibility stub
    // (1.0 per positive-spawn-weight mod) is summed instead — confirms the
    // tag-resolved numbers above truly came from the tag path.
    let stub_weak = r.inclusive_weight_for_on_base(weakest, &base, 100, &class, &[]);
    assert!(
        (stub_weak - 3.0).abs() < 1e-9,
        "without base tags each rollable peer contributes the 1.0 stub (3 peers); got {stub_weak}"
    );
}

/// Tiers at required_level [1, 50, 82]: the level-1 tier's inclusive weight at
/// ilvl 81 sums only the 1 and 50 tiers (82 is not yet rollable), and at ilvl
/// 82 it includes all three — the ilvl boundary is inclusive.
#[test]
fn inclusive_weight_multi_tier_ilvl_boundary() {
    let mods = vec![
        prefix_at("Boundary_T3", "BoundaryGroup", 1),
        prefix_at("Boundary_T2", "BoundaryGroup", 50),
        prefix_at("Boundary_T1", "BoundaryGroup", 82),
    ];
    let weights = vec![
        obs("Boundary_T3", 1000.0),
        obs("Boundary_T2", 400.0),
        obs("Boundary_T1", 100.0),
    ];
    let r = ModRegistry::from_mods(mods, weights);

    // ilvl 81: the 82-tier is excluded → 1000 + 400 = 1400.
    let at_81 = incl(&r, "Boundary_T3", 81);
    assert!(
        (at_81 - 1400.0).abs() < 1e-9,
        "at ilvl 81 only the 1 and 50 tiers count (1000+400=1400); got {at_81}"
    );

    // ilvl 82: the 82-tier unlocks (boundary inclusive) → 1000 + 400 + 100 = 1500.
    let at_82 = incl(&r, "Boundary_T3", 82);
    assert!(
        (at_82 - 1500.0).abs() < 1e-9,
        "at ilvl 82 (== required_level) all three tiers count (1000+400+100=1500); got {at_82}"
    );
    assert!(
        at_82 > at_81,
        "unlocking the top tier must raise inclusive weight"
    );
}
