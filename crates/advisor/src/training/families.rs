//! Currency-family classification shared by the training layers.
//!
//! [`StateActionAlias`](crate::training::model_learner::StateActionAlias)
//! (afterstate aliasing) and the analytic transition builder
//! ([`crate::training::analytic_model`]) both need to know which mechanic
//! family a currency id belongs to. Keeping the classification in one
//! place prevents the two from drifting (the historical shape was five
//! ad-hoc string matchers inside `model_learner`).
//!
//! Orb ids are matched **exactly** against the `Currency::id` values
//! defined in `poc2_engine::currency::{basic, variants}`. Essences are
//! catalogue-driven (bundle data, ids like `GreaterEssenceOfBattle` /
//! `PerfectEssenceOfSeeking`), so they're recognized by the documented
//! `"Essence"` substring convention the pipeline emits.

/// Mechanic family of a currency id, as far as training semantics care.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OrbFamily {
    /// Normal → Magic, add 1 mod (all tiers).
    Transmute,
    /// Magic, fill the empty slot (all tiers).
    Augment,
    /// Magic → Rare, add 1 mod (all tiers).
    Regal,
    /// Rare, add 1 mod (all tiers).
    Exalt,
    /// Rare, remove-one-add-one (all tiers).
    Chaos,
    /// Magic/Rare, remove 1 uniform mod.
    Annul,
    /// Value reroll only — feature-space identity.
    Divine,
    /// Catalogue essence (any quality tier).
    Essence,
}

/// Classify a currency id string into an [`OrbFamily`]. `None` for
/// anything the training layers have no special handling for (alloys,
/// bones, catalysts, Vaal, plugins, …).
pub(crate) fn classify(id: &str) -> Option<OrbFamily> {
    match id {
        "OrbOfTransmutation" | "GreaterOrbOfTransmutation" | "PerfectOrbOfTransmutation" => {
            Some(OrbFamily::Transmute)
        }
        "OrbOfAugmentation" | "GreaterOrbOfAugmentation" | "PerfectOrbOfAugmentation" => {
            Some(OrbFamily::Augment)
        }
        "RegalOrb" | "GreaterRegalOrb" | "PerfectRegalOrb" => Some(OrbFamily::Regal),
        "ExaltedOrb" | "GreaterExaltedOrb" | "PerfectExaltedOrb" => Some(OrbFamily::Exalt),
        "ChaosOrb" | "GreaterChaosOrb" | "PerfectChaosOrb" => Some(OrbFamily::Chaos),
        "OrbOfAnnulment" => Some(OrbFamily::Annul),
        "DivineOrb" => Some(OrbFamily::Divine),
        s if s.contains("Essence") => Some(OrbFamily::Essence),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_covers_all_tiers_and_essences() {
        for (id, family) in [
            ("OrbOfTransmutation", OrbFamily::Transmute),
            ("PerfectOrbOfTransmutation", OrbFamily::Transmute),
            ("GreaterOrbOfAugmentation", OrbFamily::Augment),
            ("RegalOrb", OrbFamily::Regal),
            ("PerfectExaltedOrb", OrbFamily::Exalt),
            ("GreaterChaosOrb", OrbFamily::Chaos),
            ("OrbOfAnnulment", OrbFamily::Annul),
            ("DivineOrb", OrbFamily::Divine),
            ("GreaterEssenceOfBattle", OrbFamily::Essence),
            ("PerfectEssenceOfSeeking", OrbFamily::Essence),
        ] {
            assert_eq!(classify(id), Some(family), "{id}");
        }
        assert_eq!(classify("VaalOrb"), None);
        assert_eq!(classify("FracturingOrb"), None);
        assert_eq!(classify("VerisiumAlloyOfTheArchon"), None);
    }
}
