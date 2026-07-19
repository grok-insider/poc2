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

/// Heuristic: does the string look like a PascalCase item-class id
/// (e.g., "BodyArmour") rather than a metadata path (e.g.,
/// "Metadata/Items/...") or some other identifier? Decides when the
/// legacy `Item.base`-is-the-class-id placeholder is recognisable
/// enough for [`BaseRegistry::resolve_item_class_opt`] to trust.
fn is_pascal_case_class_id(s: &str) -> bool {
    !s.is_empty()
        && s.chars().next().is_some_and(|c| c.is_ascii_uppercase())
        && !s.contains('/')
        && s.chars().all(|c| c.is_ascii_alphanumeric())
}

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

    /// Compute an item's class from its base when the resolution is
    /// trustworthy: a registry hit via [`Self::class_of`], or a `base`
    /// string in the legacy PascalCase class-id-placeholder shape that
    /// fixtures stuff into `Item.base`. Returns `None` for an
    /// unregistered metadata-path base, so callers can choose between
    /// fail-open gating (`Bone::can_apply_to`) and the warned fallback
    /// of [`Self::resolve_item_class`]. Every class-from-base decision
    /// in the workspace routes through here so there is exactly one
    /// resolution semantics.
    pub fn resolve_item_class_opt(&self, item: &crate::item::Item) -> Option<ItemClassId> {
        if let Some(class) = self.class_of(&item.base) {
            return Some(class.clone());
        }
        let base = item.base.as_str();
        is_pascal_case_class_id(base).then(|| ItemClassId::from(base))
    }

    /// Compute an item's class from its base (M14.2).
    ///
    /// Resolves via [`Self::resolve_item_class_opt`]; an unresolvable
    /// base (a metadata path absent from the registry) falls back to
    /// `ItemClassId::from(item.base.as_str())` with a `tracing::warn`,
    /// since that class almost certainly matches nothing downstream.
    /// The fallback is preserved through the v3 transitional period;
    /// the v3 hard-reset bundle migration (M14.7) eliminates the need
    /// for it by guaranteeing every imported item carries a real
    /// `BaseTypeId`.
    pub fn resolve_item_class(&self, item: &crate::item::Item) -> ItemClassId {
        self.resolve_item_class_opt(item).unwrap_or_else(|| {
            tracing::warn!(
                base = %item.base,
                "unresolved item base; treating the base id as its item class"
            );
            ItemClassId::from(item.base.as_str())
        })
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
    fn resolve_item_class_prefers_registry_then_falls_back_to_base_string() {
        use crate::item::{Item, QualityKind, Rarity};

        let mk_item = |base: &str| Item {
            base: BaseTypeId::from(base),
            ilvl: 82,
            rarity: Rarity::Normal,
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
        };

        let r = BaseRegistry::from_bases(vec![mk_base("Wanderer_Shoes", "Boots", &[])]);
        // Registered base resolves through the registry.
        assert_eq!(
            r.resolve_item_class(&mk_item("Wanderer_Shoes")),
            ItemClassId::from("Boots")
        );
        // Unregistered base falls back to the class-id-placeholder semantics.
        assert_eq!(
            r.resolve_item_class(&mk_item("BodyArmour")),
            ItemClassId::from("BodyArmour")
        );
        // Unregistered metadata path: same fallback (with a warn).
        assert_eq!(
            r.resolve_item_class(&mk_item("Metadata/Items/Armours/BodyArmours/FourBodyInt3")),
            ItemClassId::from("Metadata/Items/Armours/BodyArmours/FourBodyInt3")
        );
    }

    #[test]
    fn resolve_item_class_opt_distinguishes_legacy_ids_from_metadata_paths() {
        use crate::item::{Item, QualityKind, Rarity};

        let mk_item = |base: &str| Item {
            base: BaseTypeId::from(base),
            ilvl: 82,
            rarity: Rarity::Normal,
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
        };

        let r = BaseRegistry::from_bases(vec![mk_base(
            "Metadata/Items/Armours/BodyArmours/FourBodyInt3",
            "BodyArmour",
            &[],
        )]);
        // Registry hit wins regardless of base shape.
        assert_eq!(
            r.resolve_item_class_opt(&mk_item("Metadata/Items/Armours/BodyArmours/FourBodyInt3")),
            Some(ItemClassId::from("BodyArmour"))
        );
        // Legacy PascalCase placeholder is trusted.
        assert_eq!(
            r.resolve_item_class_opt(&mk_item("BodyArmour")),
            Some(ItemClassId::from("BodyArmour"))
        );
        // Unregistered metadata path is not resolvable.
        assert_eq!(
            r.resolve_item_class_opt(&mk_item("Metadata/Items/Rings/Ring1")),
            None
        );
        assert_eq!(EMPTY.resolve_item_class_opt(&mk_item("Metadata/X")), None);
    }

    #[test]
    fn for_class_returns_empty_for_missing_class() {
        let r = BaseRegistry::from_bases(vec![mk_base("X", "Boots", &[])]);
        assert!(r.for_class(&ItemClassId::from("Helmet")).is_empty());
    }
}
