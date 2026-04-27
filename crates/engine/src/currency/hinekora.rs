//! Hinekora's Lock — preview the next currency operation before committing.
//!
//! ## Semantics (per planning research)
//!
//! - Applied to an uncorrupted, non-mirrored, non-sanctified item — gives
//!   it a "purple aura" (modeled here as `Item.hinekora_lock = Some(seed)`).
//! - Once locked, hovering a currency in-game previews the **exact**
//!   outcome of applying it. Preview disappears the moment any modifying
//!   operation is committed (the lock is consumed).
//! - Cannot be applied to gems, corrupted items, mirrored items, or items
//!   that cannot be modified.
//! - Cannot interact with Recombinators, the Reforging Bench, or the Altar
//!   of Corruption per upstream design notes.
//!
//! ## Engine model
//!
//! [`HinekorasLock`] is a `Currency` that, on apply, samples a `u64` seed
//! from the live RNG and stores it on the item. Subsequent currency
//! applications go through [`apply_currency`](crate::engine::apply_currency)
//! / [`preview_currency`](crate::engine::preview_currency) which detect
//! the lock and substitute a deterministic RNG keyed by the stored seed.
//! `apply_currency` then **clears** the lock; `preview_currency` does NOT —
//! it operates on a clone.

use rand::Rng;

use crate::currency::{ApplyContext, ApplyOutcome, Currency};
use crate::error::{EngineError, EngineResult};
use crate::ids::CurrencyId;
use crate::item::Item;

#[derive(Debug)]
pub struct HinekorasLock {
    id: CurrencyId,
}

impl HinekorasLock {
    pub fn new() -> Self {
        Self {
            id: CurrencyId::from("HinekorasLock"),
        }
    }
}

impl Default for HinekorasLock {
    fn default() -> Self {
        Self::new()
    }
}

impl Currency for HinekorasLock {
    fn id(&self) -> &CurrencyId {
        &self.id
    }

    fn name(&self) -> &'static str {
        "Hinekora's Lock"
    }

    fn valid_rarities(&self) -> crate::currency::RaritySet {
        crate::currency::RaritySet::NORMAL
            .union(crate::currency::RaritySet::MAGIC)
            .union(crate::currency::RaritySet::RARE)
    }

    fn can_apply_to(&self, item: &Item) -> Result<(), crate::currency::CannotApply> {
        let valid = self.valid_rarities();
        if !valid.contains(item.rarity) {
            return Err(crate::currency::CannotApply::WrongRarity {
                item_rarity: item.rarity,
                expected: valid,
            });
        }
        if item.mirrored {
            return Err(crate::currency::CannotApply::Mirrored);
        }
        if item.hinekora_lock.is_some() {
            return Err(crate::currency::CannotApply::AlreadyLocked);
        }
        Ok(())
    }

    fn apply(&self, item: &mut Item, ctx: &mut ApplyContext<'_>) -> EngineResult<ApplyOutcome> {
        if !item.is_modifiable() {
            return Err(EngineError::InvalidApplication(
                "Hinekora's Lock requires a modifiable item".into(),
            ));
        }
        if item.hinekora_lock.is_some() {
            return Err(EngineError::InvalidApplication(
                "Hinekora's Lock: item already bound by a lock".into(),
            ));
        }
        let seed: u64 = ctx.rng.gen();
        item.hinekora_lock = Some(seed);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use rand::SeedableRng;
    use rand_xoshiro::Xoshiro256PlusPlus;
    use smallvec::smallvec;

    use super::*;
    use crate::ids::ItemClassId;
    use crate::item::{QualityKind, Rarity};
    use crate::patch::PatchVersion;
    use crate::registry::ModRegistry;

    fn fixture_item() -> Item {
        Item {
            base: ItemClassId::from("Boots").as_str().into(),
            ilvl: 82,
            rarity: Rarity::Rare,
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

    fn ctx<'a>(
        reg: &'a ModRegistry,
        rng: &'a mut Xoshiro256PlusPlus,
        omens: &'a mut crate::omen::OmenSet,
    ) -> ApplyContext<'a> {
        ApplyContext::new(reg, rng, PatchVersion::PATCH_0_4_0, omens)
    }

    #[test]
    fn lock_sets_seed_on_clean_item() {
        let reg = ModRegistry::from_mods(vec![]);
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0x1);
        let mut omens = crate::omen::OmenSet::new();
        let mut item = fixture_item();
        HinekorasLock::new()
            .apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens))
            .unwrap();
        assert!(item.hinekora_lock.is_some());
    }

    #[test]
    fn lock_rejects_corrupted() {
        let reg = ModRegistry::from_mods(vec![]);
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0x2);
        let mut omens = crate::omen::OmenSet::new();
        let mut item = fixture_item();
        item.corrupted = true;
        let r = HinekorasLock::new().apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens));
        assert!(matches!(r, Err(EngineError::InvalidApplication(_))));
    }

    #[test]
    fn lock_rejects_mirrored() {
        let reg = ModRegistry::from_mods(vec![]);
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0x3);
        let mut omens = crate::omen::OmenSet::new();
        let mut item = fixture_item();
        item.mirrored = true;
        let r = HinekorasLock::new().apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens));
        assert!(matches!(r, Err(EngineError::InvalidApplication(_))));
    }

    #[test]
    fn lock_rejects_already_locked() {
        let reg = ModRegistry::from_mods(vec![]);
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0x4);
        let mut omens = crate::omen::OmenSet::new();
        let mut item = fixture_item();
        item.hinekora_lock = Some(42);
        let r = HinekorasLock::new().apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens));
        assert!(matches!(r, Err(EngineError::InvalidApplication(_))));
    }

    #[test]
    fn lock_seed_is_drawn_from_ctx_rng_deterministically() {
        let reg = ModRegistry::from_mods(vec![]);

        let mut rng_a = Xoshiro256PlusPlus::seed_from_u64(0x00c0_ffee);
        let mut omens_a = crate::omen::OmenSet::new();
        let mut a = fixture_item();
        HinekorasLock::new()
            .apply(&mut a, &mut ctx(&reg, &mut rng_a, &mut omens_a))
            .unwrap();

        let mut rng_b = Xoshiro256PlusPlus::seed_from_u64(0x00c0_ffee);
        let mut omens_b = crate::omen::OmenSet::new();
        let mut b = fixture_item();
        HinekorasLock::new()
            .apply(&mut b, &mut ctx(&reg, &mut rng_b, &mut omens_b))
            .unwrap();

        assert_eq!(a.hinekora_lock, b.hinekora_lock);
    }
}
