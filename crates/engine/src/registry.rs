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

use ahash::AHashMap;
use smallvec::SmallVec;

use crate::ids::{ItemClassId, ModGroupId, ModId};
use crate::item::AffixType;
use crate::mods::ModDefinition;

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
}

impl ModRegistry {
    /// Build a registry from a list of mod definitions.
    ///
    /// O(n) over the input. Allocates the index maps proportionally.
    /// Duplicate mod IDs are kept (later wins for `by_id`) but logged via
    /// `tracing::warn!`; `Bundle::validate()` rejects duplicates upstream
    /// so this is just a defensive belt-and-suspenders.
    pub fn from_mods(mods: Vec<ModDefinition>) -> Self {
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

        Self {
            mods,
            by_id,
            by_group,
            by_class_affix,
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
}

#[cfg(test)]
mod tests {
    use smallvec::smallvec;

    use super::*;
    use crate::mods::{ModDomain, ModFlags, ModGroup, ModKind};
    use crate::patch::PatchRange;

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
        let r = ModRegistry::from_mods(vec![
            mk_mod("A", "G1", AffixType::Prefix, &["Boots"]),
            mk_mod("B", "G2", AffixType::Suffix, &["Boots"]),
        ]);
        assert!(r.get(&ModId::from("A")).is_some());
        assert!(r.get(&ModId::from("B")).is_some());
        assert!(r.get(&ModId::from("Z")).is_none());
        assert_eq!(r.len(), 2);
    }

    #[test]
    fn registry_indexes_by_group() {
        let r = ModRegistry::from_mods(vec![
            mk_mod("A1", "Life", AffixType::Prefix, &["BodyArmour"]),
            mk_mod("A2", "Life", AffixType::Prefix, &["BodyArmour"]),
            mk_mod("B1", "Mana", AffixType::Prefix, &["BodyArmour"]),
        ]);
        assert_eq!(r.group_members(&ModGroupId::from("Life")).len(), 2);
        assert_eq!(r.group_members(&ModGroupId::from("Mana")).len(), 1);
        assert_eq!(r.group_members(&ModGroupId::from("Nope")).len(), 0);
    }

    #[test]
    fn registry_indexes_by_class_affix() {
        let r = ModRegistry::from_mods(vec![
            mk_mod("BPrefix", "G1", AffixType::Prefix, &["Boots"]),
            mk_mod("BSuffix", "G2", AffixType::Suffix, &["Boots"]),
            mk_mod("HPrefix", "G3", AffixType::Prefix, &["Helmet"]),
        ]);
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
        let r = ModRegistry::from_mods(vec![mk_mod(
            "X",
            "GroupX",
            AffixType::Prefix,
            &["BodyArmour"],
        )]);
        assert_eq!(
            r.group_of(&ModId::from("X")).cloned(),
            Some(ModGroupId::from("GroupX"))
        );
    }
}
