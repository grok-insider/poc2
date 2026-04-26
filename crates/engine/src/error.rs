//! Engine error types.

use thiserror::Error;

pub type EngineResult<T> = Result<T, EngineError>;

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("invalid currency application: {0}")]
    InvalidApplication(String),

    #[error("item is corrupted; further crafting is not allowed")]
    ItemCorrupted,

    #[error("item is sanctified; further crafting is not allowed")]
    ItemSanctified,

    #[error("required affix slot full ({affix_type})")]
    AffixSlotFull { affix_type: &'static str },

    #[error("no eligible mods to add (base={base}, ilvl={ilvl}, affix={affix_type})")]
    NoEligibleMods {
        base: String,
        ilvl: u32,
        affix_type: &'static str,
    },

    #[error("mod group exclusivity violated: {0}")]
    ModGroupExclusive(String),

    #[error("operation requires at least {required} mods, but item has {actual}")]
    InsufficientMods { required: u32, actual: u32 },

    #[error("cannot fracture: target mod is hidden (desecrated, unrevealed)")]
    FractureHiddenMod,

    #[error("cannot modify fractured mod: {0}")]
    FracturedModImmutable(String),

    #[error("omen incompatibility: {omen} cannot modify {currency}")]
    OmenIncompatible { omen: String, currency: String },

    #[error("patch mismatch: entity requires {required}, engine running {running}")]
    PatchMismatch { required: String, running: String },

    #[error("data error: {0}")]
    Data(String),

    #[error("{0}")]
    Other(String),
}
