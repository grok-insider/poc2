//! [`Stash`] — what currencies the user has on hand.
//!
//! The advisor consults the stash to filter candidate actions: if a
//! suggested action requires a currency the user does not own, the
//! candidate is dropped from the beam (we won't recommend something the
//! user can't actually do).
//!
//! For v1, the stash is a flat `HashMap<CurrencyId, u32>` of counts.
//! M6+ adds omens, essences-by-target-mod, bones-by-subtype, and the
//! "buyable now" view from poe2scout.

use std::collections::HashMap;

use poc2_engine::ids::{CurrencyId, OmenId};
use serde::{Deserialize, Serialize};

/// What the user currently owns.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Stash {
    #[serde(default)]
    currencies: HashMap<CurrencyId, u32>,
    #[serde(default)]
    omens: HashMap<OmenId, u32>,
    /// Treat as having unlimited stash (typical for dry-run / "what if").
    /// When true, [`Stash::has_currency`] and [`Stash::has_omen`] always
    /// return true; the planner won't filter by stash availability.
    #[serde(default)]
    pub unlimited: bool,
}

impl Stash {
    /// A new empty stash. The user owns nothing.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Pretend the user has unlimited stash (for advisor "what if"
    /// browsing and tests).
    #[must_use]
    pub fn unlimited() -> Self {
        Self {
            unlimited: true,
            ..Self::default()
        }
    }

    /// Set the count of a currency.
    pub fn set_currency(&mut self, id: impl Into<CurrencyId>, count: u32) {
        self.currencies.insert(id.into(), count);
    }

    /// Set the count of an omen.
    pub fn set_omen(&mut self, id: impl Into<OmenId>, count: u32) {
        self.omens.insert(id.into(), count);
    }

    /// Read currency count (0 if unknown).
    #[must_use]
    pub fn currency_count(&self, id: &CurrencyId) -> u32 {
        if self.unlimited {
            return u32::MAX;
        }
        self.currencies.get(id).copied().unwrap_or(0)
    }

    /// Read omen count (0 if unknown).
    #[must_use]
    pub fn omen_count(&self, id: &OmenId) -> u32 {
        if self.unlimited {
            return u32::MAX;
        }
        self.omens.get(id).copied().unwrap_or(0)
    }

    /// True iff the user owns at least one of `id`.
    #[must_use]
    pub fn has_currency(&self, id: &CurrencyId) -> bool {
        self.unlimited || self.currency_count(id) > 0
    }

    /// True iff the user owns at least one of `id`.
    #[must_use]
    pub fn has_omen(&self, id: &OmenId) -> bool {
        self.unlimited || self.omen_count(id) > 0
    }

    /// True iff the user can afford the entire `(currency, omens)` combo
    /// at least once.
    #[must_use]
    pub fn can_afford(&self, currency: &CurrencyId, omens: &[OmenId]) -> bool {
        if !self.has_currency(currency) {
            return false;
        }
        omens.iter().all(|o| self.has_omen(o))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_stash_owns_nothing() {
        let s = Stash::new();
        assert!(!s.has_currency(&CurrencyId::from("ChaosOrb")));
        assert_eq!(s.currency_count(&CurrencyId::from("ChaosOrb")), 0);
    }

    #[test]
    fn unlimited_owns_everything() {
        let s = Stash::unlimited();
        assert!(s.has_currency(&CurrencyId::from("Whatever")));
        assert!(s.has_omen(&OmenId::from("AnyOmen")));
    }

    #[test]
    fn set_then_query_currency() {
        let mut s = Stash::new();
        s.set_currency("DivineOrb", 5);
        assert_eq!(s.currency_count(&CurrencyId::from("DivineOrb")), 5);
        assert!(s.has_currency(&CurrencyId::from("DivineOrb")));
        assert!(!s.has_currency(&CurrencyId::from("ExaltedOrb")));
    }

    #[test]
    fn can_afford_checks_currency_and_all_omens() {
        let mut s = Stash::new();
        s.set_currency("PerfectExaltedOrb", 2);
        s.set_omen("OmenOfSinistralExaltation", 1);
        assert!(s.can_afford(
            &CurrencyId::from("PerfectExaltedOrb"),
            &[OmenId::from("OmenOfSinistralExaltation")],
        ));
        // Missing omen → cannot afford.
        assert!(!s.can_afford(
            &CurrencyId::from("PerfectExaltedOrb"),
            &[OmenId::from("OmenOfDextralExaltation")],
        ));
        // Missing currency → cannot afford even with omens.
        assert!(!s.can_afford(&CurrencyId::from("DivineOrb"), &[]));
    }
}
