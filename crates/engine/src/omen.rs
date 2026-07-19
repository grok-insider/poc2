//! Omen system.
//!
//! Omens are "buffs" that modify the next compatible currency operation.
//! Activate one in the inventory (the in-game right-click step) — the
//! engine models this by populating an [`OmenSet`] on the apply context.
//! When a compatible currency consumes an omen, the omen is removed from
//! the active set (one-shot semantics).
//!
//! ## Omen catalogue (M2.6 scope)
//!
//! | Omen | Patch range | Effect |
//! |---|---|---|
//! | Sinistral / Dextral Exaltation | 0.3+ | Exalt adds only Prefix / Suffix |
//! | Greater Exaltation | 0.3+ | Exalt adds 2 random mods (single charge) |
//! | Sinistral / Dextral Annulment | 0.3+ | Annul removes only Prefix / Suffix |
//! | Sinistral / Dextral Erasure | 0.3+ | Chaos removes only Prefix / Suffix |
//! | Sinistral / Dextral Crystallisation | 0.3+ | Perfect Essence removes only Prefix / Suffix |
//! | Sinistral / Dextral Necromancy | 0.3+ | Bone adds Prefix / Suffix |
//! | Whittling | 0.3+ | Chaos removes lowest-mod-level mod |
//! | Light | 0.3+ | Annul removes only Desecrated mods |
//! | Abyssal Echoes | 0.3+ | Reveal offers a re-rolled set of options |
//! | Corruption | 0.3+ | Vaal cannot result in NoChange |
//! | Sanctification | 0.3+ | Divine rolls 80-120% range, then sanctifies |
//! | Blessed | 0.3+ | Divine rerolls only implicit modifier |
//! | Catalysing Exaltation | 0.3+ | Exalt consumes catalyst quality to bias toward tag |
//! | Blackblooded / Liege / Sovereign | 0.3+ | Bone grants Kurgal / Amanamu / Ulaman mod |
//! | Homogenising Exaltation | **0.3.x only** | Exalt adds same-type as existing mod |
//! | Homogenising Coronation | **0.3.x only** | Regal adds same-type as existing mod |

use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

use crate::ids::OmenId;
use crate::item::{AbyssLord, AffixType};
use crate::patch::{PatchRange, PatchVersion};

/// What an omen actually does to the next compatible operation.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OmenEffect {
    /// Restrict the next currency to act only on the given affix type.
    /// Used by Sinistral/Dextral Exaltation, Annulment, Erasure,
    /// Crystallisation, and Necromancy.
    AffixOnly(AffixType),

    /// Greater Exaltation: next Exalted Orb adds **two** random mods.
    GreaterExaltation,

    /// Whittling: next Chaos Orb removes the lowest-required-level mod.
    Whittling,

    /// Light: next Annul removes only desecrated mods.
    Light,

    /// Abyssal Echoes: next Reveal offers a re-rolled second set of options
    /// (caller can pick from either).
    AbyssalEchoes,

    /// Corruption: next Vaal cannot result in NoChange (rerolls if drawn).
    PreventNoChange,

    /// Sanctification: next Divine rolls 80-120% of the normal range and
    /// permanently sanctifies the item.
    Sanctification,

    /// Blessed: next Divine rerolls only the implicit modifier.
    Blessed,

    /// Catalysing Exaltation: next Exalt consumes all catalyst quality and
    /// biases the rolled mod toward the catalyst's tag.
    CatalystingExaltation,

    /// Lord-targeting bone omens: next Bone reveal grants a mod from the
    /// specified Abyss Lord's pool. Restricted to weapons & jewellery
    /// (per game rules).
    LordTarget(AbyssLord),

    /// Homogenising: next currency adds a mod sharing a tag with an
    /// existing mod on the item. **Disabled in 0.4** but legacy
    /// stockpiles still function — the engine evaluates the patch
    /// range before consumption.
    HomogenisingTagMatch,
}

/// One queued omen with its lifecycle data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Omen {
    pub id: OmenId,
    pub effect: OmenEffect,
    pub patch_range: PatchRange,
}

impl Omen {
    pub fn new(id: impl Into<OmenId>, effect: OmenEffect) -> Self {
        Self {
            id: id.into(),
            effect,
            patch_range: PatchRange::ALL,
        }
    }

    #[must_use]
    pub fn with_patch_range(mut self, r: PatchRange) -> Self {
        self.patch_range = r;
        self
    }
}

/// Set of currently-active omens, drained by currencies as they're consumed.
///
/// In typical use the player has 1-3 omens active at once. We keep them in
/// a small inline vector to avoid allocation in the hot path.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct OmenSet {
    active: SmallVec<[Omen; 4]>,
}

impl OmenSet {
    pub fn new() -> Self {
        Self::default()
    }

    /// Convenience: build from an iterator.
    pub fn from_iter_omens<I: IntoIterator<Item = Omen>>(it: I) -> Self {
        Self {
            active: it.into_iter().collect(),
        }
    }

    pub fn push(&mut self, o: Omen) {
        self.active.push(o);
    }

    pub fn is_empty(&self) -> bool {
        self.active.is_empty()
    }

    pub fn len(&self) -> usize {
        self.active.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Omen> {
        self.active.iter()
    }

    /// Find-and-remove a single omen matching `pred`. Honors the omen's
    /// patch range against `patch` — omens out of range are NOT consumed
    /// and the caller is told there's no matching active omen.
    pub fn consume<F>(&mut self, patch: PatchVersion, pred: F) -> Option<Omen>
    where
        F: Fn(&OmenEffect) -> bool,
    {
        let pos = self
            .active
            .iter()
            .position(|o| pred(&o.effect) && o.patch_range.contains(patch))?;
        Some(self.active.remove(pos))
    }

    // ---- Typed consumption helpers (called from currencies) ---------------

    /// AffixOnly(_): consumes the first omen restricting an affix type to
    /// the requested currency category. Returns the affix to restrict to.
    pub fn consume_affix_only(&mut self, patch: PatchVersion) -> Option<AffixType> {
        let o = self.consume(patch, |e| matches!(e, OmenEffect::AffixOnly(_)))?;
        if let OmenEffect::AffixOnly(a) = o.effect {
            Some(a)
        } else {
            None
        }
    }

    pub fn consume_greater_exaltation(&mut self, patch: PatchVersion) -> bool {
        self.consume(patch, |e| matches!(e, OmenEffect::GreaterExaltation))
            .is_some()
    }

    pub fn consume_whittling(&mut self, patch: PatchVersion) -> bool {
        self.consume(patch, |e| matches!(e, OmenEffect::Whittling))
            .is_some()
    }

    pub fn consume_light(&mut self, patch: PatchVersion) -> bool {
        self.consume(patch, |e| matches!(e, OmenEffect::Light))
            .is_some()
    }

    pub fn consume_abyssal_echoes(&mut self, patch: PatchVersion) -> bool {
        self.consume(patch, |e| matches!(e, OmenEffect::AbyssalEchoes))
            .is_some()
    }

    /// Consume an Omen of Corruption (suppresses the Vaal NoChange outcome).
    ///
    /// Cross-version league gate (P4): in **0.5 "Return of the Ancients"** the
    /// Omen of Corruption only appears / functions in **Standard** leagues —
    /// it is not available in the Runes of Aldur challenge league. So in 0.5+
    /// Challenge the omen is not consumed (no effect), mirroring the
    /// legacy-stockpile semantics already used for the Homogenising omens.
    pub fn consume_prevent_no_change(
        &mut self,
        patch: PatchVersion,
        league: crate::patch::League,
    ) -> bool {
        if patch >= PatchVersion::PATCH_0_5_0 && league != crate::patch::League::Standard {
            return false;
        }
        self.consume(patch, |e| matches!(e, OmenEffect::PreventNoChange))
            .is_some()
    }

    pub fn consume_sanctification(&mut self, patch: PatchVersion) -> bool {
        self.consume(patch, |e| matches!(e, OmenEffect::Sanctification))
            .is_some()
    }

    pub fn consume_blessed(&mut self, patch: PatchVersion) -> bool {
        self.consume(patch, |e| matches!(e, OmenEffect::Blessed))
            .is_some()
    }

    pub fn consume_catalysing(&mut self, patch: PatchVersion) -> bool {
        self.consume(patch, |e| matches!(e, OmenEffect::CatalystingExaltation))
            .is_some()
    }

    pub fn consume_lord_target(&mut self, patch: PatchVersion) -> Option<AbyssLord> {
        let o = self.consume(patch, |e| matches!(e, OmenEffect::LordTarget(_)))?;
        if let OmenEffect::LordTarget(l) = o.effect {
            Some(l)
        } else {
            None
        }
    }

    pub fn consume_homogenising(&mut self, patch: PatchVersion) -> bool {
        // Homogenising omens are 0.3.x-only. The patch_range filter inside
        // `consume` handles this transparently — they're never returned
        // for current-patch (0.4+) consumption even when present in the
        // active set (legacy stockpile semantics).
        self.consume(patch, |e| matches!(e, OmenEffect::HomogenisingTagMatch))
            .is_some()
    }
}

// =========================================================================
// Pre-built omens (the canonical 0.3+/0.4 set)
// =========================================================================

impl Omen {
    pub fn sinistral_exaltation() -> Self {
        Self::new(
            "OmenOfSinistralExaltation",
            OmenEffect::AffixOnly(AffixType::Prefix),
        )
    }
    pub fn dextral_exaltation() -> Self {
        Self::new(
            "OmenOfDextralExaltation",
            OmenEffect::AffixOnly(AffixType::Suffix),
        )
    }
    pub fn greater_exaltation() -> Self {
        Self::new("OmenOfGreaterExaltation", OmenEffect::GreaterExaltation)
    }
    pub fn sinistral_annulment() -> Self {
        Self::new(
            "OmenOfSinistralAnnulment",
            OmenEffect::AffixOnly(AffixType::Prefix),
        )
    }
    pub fn dextral_annulment() -> Self {
        Self::new(
            "OmenOfDextralAnnulment",
            OmenEffect::AffixOnly(AffixType::Suffix),
        )
    }
    pub fn sinistral_erasure() -> Self {
        Self::new(
            "OmenOfSinistralErasure",
            OmenEffect::AffixOnly(AffixType::Prefix),
        )
    }
    pub fn dextral_erasure() -> Self {
        Self::new(
            "OmenOfDextralErasure",
            OmenEffect::AffixOnly(AffixType::Suffix),
        )
    }
    pub fn sinistral_crystallisation() -> Self {
        Self::new(
            "OmenOfSinistralCrystallisation",
            OmenEffect::AffixOnly(AffixType::Prefix),
        )
    }
    pub fn dextral_crystallisation() -> Self {
        Self::new(
            "OmenOfDextralCrystallisation",
            OmenEffect::AffixOnly(AffixType::Suffix),
        )
    }
    pub fn sinistral_necromancy() -> Self {
        Self::new(
            "OmenOfSinistralNecromancy",
            OmenEffect::AffixOnly(AffixType::Prefix),
        )
    }
    pub fn dextral_necromancy() -> Self {
        Self::new(
            "OmenOfDextralNecromancy",
            OmenEffect::AffixOnly(AffixType::Suffix),
        )
    }
    pub fn whittling() -> Self {
        Self::new("OmenOfWhittling", OmenEffect::Whittling)
    }
    pub fn light() -> Self {
        Self::new("OmenOfLight", OmenEffect::Light)
    }
    pub fn abyssal_echoes() -> Self {
        Self::new("OmenOfAbyssalEchoes", OmenEffect::AbyssalEchoes)
    }
    pub fn corruption() -> Self {
        Self::new("OmenOfCorruption", OmenEffect::PreventNoChange)
    }
    pub fn sanctification() -> Self {
        Self::new("OmenOfSanctification", OmenEffect::Sanctification)
    }
    pub fn blessed() -> Self {
        Self::new("OmenOfTheBlessed", OmenEffect::Blessed)
    }
    pub fn catalysing_exaltation() -> Self {
        Self::new(
            "OmenOfCatalysingExaltation",
            OmenEffect::CatalystingExaltation,
        )
    }
    pub fn blackblooded() -> Self {
        Self::new(
            "OmenOfTheBlackblooded",
            OmenEffect::LordTarget(AbyssLord::Kurgal),
        )
    }
    pub fn liege() -> Self {
        Self::new("OmenOfTheLiege", OmenEffect::LordTarget(AbyssLord::Amanamu))
    }
    pub fn sovereign() -> Self {
        Self::new(
            "OmenOfTheSovereign",
            OmenEffect::LordTarget(AbyssLord::Ulaman),
        )
    }

    /// Disabled in 0.4 — we attach a `patch_max = 0.3.x` so the engine's
    /// patch-range filter prevents consumption on current-patch crafts.
    /// Players still own their stockpile of these but cannot use them
    /// in current-league crafting.
    pub fn homogenising_exaltation() -> Self {
        let patch_3_x_only = PatchRange::until(PatchVersion::new(0, 3, 255));
        Self::new(
            "OmenOfHomogenisingExaltation",
            OmenEffect::HomogenisingTagMatch,
        )
        .with_patch_range(patch_3_x_only)
    }
    pub fn homogenising_coronation() -> Self {
        let patch_3_x_only = PatchRange::until(PatchVersion::new(0, 3, 255));
        Self::new(
            "OmenOfHomogenisingCoronation",
            OmenEffect::HomogenisingTagMatch,
        )
        .with_patch_range(patch_3_x_only)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn omenset_consume_returns_and_removes() {
        let mut s = OmenSet::new();
        s.push(Omen::sinistral_exaltation());
        s.push(Omen::greater_exaltation());

        assert_eq!(s.len(), 2);
        let a = s.consume_affix_only(PatchVersion::PATCH_0_4_0);
        assert_eq!(a, Some(AffixType::Prefix));
        assert_eq!(s.len(), 1);
    }

    #[test]
    fn omenset_consume_returns_none_when_not_present() {
        let mut s = OmenSet::new();
        assert!(!s.consume_whittling(PatchVersion::PATCH_0_4_0));
        s.push(Omen::greater_exaltation());
        assert!(!s.consume_whittling(PatchVersion::PATCH_0_4_0));
    }

    #[test]
    fn homogenising_disabled_in_0_4() {
        // Player owns a Homogenising Exaltation (legacy stockpile).
        let mut s = OmenSet::new();
        s.push(Omen::homogenising_exaltation());
        // On 0.4.0 it is NOT consumed (effectively disabled).
        assert!(!s.consume_homogenising(PatchVersion::PATCH_0_4_0));
        // It's still in the set (we didn't burn it).
        assert_eq!(s.len(), 1);
        // On 0.3.0 it IS consumable.
        assert!(s.consume_homogenising(PatchVersion::new(0, 3, 0)));
        assert_eq!(s.len(), 0);
    }

    #[test]
    fn homogenising_coronation_disabled_in_0_4() {
        let mut s = OmenSet::new();
        s.push(Omen::homogenising_coronation());
        assert!(!s.consume_homogenising(PatchVersion::PATCH_0_4_0));
        assert!(s.consume_homogenising(PatchVersion::new(0, 3, 0)));
    }

    #[test]
    fn lord_target_consumption() {
        let mut s = OmenSet::new();
        s.push(Omen::blackblooded());
        s.push(Omen::liege());
        let lord = s.consume_lord_target(PatchVersion::PATCH_0_4_0);
        assert!(matches!(lord, Some(AbyssLord::Kurgal | AbyssLord::Amanamu)));
        assert_eq!(s.len(), 1);
    }
}
