//! Base-type registry — fast indices over a `Vec<BaseType>` so the engine
//! can resolve `BaseTypeId → ItemClassId` and `BaseTypeId → tags` in O(1).
//!
//! Built once at engine startup from a [`crate::base::BaseType`] list
//! (typically `bundle.base_items`). Read-only and `Send + Sync`.
//!
//! ## Why this exists (M14.2)
//!
//! Before v3 the engine treated `Item.base` as a placeholder for the item
//! class — fixtures stuffed `"BodyArmour"` into `Item.base` and downstream
//! code did `ItemClassId::from(item.base.as_str())`. Real bundle bases
//! (`"Metadata/Items/Armours/BodyArmours/FourBodyInt3"`) broke that
//! placeholder. The registry replaces the placeholder with a real lookup
//! while preserving back-compat: legacy items keep `Item.base_type_id =
//! None` and the class-resolution helper falls back to the old behaviour.
//!
//! ## Indexes maintained
//!
//! - `by_id`    — `BaseTypeId → BaseType` (O(1) get)
//! - `by_class` — `ItemClassId → [BaseTypeId]` (every base in a class)
//! - `EMPTY`    — a `Lazy<BaseRegistry>` static used by tests and
//!   currency-internal helpers that don't need class-aware resolution.

use std::sync::LazyLock;

use ahash::AHashMap;

use crate::base::BaseType;
use crate::ids::{BaseTypeId, ItemClassId, TagId};

/// Read-only registry mapping `BaseTypeId → BaseType` with class + tag
/// secondary indexes.
#[derive(Debug, Clone, Default)]
pub struct BaseRegistry {
    bases: Vec<BaseType>,
    by_id: AHashMap<BaseTypeId, usize>,
    by_class: AHashMap<ItemClassId, Vec<BaseTypeId>>,
}

/// A globally-shared empty registry. Used by test fixtures and engine
/// internals that don't require class-aware base resolution; production
/// code constructs a populated registry via [`BaseRegistry::from_bases`].
pub static EMPTY: LazyLock<BaseRegistry> = LazyLock::new(BaseRegistry::default);

impl BaseRegistry {
    /// Build a registry from a list of base definitions.
    ///
    /// O(n) over the input. Allocates the index maps proportionally.
    /// Duplicate base IDs are kept (later wins for `by_id`) but logged via
    /// `tracing::warn!`.
    pub fn from_bases(bases: Vec<BaseType>) -> Self {
        let mut by_id = AHashMap::with_capacity(bases.len());
        let mut by_class: AHashMap<ItemClassId, Vec<BaseTypeId>> = AHashMap::new();

        for (i, b) in bases.iter().enumerate() {
            if by_id.insert(b.id.clone(), i).is_some() {
                tracing::warn!(base_id = %b.id, "duplicate base id in registry input");
            }
            by_class
                .entry(b.item_class.clone())
                .or_default()
                .push(b.id.clone());
        }

        Self {
            bases,
            by_id,
            by_class,
        }
    }

    /// Number of bases in the registry.
    pub fn len(&self) -> usize {
        self.bases.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bases.is_empty()
    }

    /// Look up a base by id.
    pub fn get(&self, id: &BaseTypeId) -> Option<&BaseType> {
        self.by_id.get(id).and_then(|i| self.bases.get(*i))
    }

    /// Resolve `BaseTypeId → ItemClassId`. Returns `None` if the base is
    /// not registered.
    pub fn class_of(&self, id: &BaseTypeId) -> Option<&ItemClassId> {
        self.get(id).map(|b| &b.item_class)
    }

    /// Resolve `BaseTypeId → tags`. Returns an empty slice if the base is
    /// not registered.
    pub fn tags_of(&self, id: &BaseTypeId) -> &[TagId] {
        self.get(id).map_or(&[][..], |b| &b.tags[..])
    }

    /// All base ids belonging to the given item class.
    pub fn for_class(&self, class: &ItemClassId) -> &[BaseTypeId] {
        self.by_class.get(class).map_or(&[][..], |v| v.as_slice())
    }

    /// Iterator over all base definitions (in input order).
    pub fn iter(&self) -> impl Iterator<Item = &BaseType> {
        self.bases.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::base::{InventorySize, ReleaseState};
    use crate::item_class::AttributePool;
    use crate::patch::PatchRange;
    use smallvec::smallvec;

    fn mk_base(id: &str, class: &str, tags: &[&str]) -> BaseType {
        BaseType {
            id: BaseTypeId::from(id),
            name: id.to_string(),
            item_class: ItemClassId::from(class),
            attribute_pool: AttributePool::Str,
            drop_level: 1,
            tags: tags.iter().map(|t| TagId::from(*t)).collect(),
            implicits: smallvec![],
            inventory: InventorySize {
                width: 2,
                height: 2,
            },
            release_state: ReleaseState::Released,
            patch_range: PatchRange::ALL,
        }
    }

    #[test]
    fn registry_resolves_class_and_tags() {
        let r = BaseRegistry::from_bases(vec![
            mk_base("Heavy_Belt", "Belt", &["belt", "str_armour"]),
            mk_base("Wanderer_Shoes", "Boots", &["boots", "dex_armour"]),
        ]);
        assert_eq!(
            r.class_of(&BaseTypeId::from("Heavy_Belt")),
            Some(&ItemClassId::from("Belt"))
        );
        assert!(r
            .tags_of(&BaseTypeId::from("Heavy_Belt"))
            .iter()
            .any(|t| t == &TagId::from("str_armour")));
        assert_eq!(
            r.for_class(&ItemClassId::from("Boots")),
            &[BaseTypeId::from("Wanderer_Shoes")][..]
        );
    }

    #[test]
    fn unknown_id_returns_none() {
        let r = BaseRegistry::from_bases(vec![]);
        assert!(r.get(&BaseTypeId::from("ghost")).is_none());
        assert!(r.class_of(&BaseTypeId::from("ghost")).is_none());
        assert_eq!(r.tags_of(&BaseTypeId::from("ghost")), &[][..]);
    }

    #[test]
    fn empty_static_is_truly_empty() {
        assert!(EMPTY.is_empty());
        assert_eq!(EMPTY.len(), 0);
    }

    #[test]
    fn for_class_returns_empty_for_missing_class() {
        let r = BaseRegistry::from_bases(vec![mk_base("X", "Boots", &[])]);
        assert!(r.for_class(&ItemClassId::from("Helmet")).is_empty());
    }
}
