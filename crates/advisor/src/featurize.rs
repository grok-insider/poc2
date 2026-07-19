//! M16.1 — Featurized state representation for offline training.
//!
//! Maps a full [`Item`] state to a compact [`FeatureVec`] so the Q-table
//! is tractable. The featurization mirrors the model used in
//! [Britz, *Solving the Path of Exile crafting MDP*](https://dennybritz.com/posts/poe-crafting/)
//! adapted for PoE2's mod-slot count, fracture/desecrated machinery, and
//! Hinekora's Lock state.
//!
//! ## Why featurize at all
//!
//! The full [`Item`] is unique-per-roll: a Q-table keyed on `Item`
//! directly explodes the state space and learns nothing reusable.
//! [`FeatureVec`] collapses items that are equivalent for the planner's
//! decision purposes — same rarity, same bitmap of which target specs
//! are satisfied, same affix-slot occupancy, same lock/fracture/corrupt
//! signals. Reachable subset per goal is on the order of `10^4` for
//! typical body-armour / weapon goals.
//!
//! ## Field-by-field rationale
//!
//! - `rarity`            — drives which currencies are eligible
//! - `target_match`      — bitmap of satisfied target specs (cap 16)
//! - `n_prefixes` /
//!   `n_suffixes`        — affix-slot occupancy 0..=3
//! - `has_hidden_desecrated` — distinguishes pre-Reveal from post-Reveal
//! - `has_fractured`     — fractures lock mods; downstream planning differs
//! - `is_corrupted`      — terminal-state signal for Vaal-finish branches
//! - `has_hinekora_lock` — locked items use a deterministic seed
//! - `extra_flags`       — match-progress lanes for `count > 1` specs
//!   (bits 4–7 reserved for future per-class signals)
//!
//! ## State-space size
//!
//! `4 × 2^16 × 4 × 4 × 2^4 = ~67M` raw, but reachable subset is
//! `~10^4` per goal because:
//! - `n_prefixes + n_suffixes ≤ 6`
//! - `target_match` is bounded by goal cardinality (typical ≤ 5)
//! - rarity transitions are monotonic for most chains.
//!
//! Reference: `docs/81-engine-training-and-rule-encoding-plan.md` §6.1
//! Tier 3.1.

use poc2_engine::item::{Item, Rarity};
use poc2_engine::registry::ModRegistry;
use poc2_strategies::TargetSpec;

use crate::goal::Goal;

/// Compact featurized representation of an [`Item`] relative to a [`Goal`].
///
/// The feature vector is the canonical Q-table key. Two items with
/// identical `FeatureVec`s are treated as the same state by the trained
/// policy.
///
/// `target_match` bit `i` is set when the item carries at least one mod
/// satisfying [`Goal::target.prefixes[i]`] (for `i < n_prefixes_specs`)
/// or [`Goal::target.suffixes[i - n_prefixes_specs]`] otherwise. The
/// caller is responsible for aligning bitmap indices with the goal's
/// spec ordering — [`featurize`] does so deterministically by
/// concatenating `target.prefixes` followed by `target.suffixes`.
///
/// Cap: 16 target specs total. Goals with more than 16 specs collapse
/// the overflow into a single bit (clamped to bit 15) — the trained
/// policy loses some resolution but degrades gracefully.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[allow(clippy::struct_excessive_bools)] // FeatureVec layout matches the documented Q-table key.
pub struct FeatureVec {
    /// Rarity tier (`0=Normal`, `1=Magic`, `2=Rare`, `3=Unique`).
    pub rarity: u8,
    /// Bitmap of satisfied target specs (cap 16).
    pub target_match: u16,
    /// Number of explicit prefixes (0..=3, clamped).
    pub n_prefixes: u8,
    /// Number of explicit suffixes (0..=3, clamped).
    pub n_suffixes: u8,
    /// Item carries an unrevealed hidden desecrated mod.
    pub has_hidden_desecrated: bool,
    /// At least one mod is fractured (any affix slot).
    pub has_fractured: bool,
    /// Item is Vaal-corrupted.
    pub is_corrupted: bool,
    /// Item carries an active Hinekora's Lock seed.
    pub has_hinekora_lock: bool,
    /// Per-spec match-progress lanes for `count > 1` specs (artefact
    /// schema v2): bits 0–1 = `min(matched, 3)` of the first multi-count
    /// spec, bits 2–3 = the second. Bits 4–7 remain reserved for future
    /// per-class signals. See [`multi_count_progress`].
    pub extra_flags: u8,
}

impl FeatureVec {
    /// Pack this feature vector into a `u64` Q-table key. Bit-stable so
    /// trained models can ship as a serialized table without per-host
    /// drift.
    ///
    /// Layout (LSB → MSB):
    /// - bits 0..=1   : rarity (2 bits)
    /// - bits 2..=17  : target_match (16 bits)
    /// - bits 18..=20 : n_prefixes (3 bits, supports clamp at 7)
    /// - bits 21..=23 : n_suffixes (3 bits)
    /// - bit  24      : has_hidden_desecrated
    /// - bit  25      : has_fractured
    /// - bit  26      : is_corrupted
    /// - bit  27      : has_hinekora_lock
    /// - bits 28..=35 : extra_flags (8 bits)
    /// - bits 36..=63 : reserved zero
    #[must_use]
    pub fn pack(self) -> u64 {
        (u64::from(self.rarity) & 0b11)
            | ((u64::from(self.target_match) & 0xFFFF) << 2)
            | ((u64::from(self.n_prefixes) & 0b111) << 18)
            | ((u64::from(self.n_suffixes) & 0b111) << 21)
            | (u64::from(self.has_hidden_desecrated) << 24)
            | (u64::from(self.has_fractured) << 25)
            | (u64::from(self.is_corrupted) << 26)
            | (u64::from(self.has_hinekora_lock) << 27)
            | ((u64::from(self.extra_flags) & 0xFF) << 28)
    }
}

/// Featurize an [`Item`] relative to a [`Goal`].
///
/// Pure function — no RNG, no state. Two calls with the same inputs
/// produce identical output (the round-trip property the trained
/// policy relies on).
#[must_use]
pub fn featurize(item: &Item, goal: &Goal, registry: &ModRegistry) -> FeatureVec {
    let rarity = match item.rarity {
        Rarity::Normal => 0,
        Rarity::Magic => 1,
        Rarity::Rare => 2,
        Rarity::Unique => 3,
    };

    FeatureVec {
        rarity,
        target_match: target_match_bitmap(item, goal, registry),
        n_prefixes: clamp_u8_to_3plus(item.prefixes.len()),
        n_suffixes: clamp_u8_to_3plus(item.suffixes.len()),
        has_hidden_desecrated: item.hidden_desecrated.is_some(),
        has_fractured: item.has_fractured(),
        is_corrupted: item.corrupted,
        has_hinekora_lock: item.hinekora_lock.is_some(),
        extra_flags: multi_count_progress(item, goal, registry),
    }
}

/// Per-spec match-progress encoding for `count > 1` specs (artefact
/// schema v2, `extra_flags`).
///
/// The satisfaction bitmap (`target_match`) only flips a `count = N`
/// spec's bit when all `N` mods are present, which makes intermediate
/// progress (1-of-3 resistances vs 2-of-3) invisible to the featurized
/// state — the abstracted MDP then can't represent the path to the
/// terminal at all (states with different progress collapse, and the
/// collapsed representative's transitions may never reach the goal).
///
/// Encoding: the first **two** `count > 1` specs, in the same canonical
/// order as the bitmap (prefixes then suffixes), each get 2 bits carrying
/// `min(matched, 3)`: bits 0–1 for the first such spec, bits 2–3 for the
/// second. PoE2 caps each affix side at 3 mods, so `count > 3` is
/// unsatisfiable and 2 bits of progress are exact. Goals with a third+
/// multi-count spec lose progress resolution on it (the terminal bit
/// stays exact; only path guidance coarsens — graceful degradation, same
/// spirit as the 16-spec bitmap cap).
fn multi_count_progress(item: &Item, goal: &Goal, registry: &ModRegistry) -> u8 {
    let n_prefix_specs = goal.target.prefixes.len();
    let total_specs = n_prefix_specs + goal.target.suffixes.len();
    let cap = total_specs.min(16);

    let mut flags = 0u8;
    let mut lane = 0u8;
    for spec_idx in 0..cap {
        let (spec, slot): (&TargetSpec, &[poc2_engine::item::ModRoll]) =
            if spec_idx < n_prefix_specs {
                (&goal.target.prefixes[spec_idx], &item.prefixes[..])
            } else {
                (
                    &goal.target.suffixes[spec_idx - n_prefix_specs],
                    &item.suffixes[..],
                )
            };
        if spec.count <= 1 {
            continue;
        }
        let matched = crate::goal::spec_match_count(spec, slot, registry).min(3);
        flags |= matched << (2 * lane);
        lane += 1;
        if lane == 2 {
            break;
        }
    }
    flags
}

/// Build the target-match bitmap for an item under the supplied goal.
///
/// Bit `i` is set when the goal's `i`-th target spec is **fully
/// satisfied** — i.e. the item carries at least `spec.count` mods
/// matching the spec's concept set, honouring `affix` and
/// `allow_hybrid`, per the exact predicate [`crate::goal::is_satisfied`]
/// evaluates ([`crate::goal::spec_satisfied`]). Specs are enumerated in
/// `target.prefixes` order followed by `target.suffixes` order (cap 16).
///
/// Historical note: v3 originally encoded *presence* ("any one mod
/// matches"), which made bitmap-based terminal predicates fire before
/// real satisfaction on `count > 1` goals — the training-quality pass
/// aligned the bitmap with `spec_satisfied` (artefact schema v2).
///
/// Hybrid mods participate in every spec they overlap with — i.e., a
/// `+ES + +Life` hybrid counts toward both an `EnergyShield` spec
/// and a `Life` spec (when `allow_hybrid`). This generalizes the trained
/// policy across goals that include hybrid keepers.
#[must_use]
pub fn target_match_bitmap(item: &Item, goal: &Goal, registry: &ModRegistry) -> u16 {
    let mut bitmap = 0u16;
    let n_prefix_specs = goal.target.prefixes.len();
    let total_specs = n_prefix_specs + goal.target.suffixes.len();
    let cap = total_specs.min(16);

    // Materialize spec-vs-slot pairings up front: for each i in [0, cap),
    // remember whether the spec is a prefix or suffix and which slice
    // to scan.
    for spec_idx in 0..cap {
        let (spec, slot): (&TargetSpec, &[poc2_engine::item::ModRoll]) =
            if spec_idx < n_prefix_specs {
                (&goal.target.prefixes[spec_idx], &item.prefixes[..])
            } else {
                (
                    &goal.target.suffixes[spec_idx - n_prefix_specs],
                    &item.suffixes[..],
                )
            };
        if crate::goal::spec_satisfied(spec, slot, registry) {
            bitmap |= 1u16 << spec_idx;
        }
    }
    bitmap
}

/// Clamp a usize affix count to `0..=3`. The featurization treats
/// 4+-mod items the same as 3-mod items because the engine caps Rare
/// affix slots at 3 per side.
fn clamp_u8_to_3plus(n: usize) -> u8 {
    n.min(3) as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use poc2_engine::ids::{BaseTypeId, ConceptId, ItemClassId, ModGroupId, ModId, StatId, TagId};
    use poc2_engine::item::{AffixType, ModRoll, QualityKind};
    use poc2_engine::mods::{
        ModDefinition, ModDomain, ModFlags, ModGroup, ModKind, ModStat, SpawnWeight,
    };
    use poc2_engine::patch::PatchRange;
    use poc2_market::DivEquiv;
    use poc2_strategies::Target;
    use smallvec::smallvec;

    fn mk_es_mod(id: &str, hybrid: bool) -> ModDefinition {
        let concept_set = if hybrid {
            smallvec![ConceptId::from("EnergyShield"), ConceptId::from("Life"),]
        } else {
            smallvec![ConceptId::from("EnergyShield")]
        };
        let flags = if hybrid {
            ModFlags::HYBRID
        } else {
            ModFlags::empty()
        };
        ModDefinition {
            id: ModId::from(id),
            name: None,
            mod_group: ModGroup(ModGroupId::from(format!("ES-{id}"))),
            affix_type: AffixType::Prefix,
            kind: ModKind::Explicit,
            domain: ModDomain::Item,
            tags: smallvec![],
            concept_set,
            spawn_weights: smallvec![SpawnWeight {
                tag: TagId::from("BodyArmour"),
                weight: 1
            }],
            stats: smallvec![ModStat {
                stat_id: StatId::from("local_energy_shield"),
                min: 50.0,
                max: 80.0,
            }],
            required_level: 75,
            tier: None,
            allowed_item_classes: smallvec![ItemClassId::from("BodyArmour")],
            patch_range: PatchRange::ALL,
            flags,
            text_template: None,
        }
    }

    fn mk_life_mod(id: &str) -> ModDefinition {
        ModDefinition {
            id: ModId::from(id),
            name: None,
            mod_group: ModGroup(ModGroupId::from(format!("Life-{id}"))),
            affix_type: AffixType::Suffix,
            kind: ModKind::Explicit,
            domain: ModDomain::Item,
            tags: smallvec![],
            concept_set: smallvec![ConceptId::from("Life")],
            spawn_weights: smallvec![],
            stats: smallvec![],
            required_level: 1,
            tier: None,
            allowed_item_classes: smallvec![ItemClassId::from("BodyArmour")],
            patch_range: PatchRange::ALL,
            flags: ModFlags::empty(),
            text_template: None,
        }
    }

    fn mk_item(rarity: Rarity, prefixes: Vec<&str>, suffixes: Vec<&str>) -> Item {
        Item {
            base: BaseTypeId::from("BodyArmour"),
            ilvl: 82,
            rarity,
            corrupted: false,
            sanctified: false,
            mirrored: false,
            quality: 0,
            quality_kind: QualityKind::Untagged,
            implicits: smallvec![],
            prefixes: prefixes
                .into_iter()
                .map(|id| ModRoll {
                    mod_id: ModId::from(id),
                    affix_type: AffixType::Prefix,
                    kind: ModKind::Explicit,
                    values: smallvec![],
                    is_fractured: false,
                })
                .collect(),
            suffixes: suffixes
                .into_iter()
                .map(|id| ModRoll {
                    mod_id: ModId::from(id),
                    affix_type: AffixType::Suffix,
                    kind: ModKind::Explicit,
                    values: smallvec![],
                    is_fractured: false,
                })
                .collect(),
            enchantments: smallvec![],
            hidden_desecrated: None,
            sockets: smallvec![],
            hinekora_lock: None,
        }
    }

    fn es_target(count: u8) -> TargetSpec {
        TargetSpec {
            concept: Some(ConceptId::from("EnergyShield")),
            concept_any: vec![],
            affix: None,
            count,
            min_tier: None,
            allow_hybrid: true,
        }
    }

    fn life_target(count: u8) -> TargetSpec {
        TargetSpec {
            concept: Some(ConceptId::from("Life")),
            concept_any: vec![],
            affix: None,
            count,
            min_tier: None,
            allow_hybrid: true,
        }
    }

    #[test]
    fn featurize_round_trip_is_pure() {
        let registry = ModRegistry::from_mods(vec![mk_es_mod("ES1", false)], vec![]);
        let item = mk_item(Rarity::Magic, vec!["ES1"], vec![]);
        let goal = Goal::new(
            Target {
                prefixes: vec![es_target(1)],
                suffixes: vec![],
                constraints: vec![],
            },
            DivEquiv::point(100.0),
        );
        let a = featurize(&item, &goal, &registry);
        let b = featurize(&item, &goal, &registry);
        assert_eq!(a, b);
        assert_eq!(a.pack(), b.pack());
    }

    #[test]
    fn featurize_rarity_maps_correctly() {
        let registry = ModRegistry::from_mods(vec![], vec![]);
        let goal = Goal::empty(DivEquiv::point(100.0));
        for (rarity, expected_byte) in [
            (Rarity::Normal, 0u8),
            (Rarity::Magic, 1),
            (Rarity::Rare, 2),
            (Rarity::Unique, 3),
        ] {
            let item = mk_item(rarity, vec![], vec![]);
            let f = featurize(&item, &goal, &registry);
            assert_eq!(f.rarity, expected_byte, "rarity {rarity:?}");
        }
    }

    #[test]
    fn featurize_clamps_affix_counts_at_3() {
        let registry = ModRegistry::from_mods(vec![], vec![]);
        let goal = Goal::empty(DivEquiv::point(100.0));
        let item = mk_item(Rarity::Rare, vec!["a", "b", "c"], vec!["x", "y", "z"]);
        let f = featurize(&item, &goal, &registry);
        assert_eq!(f.n_prefixes, 3);
        assert_eq!(f.n_suffixes, 3);
    }

    #[test]
    fn target_match_bitmap_sets_bit_for_satisfied_spec() {
        let registry = ModRegistry::from_mods(vec![mk_es_mod("ES1", false)], vec![]);
        let item = mk_item(Rarity::Magic, vec!["ES1"], vec![]);
        let goal = Goal::new(
            Target {
                prefixes: vec![es_target(1)],
                suffixes: vec![],
                constraints: vec![],
            },
            DivEquiv::point(100.0),
        );
        let bitmap = target_match_bitmap(&item, &goal, &registry);
        assert_eq!(bitmap & 0b1, 0b1);
    }

    #[test]
    fn target_match_bitmap_handles_hybrid_mods() {
        // A hybrid +ES +Life mod sets bits for both the ES prefix-spec
        // (idx 0) and the Life suffix-spec (idx 1, which falls in the
        // suffix-half of the bitmap because n_prefix_specs == 1).
        let registry = ModRegistry::from_mods(vec![mk_es_mod("HYB1", true)], vec![]);
        let item = mk_item(Rarity::Magic, vec!["HYB1"], vec![]);
        let goal = Goal::new(
            Target {
                prefixes: vec![es_target(1)],
                suffixes: vec![life_target(1)],
                constraints: vec![],
            },
            DivEquiv::point(100.0),
        );
        let bitmap = target_match_bitmap(&item, &goal, &registry);
        // Bit 0 = ES spec (prefix slot) — HYB1 is in prefixes → set.
        assert_eq!(bitmap & 0b1, 0b1);
        // Bit 1 = Life spec (suffix slot) — HYB1 is in prefixes, NOT in
        // suffixes, so the suffix-spec scan won't see it. Bit unset.
        assert_eq!(bitmap & 0b10, 0);
    }

    #[test]
    fn target_match_bitmap_disregards_hybrid_when_allow_hybrid_false() {
        let registry = ModRegistry::from_mods(vec![mk_es_mod("HYB1", true)], vec![]);
        let item = mk_item(Rarity::Magic, vec!["HYB1"], vec![]);
        let mut spec = es_target(1);
        spec.allow_hybrid = false;
        let goal = Goal::new(
            Target {
                prefixes: vec![spec],
                suffixes: vec![],
                constraints: vec![],
            },
            DivEquiv::point(100.0),
        );
        let bitmap = target_match_bitmap(&item, &goal, &registry);
        assert_eq!(
            bitmap, 0,
            "hybrid should be ignored when allow_hybrid=false"
        );
    }

    #[test]
    fn target_match_bitmap_caps_at_16_specs() {
        // 17 distinct ES specs — only 16 bits should be set even after
        // every spec is satisfied (the 17th overflows).
        let registry =
            ModRegistry::from_mods(vec![mk_es_mod("ES1", false), mk_life_mod("Life1")], vec![]);
        let mut item = mk_item(Rarity::Rare, vec!["ES1"], vec!["Life1"]);
        // Add many more matchers — all should satisfy the same item.
        item.prefixes.push(ModRoll {
            mod_id: ModId::from("ES1"),
            affix_type: AffixType::Prefix,
            kind: ModKind::Explicit,
            values: smallvec![],
            is_fractured: false,
        });
        let mut prefixes_specs: Vec<TargetSpec> = (0..17).map(|_| es_target(1)).collect();
        prefixes_specs.push(life_target(1));
        let goal = Goal::new(
            Target {
                prefixes: prefixes_specs,
                suffixes: vec![],
                constraints: vec![],
            },
            DivEquiv::point(100.0),
        );
        let bitmap = target_match_bitmap(&item, &goal, &registry);
        // All 16 visible bits set; the 17th is clamped away.
        assert_eq!(bitmap, 0xFFFF);
    }

    #[test]
    fn target_match_bitmap_is_count_aware() {
        // A count=2 spec must NOT set its bit with only one matching mod
        // (the historical presence-only bitmap did — the source of the
        // over-optimistic terminal predicate on count>1 goals).
        let registry = ModRegistry::from_mods(
            vec![mk_es_mod("ES1", false), mk_es_mod("ES2", false)],
            vec![],
        );
        let goal = Goal::new(
            Target {
                prefixes: vec![es_target(2)],
                suffixes: vec![],
                constraints: vec![],
            },
            DivEquiv::point(100.0),
        );

        let one = mk_item(Rarity::Magic, vec!["ES1"], vec![]);
        assert_eq!(
            target_match_bitmap(&one, &goal, &registry),
            0,
            "count=2 spec must not fire with 1 matching mod"
        );

        let two = mk_item(Rarity::Rare, vec!["ES1", "ES2"], vec![]);
        assert_eq!(
            target_match_bitmap(&two, &goal, &registry),
            0b1,
            "count=2 spec fires with 2 matching mods"
        );
    }

    #[test]
    fn extra_flags_encode_multi_count_progress() {
        // Goal: count=1 ES prefix spec (no lane) + count=3 Life suffix
        // spec (first multi-count lane, bits 0-1).
        let registry = ModRegistry::from_mods(
            vec![
                mk_es_mod("ES1", false),
                mk_life_mod("L1"),
                mk_life_mod("L2"),
                mk_life_mod("L3"),
            ],
            vec![],
        );
        let goal = Goal::new(
            Target {
                prefixes: vec![es_target(1)],
                suffixes: vec![life_target(3)],
                constraints: vec![],
            },
            DivEquiv::point(100.0),
        );

        let zero = mk_item(Rarity::Rare, vec![], vec![]);
        assert_eq!(featurize(&zero, &goal, &registry).extra_flags, 0);

        let one = mk_item(Rarity::Rare, vec![], vec!["L1"]);
        let f1 = featurize(&one, &goal, &registry);
        assert_eq!(f1.extra_flags, 1, "1-of-3 progress in lane 0");
        assert_eq!(f1.target_match, 0, "count=3 bit must not fire yet");

        let two = mk_item(Rarity::Rare, vec![], vec!["L1", "L2"]);
        assert_eq!(featurize(&two, &goal, &registry).extra_flags, 2);

        let three = mk_item(Rarity::Rare, vec![], vec!["L1", "L2", "L3"]);
        let f3 = featurize(&three, &goal, &registry);
        assert_eq!(f3.extra_flags, 3);
        assert_eq!(f3.target_match, 0b10, "count=3 spec satisfied");

        // Progress states must be DISTINCT FeatureVecs — this is what
        // makes count>1 terminals reachable in the abstracted MDP.
        assert_ne!(f1.pack(), featurize(&two, &goal, &registry).pack());
    }

    #[test]
    fn featurize_signals_corruption_lock_fracture_desecrated() {
        let registry = ModRegistry::from_mods(vec![], vec![]);
        let goal = Goal::empty(DivEquiv::point(100.0));
        let mut item = mk_item(Rarity::Rare, vec![], vec![]);
        item.corrupted = true;
        item.hinekora_lock = Some(0xfeed_face_u64);
        item.hidden_desecrated = Some(poc2_engine::item::HiddenDesecratedSlot {
            affix_type: AffixType::Prefix,
            bone_size: poc2_engine::item::BoneSize::Preserved,
            bone_subtype: poc2_engine::item::BoneSubtype::Rib,
            abyss_lord: None,
            min_mod_level: 0,
            otherworldly: false,
        });
        item.prefixes.push(ModRoll {
            mod_id: ModId::from("frac1"),
            affix_type: AffixType::Prefix,
            kind: ModKind::Explicit,
            values: smallvec![],
            is_fractured: true,
        });

        let f = featurize(&item, &goal, &registry);
        assert!(f.is_corrupted);
        assert!(f.has_hinekora_lock);
        assert!(f.has_hidden_desecrated);
        assert!(f.has_fractured);
    }

    #[test]
    fn pack_round_trips_through_known_layout() {
        // Construct a feature vec with every field set distinctly and
        // assert the bit layout matches the documented order.
        let f = FeatureVec {
            rarity: 2,                 // Rare
            target_match: 0b1010_1010, // 0xAA
            n_prefixes: 2,
            n_suffixes: 1,
            has_hidden_desecrated: true,
            has_fractured: false,
            is_corrupted: true,
            has_hinekora_lock: false,
            extra_flags: 0x42,
        };
        let packed = f.pack();
        // bit 0..=1 = rarity
        assert_eq!(packed & 0b11, 2);
        // bits 2..=17 = target_match
        assert_eq!((packed >> 2) & 0xFFFF, 0xAA);
        // bits 18..=20 = n_prefixes
        assert_eq!((packed >> 18) & 0b111, 2);
        // bits 21..=23 = n_suffixes
        assert_eq!((packed >> 21) & 0b111, 1);
        // bit 24 = has_hidden_desecrated
        assert_eq!((packed >> 24) & 1, 1);
        // bit 25 = has_fractured
        assert_eq!((packed >> 25) & 1, 0);
        // bit 26 = is_corrupted
        assert_eq!((packed >> 26) & 1, 1);
        // bit 27 = has_hinekora_lock
        assert_eq!((packed >> 27) & 1, 0);
        // bits 28..=35 = extra_flags
        assert_eq!((packed >> 28) & 0xFF, 0x42);
        // bits 36..=63 = reserved zero
        assert_eq!(packed >> 36, 0);
    }
}
