//! Currency definitions and the `Currency` trait.
//!
//! Each PoE2 crafting orb / essence / bone / catalyst implements the
//! [`Currency`] trait. The trait is small on purpose: a currency takes the
//! item, a context, and produces a result. All randomness flows through the
//! context's RNG (deterministic for tests, real for production).
//!
//! ## Module layout
//!
//! - this file — trait, [`ApplyContext`], dispatch helpers
//! - [`basic`] — classic orbs (Transmute / Aug / Alch / Regal / Exalt / Chaos / Annul / Divine / Vaal) plus Greater / Perfect variants
//! - `essence` — essence application (lands in M2.5)
//! - `bone` — desecration bones + Well-of-Souls reveal (M2.5)
//! - `catalyst` — jewelry catalyst quality (M2.5)
//! - `fracturing` — Fracturing Orb (M2.5)
//! - `hinekora` — Hinekora's Lock preview / commit (M2.5)
//! - `recombinator` — Recombinator (M2.5)

pub mod basic;
pub mod bone;
pub mod catalyst;
pub mod essence;
pub mod fracturing;
pub mod hinekora;
pub mod recombinator;
pub mod resolver;

pub use bone::{reveal_at_well_of_souls, sample_reveal_options, Bone, RevealOptions};
pub use catalyst::{
    Catalyst, CATALYST_INCREMENT_ADAPTIVE, CATALYST_INCREMENT_DEFAULT, CATALYST_QUALITY_CAP,
};
pub use essence::{Essence, EssenceQuality};
pub use fracturing::FracturingOrb;
pub use hinekora::HinekorasLock;
pub use recombinator::recombine;
pub use resolver::{CurrencyResolver, DefaultCurrencyResolver};

use rand::RngCore;

use crate::error::EngineResult;
use crate::ids::CurrencyId;
use crate::item::Item;
use crate::patch::PatchVersion;
use crate::registry::ModRegistry;

/// What every currency operation produces.
///
/// Today this is just `()`; later (M2.6+) the engine returns rich outcome
/// metadata (e.g., "Vaal corrupted outcome variant 3 was sampled").
pub type ApplyOutcome = ();

/// Context passed to every `Currency::apply` invocation.
///
/// Holds the registry, RNG, current patch, and the active omen set.
/// Currencies consume omens from the set as part of their apply paths
/// (see [`crate::omen::OmenSet`] for the consumption helpers).
pub struct ApplyContext<'a> {
    pub registry: &'a ModRegistry,
    pub rng: &'a mut dyn RngCore,
    pub patch: PatchVersion,
    pub omens: &'a mut crate::omen::OmenSet,
}

impl<'a> ApplyContext<'a> {
    pub fn new(
        registry: &'a ModRegistry,
        rng: &'a mut dyn RngCore,
        patch: PatchVersion,
        omens: &'a mut crate::omen::OmenSet,
    ) -> Self {
        Self {
            registry,
            rng,
            patch,
            omens,
        }
    }
}

/// A crafting currency.
///
/// Implementations must be pure functions of `(self, item, ctx)` modulo
/// the RNG in `ctx`. Currencies do not own state of their own — they're
/// typically zero-sized structs or carry only configuration (e.g., the
/// minimum mod level for a Greater variant).
pub trait Currency: std::fmt::Debug + Send + Sync {
    /// Stable identifier (e.g., `"OrbOfTransmutation"`, `"PerfectExaltedOrb"`).
    fn id(&self) -> &CurrencyId;

    /// Human-readable display name.
    fn name(&self) -> &'static str;

    /// Apply this currency to the item in place.
    ///
    /// Errors carry diagnostic detail (`EngineError::*`); the advisor surfaces
    /// these as "this currency cannot be applied because ...".
    fn apply(&self, item: &mut Item, ctx: &mut ApplyContext<'_>) -> EngineResult<ApplyOutcome>;
}
