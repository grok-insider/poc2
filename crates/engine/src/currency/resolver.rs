//! Currency resolver: map [`CurrencyId`] strings to concrete
//! [`Currency`] trait objects.
//!
//! Used by the advisor and any consumer that needs to dispatch on a
//! data-driven currency reference. The mapping itself is pure mechanism
//! (no policy), so it lives in the engine alongside the currency
//! implementations themselves.
//!
//! ## Coverage
//!
//! - **Basic orbs**: Transmute, Augment, Regal, Alchemy, Exalt, Annul,
//!   Chaos, Divine, Vaal.
//! - **Greater / Perfect tiers**: Transmute, Augment, Regal, Exalt, Chaos.
//! - **Specialty currencies**: Hinekora's Lock, Fracturing Orb.
//! - **Bones**: parsed from the canonical `{Size}{Subtype}` id format
//!   (e.g., `"PreservedRib"`, `"AncientJawbone"`).
//! - **Essences**: looked up from a caller-supplied catalogue. The bundle
//!   carries the full essence list (id + display name + quality + target
//!   mod); the resolver clones the matching entry into a trait object.
//!
//! Catalysts, Verisium Alloys and Distilled Emotions are likewise
//! data-driven from caller-supplied catalogues (`with_catalysts` /
//! `with_alloys`). Ids the resolver does not recognize resolve to `None` —
//! calling code should treat that as "we can't simulate this currency yet"
//! and skip it rather than crash.

use crate::currency::basic::{
    ChaosOrb, DivineOrb, ExaltedOrb, OrbOfAlchemy, OrbOfAnnulment, OrbOfAugmentation,
    OrbOfTransmutation, RegalOrb,
};
use crate::currency::bone::Bone;
use crate::currency::catalyst::Catalyst;
use crate::currency::essence::Essence;
use crate::currency::fracturing::FracturingOrb;
use crate::currency::hinekora::HinekorasLock;
use crate::currency::vaal::VaalOrb;
use crate::currency::variants::{
    GreaterChaosOrb, GreaterExaltedOrb, GreaterOrbOfAugmentation, GreaterOrbOfTransmutation,
    GreaterRegalOrb, PerfectChaosOrb, PerfectExaltedOrb, PerfectOrbOfAugmentation,
    PerfectOrbOfTransmutation, PerfectRegalOrb,
};
use crate::currency::Currency;
use crate::ids::CurrencyId;
use crate::item::{BoneSize, BoneSubtype};

/// Map [`CurrencyId`] strings to concrete [`Currency`] trait objects.
pub trait CurrencyResolver: Send + Sync {
    /// Resolve a currency id to a fresh trait object. Returns `None` if the
    /// id is not recognized (caller decides whether to error or skip).
    fn resolve(&self, id: &CurrencyId) -> Option<Box<dyn Currency>>;

    /// The Verisium Alloy catalogue this resolver carries (0.5+). Used by
    /// the advisor's candidate generator to enumerate alloy moves; default
    /// empty so non-bundle resolvers stay unaffected.
    fn alloys(&self) -> &[crate::currency::Alloy] {
        &[]
    }
}

/// Default resolver covering all currencies the engine implements today.
///
/// Construct with [`DefaultCurrencyResolver::new`] for the basic catalogue,
/// or [`DefaultCurrencyResolver::with_essences`] to additionally support
/// essence lookups from a bundle.
#[derive(Debug, Default, Clone)]
pub struct DefaultCurrencyResolver {
    essences: Vec<Essence>,
    catalysts: Vec<Catalyst>,
    /// Verisium Alloys (0.5+) — data-driven, like essences.
    alloys: Vec<crate::currency::Alloy>,
}

impl DefaultCurrencyResolver {
    /// Build a fresh resolver with no essence catalogue. Essence ids will
    /// resolve to `None`.
    #[must_use]
    pub fn new() -> Self {
        let mut s = Self::default();
        s.register_default_catalyst_presets();
        s
    }

    /// Attach an essence catalogue. Each `Essence` is matched by exact
    /// `CurrencyId` equality.
    #[must_use]
    pub fn with_essences(mut self, essences: Vec<Essence>) -> Self {
        self.essences = essences;
        self
    }

    /// Attach a catalyst catalogue. Bundle entries override same-id
    /// presets; presets without a bundle counterpart survive the merge —
    /// CoE only exports the 12 base catalysts, so a wholesale replace
    /// would silently drop the jewel-gated `Refined` variants (M14 audit).
    #[must_use]
    pub fn with_catalysts(mut self, catalysts: Vec<Catalyst>) -> Self {
        for c in catalysts {
            if let Some(slot) = self.catalysts.iter_mut().find(|p| p.id() == c.id()) {
                *slot = c;
            } else {
                self.catalysts.push(c);
            }
        }
        self
    }

    /// Add a single essence to the catalogue.
    pub fn add_essence(&mut self, essence: Essence) {
        self.essences.push(essence);
    }

    /// Add a single catalyst to the catalogue.
    pub fn add_catalyst(&mut self, catalyst: Catalyst) {
        self.catalysts.push(catalyst);
    }

    /// Attach a Verisium Alloy catalogue (0.5+). Each `Alloy` is matched by
    /// exact `CurrencyId` equality.
    #[must_use]
    pub fn with_alloys(mut self, alloys: Vec<crate::currency::Alloy>) -> Self {
        self.alloys = alloys;
        self
    }

    /// Add a single Verisium Alloy to the catalogue.
    pub fn add_alloy(&mut self, alloy: crate::currency::Alloy) {
        self.alloys.push(alloy);
    }

    /// Pre-populate the resolver with the engine's catalyst presets so
    /// strategies / rules referring to `FleshCatalyst`, `ReaverCatalyst`,
    /// etc. resolve out of the box. Production callers can extend with
    /// the full bundle catalogue via [`with_catalysts`].
    ///
    /// The preset set is the full PoE2 0.5 catalogue (poe2db
    /// catalysts.html): 12 base catalysts (ring/amulet-gated) plus their
    /// 12 `Refined` variants (jewel-gated). The PoE1 names
    /// `IntrinsicCatalyst` / `UnstableCatalyst` do not exist in 0.5 and
    /// intentionally resolve to `None`.
    fn register_default_catalyst_presets(&mut self) {
        self.catalysts.extend(Catalyst::default_catalogue());
    }
}

impl CurrencyResolver for DefaultCurrencyResolver {
    fn alloys(&self) -> &[crate::currency::Alloy] {
        &self.alloys
    }

    fn resolve(&self, id: &CurrencyId) -> Option<Box<dyn Currency>> {
        let s = id.as_str();

        // Basic and tier-variant orbs — exact-string match.
        match s {
            "OrbOfTransmutation" => return Some(Box::new(OrbOfTransmutation::new())),
            "GreaterOrbOfTransmutation" => return Some(Box::new(GreaterOrbOfTransmutation::new())),
            "PerfectOrbOfTransmutation" => return Some(Box::new(PerfectOrbOfTransmutation::new())),
            "OrbOfAugmentation" => return Some(Box::new(OrbOfAugmentation::new())),
            "GreaterOrbOfAugmentation" => return Some(Box::new(GreaterOrbOfAugmentation::new())),
            "PerfectOrbOfAugmentation" => return Some(Box::new(PerfectOrbOfAugmentation::new())),
            "RegalOrb" => return Some(Box::new(RegalOrb::new())),
            "GreaterRegalOrb" => return Some(Box::new(GreaterRegalOrb::new())),
            "PerfectRegalOrb" => return Some(Box::new(PerfectRegalOrb::new())),
            "OrbOfAlchemy" => return Some(Box::new(OrbOfAlchemy::new())),
            "ExaltedOrb" => return Some(Box::new(ExaltedOrb::new())),
            "GreaterExaltedOrb" => return Some(Box::new(GreaterExaltedOrb::new())),
            "PerfectExaltedOrb" => return Some(Box::new(PerfectExaltedOrb::new())),
            "OrbOfAnnulment" => return Some(Box::new(OrbOfAnnulment::new())),
            "ChaosOrb" => return Some(Box::new(ChaosOrb::new())),
            "GreaterChaosOrb" => return Some(Box::new(GreaterChaosOrb::new())),
            "PerfectChaosOrb" => return Some(Box::new(PerfectChaosOrb::new())),
            "DivineOrb" => return Some(Box::new(DivineOrb::new())),
            "VaalOrb" => return Some(Box::new(VaalOrb::new())),
            "HinekorasLock" => return Some(Box::new(HinekorasLock::new())),
            "FracturingOrb" => return Some(Box::new(FracturingOrb::new())),
            _ => {}
        }

        // Bones — `{Size}{Subtype}` canonical naming.
        if let Some((size, subtype)) = parse_bone_id(s) {
            return Some(Box::new(Bone::new(size, subtype)));
        }

        // Catalysts — preset + caller-supplied catalogue.
        if let Some(c) = self.catalysts.iter().find(|c| c.id().as_str() == s) {
            return Some(Box::new(c.clone()));
        }

        // Essences — caller-supplied catalogue.
        if let Some(e) = self.essences.iter().find(|e| e.id().as_str() == s) {
            return Some(Box::new(e.clone()));
        }

        // Verisium Alloys (0.5+) — caller-supplied catalogue.
        if let Some(a) = self.alloys.iter().find(|a| a.id().as_str() == s) {
            return Some(Box::new(a.clone()));
        }

        None
    }
}

fn parse_bone_id(s: &str) -> Option<(BoneSize, BoneSubtype)> {
    let (size_str, rest) = if let Some(rest) = s.strip_prefix("Gnawed") {
        ("Gnawed", rest)
    } else if let Some(rest) = s.strip_prefix("Preserved") {
        ("Preserved", rest)
    } else if let Some(rest) = s.strip_prefix("Ancient") {
        ("Ancient", rest)
    } else if let Some(rest) = s.strip_prefix("Altered") {
        // 0.5 breach desecration: only "Altered Collarbone" exists.
        ("Altered", rest)
    } else {
        return None;
    };
    let size = match size_str {
        "Gnawed" => BoneSize::Gnawed,
        "Preserved" => BoneSize::Preserved,
        "Ancient" => BoneSize::Ancient,
        "Altered" => BoneSize::Altered,
        _ => unreachable!(),
    };
    let subtype = match rest {
        "Rib" => BoneSubtype::Rib,
        "Jawbone" => BoneSubtype::Jawbone,
        "Collarbone" => BoneSubtype::Collarbone,
        "Cranium" => BoneSubtype::Cranium,
        _ => return None,
    };
    // Size × subtype combinations that do not exist as currency items
    // (Gnawed/Ancient Cranium, Altered non-Collarbone) do not resolve.
    if !size.valid_with(subtype) {
        return None;
    }
    Some((size, subtype))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::currency::essence::EssenceQuality;
    use crate::ids::ModId;

    #[test]
    fn resolves_basic_orbs() {
        let r = DefaultCurrencyResolver::new();
        assert!(r.resolve(&CurrencyId::from("OrbOfTransmutation")).is_some());
        assert!(r.resolve(&CurrencyId::from("RegalOrb")).is_some());
        assert!(r.resolve(&CurrencyId::from("ChaosOrb")).is_some());
        assert!(r.resolve(&CurrencyId::from("DivineOrb")).is_some());
        assert!(r.resolve(&CurrencyId::from("VaalOrb")).is_some());
    }

    #[test]
    fn resolves_perfect_tier() {
        let r = DefaultCurrencyResolver::new();
        for id in [
            "PerfectOrbOfTransmutation",
            "PerfectOrbOfAugmentation",
            "PerfectRegalOrb",
            "PerfectExaltedOrb",
            "PerfectChaosOrb",
        ] {
            assert!(
                r.resolve(&CurrencyId::from(id)).is_some(),
                "did not resolve {id}"
            );
        }
    }

    #[test]
    fn resolves_specialty_currencies() {
        let r = DefaultCurrencyResolver::new();
        assert!(r.resolve(&CurrencyId::from("HinekorasLock")).is_some());
        assert!(r.resolve(&CurrencyId::from("FracturingOrb")).is_some());
    }

    #[test]
    fn resolves_bones_by_canonical_id() {
        let r = DefaultCurrencyResolver::new();
        for id in [
            "GnawedRib",
            "PreservedRib",
            "AncientRib",
            "PreservedJawbone",
            "PreservedCollarbone",
            "PreservedCranium",
        ] {
            assert!(
                r.resolve(&CurrencyId::from(id)).is_some(),
                "did not resolve bone {id}"
            );
        }
    }

    #[test]
    fn unknown_id_returns_none() {
        let r = DefaultCurrencyResolver::new();
        assert!(r
            .resolve(&CurrencyId::from("NonexistentCurrency"))
            .is_none());
        // Bone parser shouldn't accept arbitrary strings.
        assert!(r.resolve(&CurrencyId::from("PreservedFemur")).is_none());
        assert!(r.resolve(&CurrencyId::from("EternalRib")).is_none());
    }

    #[test]
    fn essence_catalogue_is_consulted() {
        let essence = Essence::new(
            "PerfectEssenceOfSeeking",
            "Perfect Essence of Seeking",
            EssenceQuality::Perfect,
            ModId::from("PerfectSeekingMod"),
        );
        let r = DefaultCurrencyResolver::new().with_essences(vec![essence]);
        assert!(r
            .resolve(&CurrencyId::from("PerfectEssenceOfSeeking"))
            .is_some());
        assert!(r
            .resolve(&CurrencyId::from("PerfectEssenceOfBattle"))
            .is_none());
    }

    #[test]
    fn default_catalyst_presets_resolve() {
        let r = DefaultCurrencyResolver::new();
        // The full 0.5 catalogue resolves out of the box (base + Refined).
        for catalyst in Catalyst::default_catalogue() {
            assert!(
                r.resolve(catalyst.id()).is_some(),
                "did not resolve catalyst {}",
                catalyst.id().as_str()
            );
        }
        for id in [
            "FleshCatalyst",
            "ReaverCatalyst",
            "AdaptiveCatalyst",
            "RefinedFleshCatalyst",
            "RefinedAdaptiveCatalyst",
        ] {
            assert!(
                r.resolve(&CurrencyId::from(id)).is_some(),
                "did not resolve catalyst {id}"
            );
        }
    }

    #[test]
    fn poe1_catalyst_names_do_not_resolve() {
        // "Intrinsic Catalyst" / "Unstable Catalyst" are PoE1 names that
        // do not exist in PoE2 0.5 (poe2db catalysts.html).
        let r = DefaultCurrencyResolver::new();
        assert!(r.resolve(&CurrencyId::from("IntrinsicCatalyst")).is_none());
        assert!(r.resolve(&CurrencyId::from("UnstableCatalyst")).is_none());
    }

    #[test]
    fn catalyst_catalogue_extension_works() {
        let r = DefaultCurrencyResolver::new().with_catalysts(vec![Catalyst::adaptive()]);
        assert!(r.resolve(&CurrencyId::from("AdaptiveCatalyst")).is_some());
    }

    #[test]
    fn alloy_catalogue_round_trips_via_with_alloys() {
        let alloy = crate::currency::Alloy::new(
            "AlloyRunicWard",
            "Verisium Alloy of Runic Ward",
            ModId::from("RunicWardCrafted"),
        );
        let r = DefaultCurrencyResolver::new().with_alloys(vec![alloy]);
        assert!(
            r.resolve(&CurrencyId::from("AlloyRunicWard")).is_some(),
            "seeded alloy id must resolve"
        );
        assert!(
            r.resolve(&CurrencyId::from("AlloyUnknown")).is_none(),
            "unseeded alloy id must not resolve"
        );
    }

    #[test]
    fn add_alloy_extends_the_catalogue() {
        let mut r = DefaultCurrencyResolver::new();
        assert!(r.resolve(&CurrencyId::from("AlloyOfBattle")).is_none());
        r.add_alloy(crate::currency::Alloy::new(
            "AlloyOfBattle",
            "Verisium Alloy of Battle",
            ModId::from("BattleCrafted"),
        ));
        assert!(r.resolve(&CurrencyId::from("AlloyOfBattle")).is_some());
    }
}
