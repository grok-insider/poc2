//! Mod registry — fast indices over a `Vec<ModDefinition>`.
//!
//! Built once at engine startup from a [`crate::mods::ModDefinition`] list
//! (typically `bundle.mods`). The registry is read-only and `Send + Sync`,
//! so it can be shared across the advisor's beam-search workers cheaply.
//!
//! Indexes maintained:
//! - `by_id`         — `ModId → index` (O(1) lookup)
//! - `by_group`      — `ModGroupId → [index]` (mod-group ladder)
//! - `by_class_affix` — `(ItemClassId, AffixType) → [index]` (per-class
//!   prefixes/suffixes, the bread-and-butter `apply()` query)
//! - `weights_by_mod_base` — `(ModId, BaseTypeId) → f64` (CoE per-base weight)
//! - `weights_by_mod_class` — `(ModId, ItemClassId) → f64` (CoE per-class
//!   aggregate weight)
//! - `weights_by_mod_base_ilvl` — `(ModId, BaseTypeId, ilvl_floor) → f64`
//!   (forward-compat for `BaseAtIlvl` scope; not currently emitted by the
//!   pipeline but indexed so future per-ilvl weight tiers light up the
//!   resolution path without further registry changes).

use ahash::AHashMap;
use smallvec::SmallVec;

use crate::ids::{BaseTypeId, ItemClassId, ModGroupId, ModId};
use crate::item::AffixType;
use crate::mods::ModDefinition;
use crate::weights::{WeightObservation, WeightScope};

/// Per-(mod, base) ladder of `(min_ilvl, weight)` thresholds populated from
/// `WeightScope::BaseAtIlvl`. Stored sorted ascending by `min_ilvl`. Aliased
/// as a `type` purely to keep the index field declaration readable; clippy's
/// `type_complexity` lint flags the inline form otherwise.
type IlvlWeightLadder = SmallVec<[(u32, f64); 4]>;

/// Opaque internal index into the registry's mod list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ModIndex(u32);

impl ModIndex {
    pub fn get(self) -> u32 {
        self.0
    }
}

/// Registry of mod definitions with the indices the engine needs in its hot path.
pub struct ModRegistry {
    mods: Vec<ModDefinition>,
    by_id: AHashMap<ModId, ModIndex>,
    by_group: AHashMap<ModGroupId, SmallVec<[ModIndex; 8]>>,
    by_class_affix: AHashMap<(ItemClassId, AffixType), Vec<ModIndex>>,
    /// CoE-derived per-base numerical weight: `(mod, base) → weight`.
    /// Populated from [`WeightScope::Base`] observations.
    weights_by_mod_base: AHashMap<(ModId, BaseTypeId), f64>,
    /// CoE-derived per-class aggregate weight: `(mod, item_class) → weight`.
    /// Populated from [`WeightScope::ItemClass`] observations.
    weights_by_mod_class: AHashMap<(ModId, ItemClassId), f64>,
    /// CoE-derived per-base, ilvl-stratified weight: `(mod, base) → [(min_ilvl, weight)]`.
    /// Populated from [`WeightScope::BaseAtIlvl`] observations. Stored
    /// sorted by `min_ilvl` ascending so [`Self::weight_for`] can pick the
    /// highest applicable threshold in one pass.
    weights_by_mod_base_ilvl: AHashMap<(ModId, BaseTypeId), IlvlWeightLadder>,
}

impl ModRegistry {
    /// Build a registry from a list of mod definitions and weight observations.
    ///
    /// O(n + w) over the inputs. Allocates the index maps proportionally.
    /// Duplicate mod IDs are kept (later wins for `by_id`) but logged via
    /// `tracing::warn!`; `Bundle::validate()` rejects duplicates upstream
    /// so this is just a defensive belt-and-suspenders.
    ///
    /// Weight observations whose `mod_id` does not exist in `mods` are
    /// silently dropped — bundles can carry weights for mods that have been
    /// patch-retired without breaking the registry.
    ///
    /// Per-(mod, scope) duplicate weight observations resolve via
    /// last-writer-wins. The pipeline emits at most one per pair today; if
    /// future sources cross-emit duplicates, the warning here surfaces it.
    pub fn from_mods(mods: Vec<ModDefinition>, weights: Vec<WeightObservation>) -> Self {
        let mut by_id = AHashMap::with_capacity(mods.len());
        let mut by_group: AHashMap<ModGroupId, SmallVec<[ModIndex; 8]>> = AHashMap::new();
        let mut by_class_affix: AHashMap<(ItemClassId, AffixType), Vec<ModIndex>> = AHashMap::new();

        for (i, m) in mods.iter().enumerate() {
            let idx = ModIndex(u32::try_from(i).expect("mod count fits u32"));
            if by_id.insert(m.id.clone(), idx).is_some() {
                tracing::warn!(mod_id = %m.id, "duplicate mod id in registry input");
            }
            by_group.entry(m.mod_group.0.clone()).or_default().push(idx);
            for class in &m.allowed_item_classes {
                by_class_affix
                    .entry((class.clone(), m.affix_type))
                    .or_default()
                    .push(idx);
            }
        }

        let mut weights_by_mod_base: AHashMap<(ModId, BaseTypeId), f64> = AHashMap::new();
        let mut weights_by_mod_class: AHashMap<(ModId, ItemClassId), f64> = AHashMap::new();
        let mut weights_by_mod_base_ilvl: AHashMap<(ModId, BaseTypeId), IlvlWeightLadder> =
            AHashMap::new();
        let mut dropped_unknown_mod = 0usize;
        for obs in weights {
            if !by_id.contains_key(&obs.mod_id) {
                dropped_unknown_mod += 1;
                continue;
            }
            match obs.scope {
                WeightScope::Base { base } => {
                    weights_by_mod_base.insert((obs.mod_id, base), obs.primary_weight);
                }
                WeightScope::ItemClass { item_class } => {
                    weights_by_mod_class.insert((obs.mod_id, item_class), obs.primary_weight);
                }
                WeightScope::BaseAtIlvl { base, min_ilvl } => {
                    let entry = weights_by_mod_base_ilvl
                        .entry((obs.mod_id, base))
                        .or_default();
                    entry.push((min_ilvl, obs.primary_weight));
                    entry.sort_by_key(|(ilvl, _)| *ilvl);
                }
            }
        }
        if dropped_unknown_mod > 0 {
            tracing::warn!(
                count = dropped_unknown_mod,
                "dropped weight observations referencing unknown mod ids"
            );
        }

        Self {
            mods,
            by_id,
            by_group,
            by_class_affix,
            weights_by_mod_base,
            weights_by_mod_class,
            weights_by_mod_base_ilvl,
        }
    }

    /// Total mods in the registry.
    pub fn len(&self) -> usize {
        self.mods.len()
    }

    pub fn is_empty(&self) -> bool {
        self.mods.is_empty()
    }

    /// Lookup by ID.
    pub fn get(&self, id: &ModId) -> Option<&ModDefinition> {
        self.by_id.get(id).and_then(|i| self.mods.get(i.0 as usize))
    }

    /// Lookup by index (cheaper than by-id when you already have an index).
    pub fn at(&self, idx: ModIndex) -> Option<&ModDefinition> {
        self.mods.get(idx.0 as usize)
    }

    /// All mods in the same mod-group (the "tier ladder").
    pub fn group_members(&self, group: &ModGroupId) -> &[ModIndex] {
        self.by_group.get(group).map_or(&[][..], |v| &v[..])
    }

    /// All mods that can roll on the given item-class as the given affix.
    pub fn for_class_affix(&self, class: &ItemClassId, affix: AffixType) -> &[ModIndex] {
        self.by_class_affix
            .get(&(class.clone(), affix))
            .map_or(&[][..], |v| &v[..])
    }

    /// Iterator over all mod definitions (in input order).
    pub fn iter(&self) -> impl Iterator<Item = &ModDefinition> {
        self.mods.iter()
    }

    /// Look up a mod's group via a `ModRoll`'s `mod_id`. Convenience for
    /// mod-group exclusivity checks.
    pub fn group_of(&self, id: &ModId) -> Option<&ModGroupId> {
        self.get(id).map(|m| &m.mod_group.0)
    }

    /// Resolve the spawn weight of a mod on a specific (base, ilvl, class).
    ///
    /// Resolution order — first match wins:
    /// 1. **`(mod, base)` ilvl-stratified** — pick the highest `min_ilvl`
    ///    threshold satisfied by `ilvl`. Sourced from `WeightScope::BaseAtIlvl`.
    /// 2. **`(mod, base)` exact** — sourced from `WeightScope::Base`.
    ///    This is CoE's per-base numerical weight.
    /// 3. **`(mod, item_class)` aggregate** — sourced from
    ///    `WeightScope::ItemClass`. Used when no per-base observation
    ///    exists (typical for non-recombinable bases).
    /// 4. **`spawn_weights` tag-eligibility fallback** — RePoE-fork's binary
    ///    eligibility flag. Returns `1.0` when the mod has any non-zero
    ///    `spawn_weights` entry, else `0.0`. This preserves the v2
    ///    "uniform-eligible" behaviour for mods without weight observations,
    ///    so the advisor degrades gracefully when weight coverage is
    ///    incomplete (per plan §9 — "no fails"). Real tag-intersection
    ///    requires the [`crate::base_registry::BaseRegistry`] (M14.2).
    /// 5. **Zero** — mod is not eligible.
    ///
    /// Ineligibility (`Self::for_class_affix` already filtered the candidate
    /// out) is the caller's responsibility; this method assumes the mod is
    /// at least nominally eligible on the item class.
    pub fn weight_for(
        &self,
        mod_id: &ModId,
        base: &BaseTypeId,
        ilvl: u32,
        item_class: &ItemClassId,
    ) -> f64 {
        // 1. Per-base ilvl-stratified weight. The vec is pre-sorted ascending
        //    by `min_ilvl`; iterate in reverse to find the largest threshold
        //    satisfied by `ilvl`.
        if let Some(tiers) = self
            .weights_by_mod_base_ilvl
            .get(&(mod_id.clone(), base.clone()))
        {
            for (min_ilvl, w) in tiers.iter().rev() {
                if ilvl >= *min_ilvl {
                    return *w;
                }
            }
        }
        // 2. Per-base exact weight.
        if let Some(w) = self
            .weights_by_mod_base
            .get(&(mod_id.clone(), base.clone()))
        {
            return *w;
        }
        // 3. Per-class aggregate.
        if let Some(w) = self
            .weights_by_mod_class
            .get(&(mod_id.clone(), item_class.clone()))
        {
            return *w;
        }
        // 4. RePoE-fork tag-eligibility fallback.
        if let Some(idx) = self.by_id.get(mod_id) {
            if let Some(m) = self.mods.get(idx.0 as usize) {
                if m.spawn_weights.iter().any(|sw| sw.weight > 0) {
                    return 1.0;
                }
            }
        }
        // 5. Not eligible.
        0.0
    }

    /// Number of weight observations indexed in this registry. Useful for
    /// diagnostics and the bundle-load log line.
    pub fn weight_observation_count(&self) -> usize {
        self.weights_by_mod_base.len()
            + self.weights_by_mod_class.len()
            + self
                .weights_by_mod_base_ilvl
                .values()
                .map(SmallVec::len)
                .sum::<usize>()
    }
}

#[cfg(test)]
mod tests {
    use smallvec::smallvec;

    use super::*;
    use crate::mods::{ModDomain, ModFlags, ModGroup, ModKind, SpawnWeight};
    use crate::patch::PatchRange;
    use crate::weights::Confidence;

    fn mk_mod(id: &str, group: &str, affix: AffixType, classes: &[&str]) -> ModDefinition {
        ModDefinition {
            id: ModId::from(id),
            name: None,
            mod_group: ModGroup(ModGroupId::from(group)),
            affix_type: affix,
            kind: ModKind::Explicit,
            domain: ModDomain::Item,
            tags: smallvec![],
            concept_set: smallvec![],
            spawn_weights: smallvec![],
            stats: smallvec![],
            required_level: 1,
            allowed_item_classes: classes.iter().map(|c| ItemClassId::from(*c)).collect(),
            patch_range: PatchRange::ALL,
            flags: ModFlags::empty(),
            text_template: None,
        }
    }

    #[test]
    fn registry_indexes_by_id() {
        let r = ModRegistry::from_mods(
            vec![
                mk_mod("A", "G1", AffixType::Prefix, &["Boots"]),
                mk_mod("B", "G2", AffixType::Suffix, &["Boots"]),
            ],
            vec![],
        );
        assert!(r.get(&ModId::from("A")).is_some());
        assert!(r.get(&ModId::from("B")).is_some());
        assert!(r.get(&ModId::from("Z")).is_none());
        assert_eq!(r.len(), 2);
    }

    #[test]
    fn registry_indexes_by_group() {
        let r = ModRegistry::from_mods(
            vec![
                mk_mod("A1", "Life", AffixType::Prefix, &["BodyArmour"]),
                mk_mod("A2", "Life", AffixType::Prefix, &["BodyArmour"]),
                mk_mod("B1", "Mana", AffixType::Prefix, &["BodyArmour"]),
            ],
            vec![],
        );
        assert_eq!(r.group_members(&ModGroupId::from("Life")).len(), 2);
        assert_eq!(r.group_members(&ModGroupId::from("Mana")).len(), 1);
        assert_eq!(r.group_members(&ModGroupId::from("Nope")).len(), 0);
    }

    #[test]
    fn registry_indexes_by_class_affix() {
        let r = ModRegistry::from_mods(
            vec![
                mk_mod("BPrefix", "G1", AffixType::Prefix, &["Boots"]),
                mk_mod("BSuffix", "G2", AffixType::Suffix, &["Boots"]),
                mk_mod("HPrefix", "G3", AffixType::Prefix, &["Helmet"]),
            ],
            vec![],
        );
        assert_eq!(
            r.for_class_affix(&ItemClassId::from("Boots"), AffixType::Prefix)
                .len(),
            1
        );
        assert_eq!(
            r.for_class_affix(&ItemClassId::from("Boots"), AffixType::Suffix)
                .len(),
            1
        );
        assert_eq!(
            r.for_class_affix(&ItemClassId::from("Helmet"), AffixType::Prefix)
                .len(),
            1
        );
        assert_eq!(
            r.for_class_affix(&ItemClassId::from("Helmet"), AffixType::Suffix)
                .len(),
            0
        );
    }

    #[test]
    fn registry_group_of_resolves() {
        let r = ModRegistry::from_mods(
            vec![mk_mod("X", "GroupX", AffixType::Prefix, &["BodyArmour"])],
            vec![],
        );
        assert_eq!(
            r.group_of(&ModId::from("X")).cloned(),
            Some(ModGroupId::from("GroupX"))
        );
    }

    fn obs_base(mod_id: &str, base: &str, weight: f64) -> WeightObservation {
        WeightObservation {
            mod_id: ModId::from(mod_id),
            scope: WeightScope::Base {
                base: BaseTypeId::from(base),
            },
            primary_weight: weight,
            secondary_weight: None,
            confidence: Confidence::Community,
            note: None,
        }
    }

    fn obs_class(mod_id: &str, class: &str, weight: f64) -> WeightObservation {
        WeightObservation {
            mod_id: ModId::from(mod_id),
            scope: WeightScope::ItemClass {
                item_class: ItemClassId::from(class),
            },
            primary_weight: weight,
            secondary_weight: None,
            confidence: Confidence::Community,
            note: None,
        }
    }

    fn obs_base_at_ilvl(mod_id: &str, base: &str, min_ilvl: u32, weight: f64) -> WeightObservation {
        WeightObservation {
            mod_id: ModId::from(mod_id),
            scope: WeightScope::BaseAtIlvl {
                base: BaseTypeId::from(base),
                min_ilvl,
            },
            primary_weight: weight,
            secondary_weight: None,
            confidence: Confidence::Community,
            note: None,
        }
    }

    fn mk_mod_with_eligible_tag(id: &str) -> ModDefinition {
        let mut m = mk_mod(id, id, AffixType::Prefix, &["Helmet"]);
        m.spawn_weights = smallvec![SpawnWeight {
            tag: crate::ids::TagId::from("any"),
            weight: 1,
        }];
        m
    }

    #[test]
    fn weight_for_resolves_per_base_first() {
        let r = ModRegistry::from_mods(
            vec![mk_mod("A", "G", AffixType::Prefix, &["Helmet"])],
            vec![
                obs_base("A", "Sage Wand", 850.0),
                obs_class("A", "Helmet", 500.0),
            ],
        );
        let w = r.weight_for(
            &ModId::from("A"),
            &BaseTypeId::from("Sage Wand"),
            82,
            &ItemClassId::from("Helmet"),
        );
        assert!((w - 850.0).abs() < 1e-9);
    }

    #[test]
    fn weight_for_falls_back_to_class_when_no_base_match() {
        let r = ModRegistry::from_mods(
            vec![mk_mod("A", "G", AffixType::Prefix, &["Helmet"])],
            vec![obs_class("A", "Helmet", 500.0)],
        );
        let w = r.weight_for(
            &ModId::from("A"),
            &BaseTypeId::from("UnknownBase"),
            82,
            &ItemClassId::from("Helmet"),
        );
        assert!((w - 500.0).abs() < 1e-9);
    }

    #[test]
    fn weight_for_uses_eligibility_fallback_when_no_observations() {
        let r = ModRegistry::from_mods(vec![mk_mod_with_eligible_tag("E")], vec![]);
        let w = r.weight_for(
            &ModId::from("E"),
            &BaseTypeId::from("X"),
            1,
            &ItemClassId::from("Helmet"),
        );
        assert!((w - 1.0).abs() < 1e-9);
    }

    #[test]
    fn weight_for_returns_zero_for_unknown_mod() {
        let r = ModRegistry::from_mods(vec![], vec![]);
        let w = r.weight_for(
            &ModId::from("ghost"),
            &BaseTypeId::from("X"),
            1,
            &ItemClassId::from("Helmet"),
        );
        assert!(w.abs() < 1e-12);
    }

    #[test]
    fn weight_for_picks_highest_satisfied_ilvl_threshold() {
        let r = ModRegistry::from_mods(
            vec![mk_mod("A", "G", AffixType::Prefix, &["Helmet"])],
            vec![
                obs_base_at_ilvl("A", "B", 1, 100.0),
                obs_base_at_ilvl("A", "B", 50, 250.0),
                obs_base_at_ilvl("A", "B", 80, 1000.0),
            ],
        );
        assert!(
            (r.weight_for(
                &ModId::from("A"),
                &BaseTypeId::from("B"),
                10,
                &ItemClassId::from("Helmet")
            ) - 100.0)
                .abs()
                < 1e-9
        );
        assert!(
            (r.weight_for(
                &ModId::from("A"),
                &BaseTypeId::from("B"),
                65,
                &ItemClassId::from("Helmet")
            ) - 250.0)
                .abs()
                < 1e-9
        );
        assert!(
            (r.weight_for(
                &ModId::from("A"),
                &BaseTypeId::from("B"),
                85,
                &ItemClassId::from("Helmet")
            ) - 1000.0)
                .abs()
                < 1e-9
        );
    }

    #[test]
    fn weight_observations_referencing_unknown_mods_are_dropped() {
        let r = ModRegistry::from_mods(
            vec![mk_mod("A", "G", AffixType::Prefix, &["Helmet"])],
            vec![obs_base("ghost", "B", 999.0), obs_base("A", "B", 100.0)],
        );
        assert_eq!(r.weight_observation_count(), 1);
        let w = r.weight_for(
            &ModId::from("ghost"),
            &BaseTypeId::from("B"),
            1,
            &ItemClassId::from("Helmet"),
        );
        assert!(w.abs() < 1e-12);
    }
}
