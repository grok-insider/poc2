//! Mod-weight observations with confidence tracking.
//!
//! Real numerical mod weights are not in any official source (per
//! [ADR-0004](../../../docs/adr/0004-weight-strategy.md)). The pipeline pulls
//! candidate values from Craft of Exile (primary) and poe2db.tw (secondary)
//! and emits a [`WeightObservation`] per (mod, base, ilvl) — or per
//! (mod, base-class) when finer aggregation isn't available.
//!
//! These types live in `poc2-engine` (not `poc2-data`) so [`crate::registry::ModRegistry`]
//! can index them at construction time without inducing a `data → engine`
//! dependency cycle. `poc2-data` re-exports them for back-compat with bundle
//! serialization callers.
//!
//! The advisor uses `primary_weight` for ranking but widens its confidence
//! interval per [`Confidence`].

use serde::{Deserialize, Serialize};

use crate::ids::{BaseTypeId, ItemClassId, ModId};

/// Confidence level for a weight observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    /// Sources agree within ±5%.
    Verified,
    /// Sources agree within ±25%, OR a single trusted source.
    Community,
    /// Sources disagree >25%, OR only one weak source.
    Experimental,
}

impl Confidence {
    /// Half-width of the relative confidence interval — used by the advisor
    /// to widen probability estimates.
    pub const fn relative_interval(self) -> f64 {
        match self {
            Self::Verified => 0.05,
            Self::Community => 0.25,
            Self::Experimental => 0.5,
        }
    }
}

/// Scope of a weight observation. Most weights are per (mod × item-class),
/// some refine to per-base, and a small set are per (mod × base × ilvl)
/// because PoE2's spawn weights can drift with item level breakpoints.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum WeightScope {
    /// Weight applies to this mod on any item of the class.
    ItemClass { item_class: ItemClassId },
    /// Weight applies to this mod on this specific base.
    Base { base: BaseTypeId },
    /// Weight applies to this mod on this base at-or-above this ilvl.
    BaseAtIlvl { base: BaseTypeId, min_ilvl: u32 },
}

/// One numerical weight observation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeightObservation {
    pub mod_id: ModId,
    pub scope: WeightScope,
    /// Primary value (typically Craft of Exile).
    pub primary_weight: f64,
    /// Secondary cross-check (typically poe2db.tw).
    pub secondary_weight: Option<f64>,
    pub confidence: Confidence,
    /// Free-form provenance — which sources contributed, last refresh, etc.
    pub note: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confidence_intervals_are_monotone() {
        assert!(
            Confidence::Verified.relative_interval() < Confidence::Community.relative_interval()
        );
        assert!(
            Confidence::Community.relative_interval()
                < Confidence::Experimental.relative_interval()
        );
    }
}
