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
use crate::item::{Item, Rarity};
use crate::patch::PatchVersion;
use crate::registry::ModRegistry;

/// What every currency operation produces.
///
/// Today this is just `()`; later (M2.6+) the engine returns rich outcome
/// metadata (e.g., "Vaal corrupted outcome variant 3 was sampled").
pub type ApplyOutcome = ();

/// Bitmask of rarities a currency accepts. Used by the advisor to filter
/// illegal candidate steps before scoring (Phase A of crafter helper v2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RaritySet(u8);

impl RaritySet {
    pub const NONE: Self = Self(0);
    pub const NORMAL: Self = Self(1 << 0);
    pub const MAGIC: Self = Self(1 << 1);
    pub const RARE: Self = Self(1 << 2);
    pub const UNIQUE: Self = Self(1 << 3);

    pub const fn all() -> Self {
        Self(0b1111)
    }

    pub const fn contains(self, rarity: Rarity) -> bool {
        let bit = match rarity {
            Rarity::Normal => Self::NORMAL.0,
            Rarity::Magic => Self::MAGIC.0,
            Rarity::Rare => Self::RARE.0,
            Rarity::Unique => Self::UNIQUE.0,
        };
        (self.0 & bit) != 0
    }

    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    pub fn iter(self) -> impl Iterator<Item = Rarity> {
        [Rarity::Normal, Rarity::Magic, Rarity::Rare, Rarity::Unique]
            .into_iter()
            .filter(move |r| self.contains(*r))
    }
}

/// Reasons a `can_apply_to` precondition check rejects an action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CannotApply {
    /// Item rarity is not in the currency's accepted set.
    WrongRarity {
        item_rarity: Rarity,
        expected: RaritySet,
    },
    /// All affix slots of the relevant kind are full.
    NoOpenSlots { affix: crate::item::AffixType },
    /// Item is corrupted and the currency cannot apply to corrupted items.
    Corrupted,
    /// Item is mirrored and cannot be modified by the currency.
    Mirrored,
    /// Item already has Hinekora's Lock active.
    AlreadyLocked,
    /// Fracturing requires ≥ 4 visible mods, none yet fractured.
    FractureRequiresFourMods { current: usize },
    /// Recombinator inputs do not share base/ilvl.
    RecombinatorInputMismatch,
    /// The action is not currently representable (deprecated path).
    Other(&'static str),
}

impl std::fmt::Display for CannotApply {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WrongRarity {
                item_rarity,
                expected,
            } => {
                let names: Vec<&str> = expected
                    .iter()
                    .map(|r| match r {
                        Rarity::Normal => "Normal",
                        Rarity::Magic => "Magic",
                        Rarity::Rare => "Rare",
                        Rarity::Unique => "Unique",
                    })
                    .collect();
                write!(
                    f,
                    "wrong rarity: item is {item_rarity:?}, expected one of [{}]",
                    names.join(", ")
                )
            }
            Self::NoOpenSlots { affix } => write!(f, "no open {affix:?} slot"),
            Self::Corrupted => write!(f, "item is corrupted"),
            Self::Mirrored => write!(f, "item is mirrored"),
            Self::AlreadyLocked => write!(f, "Hinekora's Lock already active"),
            Self::FractureRequiresFourMods { current } => {
                write!(f, "fracture requires 4 visible mods, item has {current}")
            }
            Self::RecombinatorInputMismatch => {
                write!(f, "recombinator inputs must share base and ilvl")
            }
            Self::Other(s) => write!(f, "{s}"),
        }
    }
}

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

    /// Rarities this currency accepts as input. Used by the advisor to skip
    /// illegal candidates *before* invoking `apply`. Default = all rarities;
    /// each concrete currency overrides with the correct subset.
    fn valid_rarities(&self) -> RaritySet {
        RaritySet::all()
    }

    /// Pre-flight check used by the advisor's candidate generator.
    ///
    /// Returns `Ok(())` when the action is currently applicable, otherwise
    /// returns a structured `CannotApply` reason the UI can surface. Default
    /// implementation enforces only the rarity gate; currencies with extra
    /// preconditions (slot capacity, fracture/lock state, etc.) override.
    fn can_apply_to(&self, item: &Item) -> Result<(), CannotApply> {
        let valid = self.valid_rarities();
        if !valid.contains(item.rarity) {
            return Err(CannotApply::WrongRarity {
                item_rarity: item.rarity,
                expected: valid,
            });
        }
        if item.mirrored {
            return Err(CannotApply::Mirrored);
        }
        Ok(())
    }
}
