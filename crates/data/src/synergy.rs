//! Synergy graph between currencies and omens.
//!
//! Most synergy edges are auto-derivable: every omen declares which
//! currency it modifies (`targets_currency`) and what its effect is. The
//! `SynergyOverride` module exists for state-dependent or wildcard cases
//! that don't fit the auto-derived pattern, e.g.:
//!
//! - **Hinekora's Lock** applies to *any* operation (wildcard).
//! - **Omen of Corruption** modifies the *outcome distribution* of Vaal Orb,
//!   not the orb's selection step.
//! - **Omen of Light** applies to Annulment only when desecrated mods are
//!   present (state-dependent).

use poc2_engine::{CurrencyId, OmenId};
use serde::{Deserialize, Serialize};

/// One edge in the auto-derived part of the graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SynergyEdge {
    pub omen: OmenId,
    pub currency: CurrencyId,
    /// Free-form description of the effect, mirrors the omen's documented
    /// behavior (e.g., `"adds only Prefix"`, `"removes lowest mod-level mod"`).
    /// The actual EffectFn is implemented in code; this string is for tooling.
    pub effect_summary: String,
}

/// A hand-curated override — applied after the auto-derived graph is built.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SynergyOverride {
    pub id: String,
    /// Which omen does this override touch? `None` = applies to all omens.
    pub omen: Option<OmenId>,
    /// Which currency does this override touch? `None` = applies to all currencies.
    pub currency: Option<CurrencyId>,
    /// What kind of override: add a new edge, remove an existing one, or
    /// modify the effect.
    pub kind: OverrideKind,
    pub note: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverrideKind {
    /// Add this (omen, currency) pairing — used for wildcard interactions
    /// like Hinekora's Lock.
    Add,
    /// Remove an auto-derived edge — used when an interaction looks legal
    /// but isn't (e.g., Omen X cannot pair with Currency Y in patch Z).
    Remove,
    /// Auto-derived edge exists but its effect needs re-routing — see `note`.
    Modify,
}
