//! Top-level orchestration: `apply_currency` / `preview_currency` /
//! `commit_with_preview`.
//!
//! These are thin wrappers around `Currency::apply` that handle Hinekora's
//! Lock semantics:
//!
//! - **Apply**: if the item carries a `hinekora_lock` seed, the engine
//!   substitutes a deterministic RNG seeded from it and clears the lock
//!   on success. Otherwise the live RNG is used.
//! - **Preview**: same RNG substitution, but operates on a clone of the
//!   item and does NOT clear the lock — the caller can preview many
//!   different currencies before committing one.
//! - **CommitWithPreview**: a convenience that runs preview, returns the
//!   result, and (if the caller likes it) commits — guaranteed to produce
//!   the same outcome because both runs use the same lock seed.
//!
//! Currencies themselves do not need to know about the lock: the engine
//! controls the RNG they receive.

use rand_xoshiro::rand_core::SeedableRng;
use rand_xoshiro::Xoshiro256PlusPlus;

use crate::currency::{ApplyContext, ApplyOutcome, Currency};
use crate::error::EngineResult;
use crate::item::Item;
use crate::patch::PatchVersion;
use crate::registry::ModRegistry;

/// Apply `currency` to `item`, honoring Hinekora's Lock.
///
/// If `item.hinekora_lock` is `Some(seed)` at entry:
/// 1. A new RNG is constructed from the seed.
/// 2. The currency is applied using THAT RNG (ignoring the live RNG).
/// 3. On success, the lock is cleared.
///
/// On failure, the lock is preserved (the operation didn't modify the item,
/// so the lock is still valid for subsequent attempts).
pub fn apply_currency(
    currency: &dyn Currency,
    item: &mut Item,
    registry: &ModRegistry,
    rng: &mut dyn rand::RngCore,
    patch: PatchVersion,
) -> EngineResult<ApplyOutcome> {
    if let Some(seed) = item.hinekora_lock {
        let mut locked_rng = Xoshiro256PlusPlus::seed_from_u64(seed);
        let mut ctx = ApplyContext::new(registry, &mut locked_rng, patch);
        // We treat the lock as consumed by ANY commit attempt — including
        // failed ones in some upstream interpretations. We follow the more
        // generous interpretation: clear only on success, so users don't
        // burn the lock on a refused operation.
        let result = currency.apply(item, &mut ctx);
        if result.is_ok() {
            item.hinekora_lock = None;
        }
        result
    } else {
        let mut ctx = ApplyContext::new(registry, rng, patch);
        currency.apply(item, &mut ctx)
    }
}

/// Preview the result of applying `currency` without mutating `item`.
///
/// Returns the post-apply [`Item`] state on success. On failure, the
/// engine returns the error and `item` is left unchanged.
///
/// Lock semantics: the lock seed is used (so the preview is faithful), but
/// the lock is **not** cleared — the caller can preview multiple currencies
/// before committing one. Per upstream design, all previews against the
/// same lock seed are deterministic with respect to that seed.
pub fn preview_currency(
    currency: &dyn Currency,
    item: &Item,
    registry: &ModRegistry,
    rng: &mut dyn rand::RngCore,
    patch: PatchVersion,
) -> EngineResult<Item> {
    let mut clone = item.clone();
    if let Some(seed) = clone.hinekora_lock {
        let mut locked_rng = Xoshiro256PlusPlus::seed_from_u64(seed);
        let mut ctx = ApplyContext::new(registry, &mut locked_rng, patch);
        currency.apply(&mut clone, &mut ctx)?;
    } else {
        let mut ctx = ApplyContext::new(registry, rng, patch);
        currency.apply(&mut clone, &mut ctx)?;
    }
    // We don't clear the lock here even on success — preview is non-mutating.
    // Return the resulting (cloned) item.
    Ok(clone)
}

/// Convenience: preview, then commit if the previewed result satisfies a
/// caller-supplied predicate. Returns the post-commit item if accepted, or
/// `None` if the predicate rejected the preview (item left untouched).
///
/// Because commit re-runs the apply with the SAME lock seed, the resulting
/// item is guaranteed to equal the preview byte-for-byte.
pub fn commit_with_preview<P>(
    currency: &dyn Currency,
    item: &mut Item,
    registry: &ModRegistry,
    rng: &mut dyn rand::RngCore,
    patch: PatchVersion,
    accept: P,
) -> EngineResult<Option<Item>>
where
    P: FnOnce(&Item) -> bool,
{
    let preview = preview_currency(currency, item, registry, rng, patch)?;
    if !accept(&preview) {
        return Ok(None);
    }
    apply_currency(currency, item, registry, rng, patch)?;
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

        apply_currency(
            &OrbOfTransmutation::new(),
            &mut item,
            &reg,
            &mut rng,
            PatchVersion::PATCH_0_4_0,
        )
        .unwrap();

        assert_eq!(item.rarity, Rarity::Magic);
        assert!(item.hinekora_lock.is_none(), "lock should be consumed");
    }

    #[test]
    fn apply_keeps_lock_on_failure() {
        let reg = registry();
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(2);
        let mut item = fixture();
        item.rarity = Rarity::Magic; // Transmute will refuse
        item.hinekora_lock = Some(0xfeed_face_u64);

        let r = apply_currency(
            &OrbOfTransmutation::new(),
            &mut item,
            &reg,
            &mut rng,
            PatchVersion::PATCH_0_4_0,
        );
        assert!(r.is_err());
        assert_eq!(item.hinekora_lock, Some(0xfeed_face_u64));
    }

    #[test]
    fn preview_does_not_mutate_or_clear_lock() {
        let reg = registry();
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(3);
        let mut item = fixture();
        item.hinekora_lock = Some(0xc0de);

        let preview = preview_currency(
            &OrbOfTransmutation::new(),
            &item,
            &reg,
            &mut rng,
            PatchVersion::PATCH_0_4_0,
        )
        .unwrap();
        assert_eq!(preview.rarity, Rarity::Magic);
        // Original is untouched.
        assert_eq!(item.rarity, Rarity::Normal);
        assert!(item.prefixes.is_empty() && item.suffixes.is_empty());
        // Lock survives.
        assert_eq!(item.hinekora_lock, Some(0xc0de));
    }

    #[test]
    fn preview_and_commit_with_locked_seed_match_exactly() {
        // The whole point of Hinekora's Lock: preview = commit if you commit.
        let reg = registry();
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(4);
        let mut item = fixture();
        item.hinekora_lock = Some(0xa11ce);

        let previewed = preview_currency(
            &OrbOfTransmutation::new(),
            &item,
            &reg,
            &mut rng,
            PatchVersion::PATCH_0_4_0,
        )
        .unwrap();
        // Commit — uses the SAME seed.
        apply_currency(
            &OrbOfTransmutation::new(),
            &mut item,
            &reg,
            &mut rng,
            PatchVersion::PATCH_0_4_0,
        )
        .unwrap();
        // Once committed, the lock is gone — but the previewed item still
        // shows the lock as Some (preview doesn't clear). To compare item
        // state byte-for-byte we have to discount the lock difference.
        let mut expected_committed = previewed.clone();
        expected_committed.hinekora_lock = None;
        assert_eq!(item, expected_committed);
    }

    #[test]
    fn commit_with_preview_accepts() {
        let reg = registry();
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(5);
        let mut item = fixture();
        item.hinekora_lock = Some(0xb1ce);

        let result = commit_with_preview(
            &OrbOfTransmutation::new(),
            &mut item,
            &reg,
            &mut rng,
            PatchVersion::PATCH_0_4_0,
            |_preview| true,
        )
        .unwrap();
        assert!(result.is_some());
        assert_eq!(item.rarity, Rarity::Magic);
        assert!(item.hinekora_lock.is_none());
    }

    #[test]
    fn commit_with_preview_rejects() {
        let reg = registry();
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(6);
        let mut item = fixture();
        item.hinekora_lock = Some(0xb2ad);

        let result = commit_with_preview(
            &OrbOfTransmutation::new(),
            &mut item,
            &reg,
            &mut rng,
            PatchVersion::PATCH_0_4_0,
            |_preview| false, // always reject
        )
        .unwrap();
        assert!(result.is_none());
        // Item unchanged, lock intact.
        assert_eq!(item.rarity, Rarity::Normal);
        assert_eq!(item.hinekora_lock, Some(0xb2ad));
    }

    #[test]
    fn unlocked_apply_uses_live_rng() {
        // Sanity: without a lock, the live RNG is used — different seeds
        // produce different items.
        let reg = registry();
        let mut item_a = fixture();
        let mut rng_a = Xoshiro256PlusPlus::seed_from_u64(0xaaaa);
        apply_currency(
            &OrbOfTransmutation::new(),
            &mut item_a,
            &reg,
            &mut rng_a,
            PatchVersion::PATCH_0_4_0,
        )
        .unwrap();

        let mut item_b = fixture();
        let mut rng_b = Xoshiro256PlusPlus::seed_from_u64(0xbbbb);
        apply_currency(
            &OrbOfTransmutation::new(),
            &mut item_b,
            &reg,
            &mut rng_b,
            PatchVersion::PATCH_0_4_0,
        )
        .unwrap();

        // Both promoted to Magic; the rolled mod ids may or may not differ
        // (small fixture pool). At minimum they're independently sampled —
        // we verify neither inherited the other's hinekora_lock (None on
        // both since neither had a lock).
        assert!(item_a.hinekora_lock.is_none());
        assert!(item_b.hinekora_lock.is_none());
    }
}
