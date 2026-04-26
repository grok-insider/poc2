//! Top-level orchestration: `apply_currency` / `preview_currency` /
//! `commit_with_preview`.
//!
//! These wrappers around `Currency::apply` handle two cross-cutting
//! concerns:
//!
//! 1. **Hinekora's Lock**: if the item carries a `hinekora_lock` seed,
//!    apply uses a deterministic RNG keyed by that seed and clears the
//!    lock on success. Preview operates on a clone and does NOT clear
//!    the lock.
//! 2. **Omens**: the active [`OmenSet`] is threaded through `ApplyContext`.
//!    Currencies consume omens during their apply path; consumed omens
//!    are removed from the set. On a previewed-but-not-committed
//!    operation, the original omen set is preserved (we operate on a
//!    cloned omen set inside the preview).

use rand_xoshiro::rand_core::SeedableRng;
use rand_xoshiro::Xoshiro256PlusPlus;

use crate::currency::{ApplyContext, ApplyOutcome, Currency};
use crate::error::EngineResult;
use crate::item::Item;
use crate::omen::OmenSet;
use crate::patch::PatchVersion;
use crate::registry::ModRegistry;

/// Apply `currency` to `item`, honoring Hinekora's Lock and consuming any
/// matching omens from `omens`.
///
/// On failure, both the lock and the omen set are preserved (the
/// operation didn't modify anything, so the player keeps their setup).
pub fn apply_currency(
    currency: &dyn Currency,
    item: &mut Item,
    registry: &ModRegistry,
    rng: &mut dyn rand::RngCore,
    patch: PatchVersion,
    omens: &mut OmenSet,
) -> EngineResult<ApplyOutcome> {
    if let Some(seed) = item.hinekora_lock {
        let mut locked_rng = Xoshiro256PlusPlus::seed_from_u64(seed);
        // Snapshot omens so a failure rolls back any consumption.
        let omen_snapshot = omens.clone();
        let mut ctx = ApplyContext::new(registry, &mut locked_rng, patch, omens);
        let result = currency.apply(item, &mut ctx);
        if result.is_ok() {
            item.hinekora_lock = None;
        } else {
            // Roll back consumed omens.
            *omens = omen_snapshot;
        }
        result
    } else {
        let omen_snapshot = omens.clone();
        let mut ctx = ApplyContext::new(registry, rng, patch, omens);
        let result = currency.apply(item, &mut ctx);
        if result.is_err() {
            *omens = omen_snapshot;
        }
        result
    }
}

/// Preview the result of applying `currency` without mutating anything.
///
/// Returns the post-apply [`Item`] state on success. On failure, the engine
/// returns the error and `item` / `omens` are left unchanged.
///
/// The returned `Item` reflects the lock having been consumed (if it would
/// have been by a real apply); the caller's `item` does not.
pub fn preview_currency(
    currency: &dyn Currency,
    item: &Item,
    registry: &ModRegistry,
    rng: &mut dyn rand::RngCore,
    patch: PatchVersion,
    omens: &OmenSet,
) -> EngineResult<Item> {
    let mut clone = item.clone();
    let mut omens_clone = omens.clone();
    if let Some(seed) = clone.hinekora_lock {
        let mut locked_rng = Xoshiro256PlusPlus::seed_from_u64(seed);
        let mut ctx = ApplyContext::new(registry, &mut locked_rng, patch, &mut omens_clone);
        currency.apply(&mut clone, &mut ctx)?;
    } else {
        let mut ctx = ApplyContext::new(registry, rng, patch, &mut omens_clone);
        currency.apply(&mut clone, &mut ctx)?;
    }
    // Don't clear the lock here even on success — preview is non-mutating.
    Ok(clone)
}

/// Convenience: preview, then commit if the previewed result satisfies the
/// caller-supplied predicate. Returns the post-commit item if accepted, or
/// `None` if the predicate rejected the preview (item left untouched).
///
/// Because commit re-runs the apply with the SAME lock seed, the post-
/// commit item byte-matches the preview (modulo lock-cleared).
pub fn commit_with_preview<P>(
    currency: &dyn Currency,
    item: &mut Item,
    registry: &ModRegistry,
    rng: &mut dyn rand::RngCore,
    patch: PatchVersion,
    omens: &mut OmenSet,
    accept: P,
) -> EngineResult<Option<Item>>
where
    P: FnOnce(&Item) -> bool,
{
    let preview = preview_currency(currency, item, registry, rng, patch, omens)?;
    if !accept(&preview) {
        return Ok(None);
    }
    apply_currency(currency, item, registry, rng, patch, omens)?;
    Ok(Some(item.clone()))
}

#[cfg(test)]
mod tests {
    use rand::SeedableRng;
    use rand_xoshiro::Xoshiro256PlusPlus;
    use smallvec::smallvec;

    use super::*;
    use crate::currency::basic::OrbOfTransmutation;
    use crate::ids::{ItemClassId, ModGroupId, ModId, TagId};
    use crate::item::{AffixType, QualityKind, Rarity};
    use crate::mods::{ModDefinition, ModDomain, ModFlags, ModGroup, ModKind, SpawnWeight};
    use crate::patch::PatchRange;

    fn mk_mod(id: &str, group: &str, affix: AffixType, class: &str) -> ModDefinition {
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
                tag: TagId::from(class),
                weight: 1
            }],
            stats: smallvec![],
            required_level: 1,
            allowed_item_classes: smallvec![ItemClassId::from(class)],
            patch_range: PatchRange::ALL,
            flags: ModFlags::empty(),
            text_template: None,
        }
    }

    fn fixture() -> Item {
        Item {
            base: ItemClassId::from("Boots").as_str().into(),
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
        }
    }

    fn registry() -> ModRegistry {
        ModRegistry::from_mods(vec![
            mk_mod("Life1", "Life", AffixType::Prefix, "Boots"),
            mk_mod("ES1", "ES", AffixType::Prefix, "Boots"),
            mk_mod("FireRes1", "FireRes", AffixType::Suffix, "Boots"),
            mk_mod("ColdRes1", "ColdRes", AffixType::Suffix, "Boots"),
        ])
    }

    #[test]
    fn apply_clears_lock_on_success() {
        let reg = registry();
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(1);
        let mut item = fixture();
        item.hinekora_lock = Some(0xdead_beef_u64);
        let mut omens = OmenSet::new();

        apply_currency(
            &OrbOfTransmutation::new(),
            &mut item,
            &reg,
            &mut rng,
            PatchVersion::PATCH_0_4_0,
            &mut omens,
        )
        .unwrap();

        assert_eq!(item.rarity, Rarity::Magic);
        assert!(item.hinekora_lock.is_none(), "lock should be consumed");
    }

    #[test]
    fn apply_keeps_lock_and_omens_on_failure() {
        let reg = registry();
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(2);
        let mut item = fixture();
        item.rarity = Rarity::Magic; // Transmute will refuse
        item.hinekora_lock = Some(0xfeed_face_u64);
        let mut omens = OmenSet::new();
        omens.push(crate::omen::Omen::greater_exaltation());

        let r = apply_currency(
            &OrbOfTransmutation::new(),
            &mut item,
            &reg,
            &mut rng,
            PatchVersion::PATCH_0_4_0,
            &mut omens,
        );
        assert!(r.is_err());
        assert_eq!(item.hinekora_lock, Some(0xfeed_face_u64));
        // Omens preserved — transmute didn't consume any.
        assert_eq!(omens.len(), 1);
    }

    #[test]
    fn preview_does_not_mutate_or_clear_lock_or_omens() {
        let reg = registry();
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(3);
        let mut item = fixture();
        item.hinekora_lock = Some(0xc0de);
        let omens = OmenSet::new();

        let preview = preview_currency(
            &OrbOfTransmutation::new(),
            &item,
            &reg,
            &mut rng,
            PatchVersion::PATCH_0_4_0,
            &omens,
        )
        .unwrap();
        assert_eq!(preview.rarity, Rarity::Magic);
        // Original is untouched.
        assert_eq!(item.rarity, Rarity::Normal);
        assert_eq!(item.hinekora_lock, Some(0xc0de));
    }

    #[test]
    fn preview_and_commit_with_locked_seed_match_exactly() {
        let reg = registry();
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(4);
        let mut item = fixture();
        item.hinekora_lock = Some(0xa11ce);
        let mut omens = OmenSet::new();

        let previewed = preview_currency(
            &OrbOfTransmutation::new(),
            &item,
            &reg,
            &mut rng,
            PatchVersion::PATCH_0_4_0,
            &omens,
        )
        .unwrap();
        apply_currency(
            &OrbOfTransmutation::new(),
            &mut item,
            &reg,
            &mut rng,
            PatchVersion::PATCH_0_4_0,
            &mut omens,
        )
        .unwrap();

        let mut expected_committed = previewed.clone();
        expected_committed.hinekora_lock = None;
        assert_eq!(item, expected_committed);
    }

    #[test]
    fn commit_with_preview_accepts_and_rejects() {
        let reg = registry();
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(5);
        let mut item = fixture();
        item.hinekora_lock = Some(0xb1ce);
        let mut omens = OmenSet::new();

        let result = commit_with_preview(
            &OrbOfTransmutation::new(),
            &mut item,
            &reg,
            &mut rng,
            PatchVersion::PATCH_0_4_0,
            &mut omens,
            |_preview| true,
        )
        .unwrap();
        assert!(result.is_some());
        assert_eq!(item.rarity, Rarity::Magic);
        assert!(item.hinekora_lock.is_none());

        // Reset and reject path.
        let mut item = fixture();
        item.hinekora_lock = Some(0xb2ad);
        let result = commit_with_preview(
            &OrbOfTransmutation::new(),
            &mut item,
            &reg,
            &mut rng,
            PatchVersion::PATCH_0_4_0,
            &mut omens,
            |_preview| false,
        )
        .unwrap();
        assert!(result.is_none());
        assert_eq!(item.rarity, Rarity::Normal);
        assert_eq!(item.hinekora_lock, Some(0xb2ad));
    }
}
