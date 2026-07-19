//! Basic orbs: Transmute / Augment / Regal / Alchemy / Exalt / Annul /
//! Chaos / Divine.
//!
//! The shared sampling/removal kernel lives in [`super::common`]; the
//! Greater / Perfect tier variants live in [`super::variants`]; the Vaal
//! Orb and its corruption model live in [`super::vaal`]. The variant and
//! Vaal types are re-exported below so callers keep addressing them via
//! `currency::basic`.

use rand::Rng;

use crate::currency::common::{
    affix_label, collect_removable_filtered, pick_lowest_mod_level, pick_open_affix,
    pick_open_affix_with_omen, push_mod, remove_mod_at, roll_mod, sample_eligible_mod,
    BASIC_ORB_EXCLUDES,
};
use crate::currency::{ApplyContext, ApplyOutcome, Currency};
use crate::error::{EngineError, EngineResult};
use crate::ids::CurrencyId;
use crate::item::{Item, Rarity};

pub use crate::currency::vaal::{VaalOrb, VaalOutcome};
pub use crate::currency::variants::{
    GreaterChaosOrb, GreaterExaltedOrb, GreaterOrbOfAugmentation, GreaterOrbOfTransmutation,
    GreaterRegalOrb, PerfectChaosOrb, PerfectExaltedOrb, PerfectOrbOfAugmentation,
    PerfectOrbOfTransmutation, PerfectRegalOrb,
};

// =========================================================================
// Orb of Transmutation
// =========================================================================

/// Orb of Transmutation: Normal → Magic with 1 random mod.
#[derive(Debug)]
pub struct OrbOfTransmutation {
    id: CurrencyId,
}

impl OrbOfTransmutation {
    pub fn new() -> Self {
        Self {
            id: CurrencyId::from("OrbOfTransmutation"),
        }
    }
}

impl Default for OrbOfTransmutation {
    fn default() -> Self {
        Self::new()
    }
}

impl Currency for OrbOfTransmutation {
    fn id(&self) -> &CurrencyId {
        &self.id
    }
    fn name(&self) -> &'static str {
        "Orb of Transmutation"
    }
    fn valid_rarities(&self) -> crate::currency::RaritySet {
        crate::currency::RaritySet::NORMAL
    }

    fn apply(&self, item: &mut Item, ctx: &mut ApplyContext<'_>) -> EngineResult<ApplyOutcome> {
        if !item.is_modifiable() {
            return Err(EngineError::InvalidApplication(
                "Orb of Transmutation requires a modifiable item".into(),
            ));
        }
        if item.rarity != Rarity::Normal {
            return Err(EngineError::InvalidApplication(
                "Orb of Transmutation requires a Normal-rarity item".into(),
            ));
        }
        let affix = pick_open_affix(item, ctx.rng, /* magic max = */ 1)
            .ok_or(EngineError::AffixSlotFull { affix_type: "any" })?;
        let m = sample_eligible_mod(
            ctx.registry,
            ctx.base_registry,
            item,
            affix,
            ctx.rng,
            ctx.patch,
            0,
            BASIC_ORB_EXCLUDES,
        )
        .ok_or_else(|| EngineError::NoEligibleMods {
            base: item.base.to_string(),
            ilvl: item.ilvl,
            affix_type: affix_label(affix),
        })?;
        let roll = roll_mod(m, ctx.rng);
        item.rarity = Rarity::Magic;
        push_mod(item, roll);
        Ok(())
    }
}

// =========================================================================
// Orb of Augmentation
// =========================================================================

/// Orb of Augmentation: Magic with 1 mod → Magic with 2 mods (fills empty slot).
#[derive(Debug)]
pub struct OrbOfAugmentation {
    id: CurrencyId,
}

impl OrbOfAugmentation {
    pub fn new() -> Self {
        Self {
            id: CurrencyId::from("OrbOfAugmentation"),
        }
    }
}

impl Default for OrbOfAugmentation {
    fn default() -> Self {
        Self::new()
    }
}

impl Currency for OrbOfAugmentation {
    fn id(&self) -> &CurrencyId {
        &self.id
    }
    fn name(&self) -> &'static str {
        "Orb of Augmentation"
    }
    fn valid_rarities(&self) -> crate::currency::RaritySet {
        crate::currency::RaritySet::MAGIC
    }

    fn apply(&self, item: &mut Item, ctx: &mut ApplyContext<'_>) -> EngineResult<ApplyOutcome> {
        if !item.is_modifiable() {
            return Err(EngineError::InvalidApplication(
                "Orb of Augmentation requires a modifiable item".into(),
            ));
        }
        if item.rarity != Rarity::Magic {
            return Err(EngineError::InvalidApplication(
                "Orb of Augmentation requires a Magic-rarity item".into(),
            ));
        }
        let affix = pick_open_affix(item, ctx.rng, 1).ok_or(EngineError::AffixSlotFull {
            affix_type: "magic-item is full",
        })?;
        let m = sample_eligible_mod(
            ctx.registry,
            ctx.base_registry,
            item,
            affix,
            ctx.rng,
            ctx.patch,
            0,
            BASIC_ORB_EXCLUDES,
        )
        .ok_or_else(|| EngineError::NoEligibleMods {
            base: item.base.to_string(),
            ilvl: item.ilvl,
            affix_type: affix_label(affix),
        })?;
        push_mod(item, roll_mod(m, ctx.rng));
        Ok(())
    }
}

// =========================================================================
// Regal Orb
// =========================================================================

/// Regal Orb: Magic → Rare, adds 1 random mod (existing mods preserved).
#[derive(Debug)]
pub struct RegalOrb {
    id: CurrencyId,
}

impl RegalOrb {
    pub fn new() -> Self {
        Self {
            id: CurrencyId::from("RegalOrb"),
        }
    }
}

impl Default for RegalOrb {
    fn default() -> Self {
        Self::new()
    }
}

impl Currency for RegalOrb {
    fn id(&self) -> &CurrencyId {
        &self.id
    }
    fn name(&self) -> &'static str {
        "Regal Orb"
    }
    fn valid_rarities(&self) -> crate::currency::RaritySet {
        crate::currency::RaritySet::MAGIC
    }

    fn apply(&self, item: &mut Item, ctx: &mut ApplyContext<'_>) -> EngineResult<ApplyOutcome> {
        if !item.is_modifiable() {
            return Err(EngineError::InvalidApplication(
                "Regal Orb requires a modifiable item".into(),
            ));
        }
        if item.rarity != Rarity::Magic {
            return Err(EngineError::InvalidApplication(
                "Regal Orb requires a Magic-rarity item".into(),
            ));
        }
        // Rare items have up to 3 prefixes / 3 suffixes by default.
        let affix = pick_open_affix(item, ctx.rng, 3).ok_or(EngineError::AffixSlotFull {
            affix_type: "rare-item already full somehow",
        })?;
        let m = sample_eligible_mod(
            ctx.registry,
            ctx.base_registry,
            item,
            affix,
            ctx.rng,
            ctx.patch,
            0,
            BASIC_ORB_EXCLUDES,
        )
        .ok_or_else(|| EngineError::NoEligibleMods {
            base: item.base.to_string(),
            ilvl: item.ilvl,
            affix_type: affix_label(affix),
        })?;
        item.rarity = Rarity::Rare;
        push_mod(item, roll_mod(m, ctx.rng));
        Ok(())
    }
}

// =========================================================================
// Orb of Alchemy
// =========================================================================

/// Orb of Alchemy: Normal → Rare with **4** random mods.
///
/// PoE2 specifies 4 mods (not the PoE1 "4-6 random"); we add exactly 4.
/// If the eligible pool is exhausted before we hit 4 (extremely small
/// item-class), we add as many as possible and stop without erroring —
/// the resulting Rare item is legal even with fewer mods.
#[derive(Debug)]
pub struct OrbOfAlchemy {
    id: CurrencyId,
}

impl OrbOfAlchemy {
    pub fn new() -> Self {
        Self {
            id: CurrencyId::from("OrbOfAlchemy"),
        }
    }
}

impl Default for OrbOfAlchemy {
    fn default() -> Self {
        Self::new()
    }
}

impl Currency for OrbOfAlchemy {
    fn id(&self) -> &CurrencyId {
        &self.id
    }
    fn name(&self) -> &'static str {
        "Orb of Alchemy"
    }
    fn valid_rarities(&self) -> crate::currency::RaritySet {
        crate::currency::RaritySet::NORMAL
    }

    fn apply(&self, item: &mut Item, ctx: &mut ApplyContext<'_>) -> EngineResult<ApplyOutcome> {
        if !item.is_modifiable() {
            return Err(EngineError::InvalidApplication(
                "Orb of Alchemy requires a modifiable item".into(),
            ));
        }
        if item.rarity != Rarity::Normal {
            return Err(EngineError::InvalidApplication(
                "Orb of Alchemy requires a Normal-rarity item".into(),
            ));
        }
        item.rarity = Rarity::Rare;
        for _ in 0..4 {
            let Some(affix) = pick_open_affix(item, ctx.rng, /* rare max = */ 3) else {
                break;
            };
            let Some(m) = sample_eligible_mod(
                ctx.registry,
                ctx.base_registry,
                item,
                affix,
                ctx.rng,
                ctx.patch,
                0,
                BASIC_ORB_EXCLUDES,
            ) else {
                break;
            };
            push_mod(item, roll_mod(m, ctx.rng));
        }
        Ok(())
    }
}

// =========================================================================
// Exalted Orb
// =========================================================================

/// Exalted Orb: Rare with ≥1 empty affix slot → add 1 random mod.
#[derive(Debug)]
pub struct ExaltedOrb {
    id: CurrencyId,
}

impl ExaltedOrb {
    pub fn new() -> Self {
        Self {
            id: CurrencyId::from("ExaltedOrb"),
        }
    }
}

impl Default for ExaltedOrb {
    fn default() -> Self {
        Self::new()
    }
}

impl Currency for ExaltedOrb {
    fn id(&self) -> &CurrencyId {
        &self.id
    }
    fn name(&self) -> &'static str {
        "Exalted Orb"
    }
    fn valid_rarities(&self) -> crate::currency::RaritySet {
        crate::currency::RaritySet::RARE
    }

    fn apply(&self, item: &mut Item, ctx: &mut ApplyContext<'_>) -> EngineResult<ApplyOutcome> {
        if !item.is_modifiable() {
            return Err(EngineError::InvalidApplication(
                "Exalted Orb requires a modifiable item".into(),
            ));
        }
        if item.rarity != Rarity::Rare {
            return Err(EngineError::InvalidApplication(
                "Exalted Orb requires a Rare-rarity item".into(),
            ));
        }

        // Greater Exaltation: add 2 mods if active.
        let n_mods = if ctx.omens.consume_greater_exaltation(ctx.patch) {
            2
        } else {
            1
        };

        for _ in 0..n_mods {
            let affix =
                pick_open_affix_with_omen(item, ctx, 3).ok_or(EngineError::AffixSlotFull {
                    affix_type: "Exalted Orb: no eligible affix slot",
                })?;
            let m = sample_eligible_mod(
                ctx.registry,
                ctx.base_registry,
                item,
                affix,
                ctx.rng,
                ctx.patch,
                0,
                BASIC_ORB_EXCLUDES,
            )
            .ok_or_else(|| EngineError::NoEligibleMods {
                base: item.base.to_string(),
                ilvl: item.ilvl,
                affix_type: affix_label(affix),
            })?;
            push_mod(item, roll_mod(m, ctx.rng));
        }
        Ok(())
    }
}

// =========================================================================
// Orb of Annulment
// =========================================================================

/// Orb of Annulment: removes 1 random non-fractured affix mod.
///
/// Works on Magic OR Rare. Does NOT change rarity; an annulled Magic that
/// drops to 0 mods stays Magic. Refuses on items with no removable mods.
#[derive(Debug)]
pub struct OrbOfAnnulment {
    id: CurrencyId,
}

impl OrbOfAnnulment {
    pub fn new() -> Self {
        Self {
            id: CurrencyId::from("OrbOfAnnulment"),
        }
    }
}

impl Default for OrbOfAnnulment {
    fn default() -> Self {
        Self::new()
    }
}

impl Currency for OrbOfAnnulment {
    fn id(&self) -> &CurrencyId {
        &self.id
    }
    fn name(&self) -> &'static str {
        "Orb of Annulment"
    }
    fn valid_rarities(&self) -> crate::currency::RaritySet {
        crate::currency::RaritySet::MAGIC.union(crate::currency::RaritySet::RARE)
    }

    fn apply(&self, item: &mut Item, ctx: &mut ApplyContext<'_>) -> EngineResult<ApplyOutcome> {
        if !item.is_modifiable() {
            return Err(EngineError::InvalidApplication(
                "Orb of Annulment requires a modifiable item".into(),
            ));
        }
        if !matches!(item.rarity, Rarity::Magic | Rarity::Rare) {
            return Err(EngineError::InvalidApplication(
                "Orb of Annulment requires a Magic or Rare item".into(),
            ));
        }
        // Sinistral/Dextral Annulment force a side; Omen of Light forces
        // Desecrated-only. The two filters compose — both can apply.
        let affix_filter = ctx.omens.consume_affix_only(ctx.patch);
        let desecrated_only = ctx.omens.consume_light(ctx.patch);
        let removables = collect_removable_filtered(item, affix_filter, desecrated_only);
        if removables.is_empty() {
            return Err(EngineError::InvalidApplication(
                "Orb of Annulment: no eligible mod to remove given omens / fractures".into(),
            ));
        }
        let pick = ctx.rng.gen_range(0..removables.len());
        let (affix, idx) = removables[pick];
        remove_mod_at(item, affix, idx);
        Ok(())
    }
}

// =========================================================================
// Chaos Orb
// =========================================================================

/// Chaos Orb (PoE2): remove **1** random non-fractured affix mod, then add
/// **1** random eligible mod. Net mod count stays the same. Operates on Rare.
///
/// PoE2 Chaos ≠ PoE1 Chaos (the latter rerolls all mods). Common confusion
/// point — see ADR-0006 / docs/12-poe2-vs-poe1.md once that lands.
#[derive(Debug)]
pub struct ChaosOrb {
    id: CurrencyId,
}

impl ChaosOrb {
    pub fn new() -> Self {
        Self {
            id: CurrencyId::from("ChaosOrb"),
        }
    }
}

impl Default for ChaosOrb {
    fn default() -> Self {
        Self::new()
    }
}

impl Currency for ChaosOrb {
    fn id(&self) -> &CurrencyId {
        &self.id
    }
    fn name(&self) -> &'static str {
        "Chaos Orb"
    }
    fn valid_rarities(&self) -> crate::currency::RaritySet {
        crate::currency::RaritySet::RARE
    }

    fn apply(&self, item: &mut Item, ctx: &mut ApplyContext<'_>) -> EngineResult<ApplyOutcome> {
        chaos_apply(item, ctx, 0)
    }
}

/// Shared Chaos apply. Used by both vanilla [`ChaosOrb`] and the Greater /
/// Perfect variants in [`super::variants`] (which thread a min-mod-level
/// through).
pub(crate) fn chaos_apply(
    item: &mut Item,
    ctx: &mut ApplyContext<'_>,
    min_level: u32,
) -> EngineResult<ApplyOutcome> {
    if !item.is_modifiable() {
        return Err(EngineError::InvalidApplication(
            "Chaos Orb requires a modifiable item".into(),
        ));
    }
    if item.rarity != Rarity::Rare {
        return Err(EngineError::InvalidApplication(
            "Chaos Orb requires a Rare-rarity item".into(),
        ));
    }

    // Removal step. Omens that may fire on this Chaos:
    //   Sinistral/Dextral Erasure  → AffixOnly filter
    //   Whittling                  → pick lowest-required-level mod
    let affix_filter = ctx.omens.consume_affix_only(ctx.patch);
    let whittling = ctx.omens.consume_whittling(ctx.patch);
    let removables = collect_removable_filtered(item, affix_filter, false);
    if removables.is_empty() {
        return Err(EngineError::InvalidApplication(
            "Chaos Orb: no eligible mod to remove given omens / fractures".into(),
        ));
    }
    let pick_idx = if whittling {
        pick_lowest_mod_level(item, &removables, ctx.registry).unwrap_or(0)
    } else {
        ctx.rng.gen_range(0..removables.len())
    };
    let (removed_affix, idx) = removables[pick_idx];
    remove_mod_at(item, removed_affix, idx);

    // Add step. The new affix slot is uniform among empty slots (or forced
    // by Sinistral/Dextral Erasure if it had a sister Add omen, but Erasure
    // covers only the removal side per planning).
    let new_affix = pick_open_affix(item, ctx.rng, 3).ok_or(EngineError::AffixSlotFull {
        affix_type: "no slot opened up after Chaos Orb removal",
    })?;
    let m = sample_eligible_mod(
        ctx.registry,
        ctx.base_registry,
        item,
        new_affix,
        ctx.rng,
        ctx.patch,
        min_level,
        BASIC_ORB_EXCLUDES,
    )
    .ok_or_else(|| EngineError::NoEligibleMods {
        base: item.base.to_string(),
        ilvl: item.ilvl,
        affix_type: affix_label(new_affix),
    })?;
    push_mod(item, roll_mod(m, ctx.rng));
    Ok(())
}

// =========================================================================
// Divine Orb
// =========================================================================

/// Divine Orb: reroll the numeric values of all explicit mods within their
/// existing tier ranges.
///
/// - Works on Magic, Rare, or Unique items.
/// - Skips fractured mods (their values are locked).
/// - Does NOT touch implicits or enchantments by default; with the
///   Omen of the Blessed (M2.6), Divine instead targets *only* implicits.
/// - Ranges come from the underlying [`crate::mods::ModDefinition::stats`];
///   we look up each `ModRoll`'s `mod_id` in the registry and reroll
///   uniformly within `[min, max]` for every stat in the mod.
#[derive(Debug)]
pub struct DivineOrb {
    id: CurrencyId,
}

impl DivineOrb {
    pub fn new() -> Self {
        Self {
            id: CurrencyId::from("DivineOrb"),
        }
    }
}

impl Default for DivineOrb {
    fn default() -> Self {
        Self::new()
    }
}

impl Currency for DivineOrb {
    fn id(&self) -> &CurrencyId {
        &self.id
    }
    fn name(&self) -> &'static str {
        "Divine Orb"
    }
    fn valid_rarities(&self) -> crate::currency::RaritySet {
        crate::currency::RaritySet::MAGIC
            .union(crate::currency::RaritySet::RARE)
            .union(crate::currency::RaritySet::UNIQUE)
    }

    fn apply(&self, item: &mut Item, ctx: &mut ApplyContext<'_>) -> EngineResult<ApplyOutcome> {
        if !item.is_modifiable() {
            return Err(EngineError::InvalidApplication(
                "Divine Orb requires a modifiable item".into(),
            ));
        }
        if item.rarity == Rarity::Normal {
            return Err(EngineError::InvalidApplication(
                "Divine Orb has no effect on a Normal-rarity item".into(),
            ));
        }

        // Omen of the Blessed: Divine rerolls *only* the implicit modifier.
        if ctx.omens.consume_blessed(ctx.patch) {
            reroll_implicit_values(item, ctx);
            return Ok(());
        }

        // Omen of Sanctification: extended-range Divine, then the item is
        // sanctified (locked from further crafting).
        if ctx.omens.consume_sanctification(ctx.patch) {
            sanctify_values(item, ctx);
            return Ok(());
        }

        reroll_explicit_values(item, ctx);
        Ok(())
    }
}

/// Omen of the Blessed: reroll only the implicit modifier's values.
fn reroll_implicit_values(item: &mut Item, ctx: &mut ApplyContext<'_>) {
    for m in &mut item.implicits {
        if let Some(def) = ctx.registry.get(&m.mod_id) {
            m.values = def
                .stats
                .iter()
                .map(|s| s.roll(ctx.rng.gen::<f64>()))
                .collect();
        }
    }
}

/// Omen of Sanctification value pass, then lock the item.
///
/// - ≤0.4: rolls each stat anywhere in 80–120% of its normal range
///   (randomise beyond range).
/// - 0.5+: per patch notes, Sanctification now *multiplies each modifier
///   based on its current value* instead of randomising. Modelled as a
///   per-mod uniform multiplier in [0.8, 1.2] (Experimental — exact factor
///   range is not published).
fn sanctify_values(item: &mut Item, ctx: &mut ApplyContext<'_>) {
    if ctx.patch >= crate::patch::PatchVersion::PATCH_0_5_0 {
        multiply_explicit_values(item, ctx, 0.8, 1.2);
    } else {
        for m in item.prefixes.iter_mut().chain(item.suffixes.iter_mut()) {
            if m.is_fractured {
                continue;
            }
            if let Some(def) = ctx.registry.get(&m.mod_id) {
                let scale = 0.8 + ctx.rng.gen::<f64>() * 0.4;
                m.values = def
                    .stats
                    .iter()
                    .map(|s| s.roll(ctx.rng.gen::<f64>()) * scale)
                    .collect();
            }
        }
    }
    item.sanctified = true;
}

pub(crate) fn reroll_explicit_values(item: &mut Item, ctx: &mut ApplyContext<'_>) {
    for m in item.prefixes.iter_mut().chain(item.suffixes.iter_mut()) {
        if m.is_fractured {
            continue;
        }
        if let Some(def) = ctx.registry.get(&m.mod_id) {
            m.values = def
                .stats
                .iter()
                .map(|s| s.roll(ctx.rng.gen::<f64>()))
                .collect();
        }
    }
}

/// 0.5 value-shift model: multiply each non-fractured explicit mod's values
/// by a single per-mod uniform factor in `[lo, hi]`.
///
/// Used by the 0.5 variants of the Vaal "unpredictable values" outcome and
/// of Sanctification, which per the 0.5 patch notes "multiply each modifier
/// based on its current value" instead of randomising. The exact factor
/// range is not published; `Confidence::Experimental`.
pub(crate) fn multiply_explicit_values(
    item: &mut Item,
    ctx: &mut ApplyContext<'_>,
    lo: f64,
    hi: f64,
) {
    for m in item.prefixes.iter_mut().chain(item.suffixes.iter_mut()) {
        if m.is_fractured {
            continue;
        }
        let factor = lo + ctx.rng.gen::<f64>() * (hi - lo);
        for v in &mut m.values {
            *v *= factor;
        }
    }
}

// =========================================================================
// Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use rand::SeedableRng;
    use rand_xoshiro::Xoshiro256PlusPlus;
    use smallvec::smallvec;

    use super::*;
    use crate::currency::common::test_fixtures::{
        ctx, fixture_normal_boots, fixture_registry, mk_mod_lvl,
    };
    use crate::ids::{ItemClassId, ModGroupId, ModId, TagId};
    use crate::item::{AffixType, ModRoll};
    use crate::mods::{ModDefinition, ModDomain, ModFlags, ModKind, SpawnWeight};
    use crate::patch::PatchRange;
    use crate::registry::ModRegistry;

    #[test]
    fn transmute_promotes_normal_to_magic_and_adds_one_mod() {
        let reg = fixture_registry();
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(42);
        let mut omens = crate::omen::OmenSet::new();
        let mut item = fixture_normal_boots();
        OrbOfTransmutation::new()
            .apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens))
            .unwrap();
        assert_eq!(item.rarity, Rarity::Magic);
        assert_eq!(item.prefixes.len() + item.suffixes.len(), 1);
    }

    #[test]
    fn transmute_rejects_magic_item() {
        let reg = fixture_registry();
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(1);
        let mut omens = crate::omen::OmenSet::new();
        let mut item = fixture_normal_boots();
        item.rarity = Rarity::Magic;
        let r = OrbOfTransmutation::new().apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens));
        assert!(matches!(r, Err(EngineError::InvalidApplication(_))));
    }

    #[test]
    fn augment_fills_empty_slot_on_magic() {
        let reg = fixture_registry();
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(7);
        let mut omens = crate::omen::OmenSet::new();
        let mut item = fixture_normal_boots();
        OrbOfTransmutation::new()
            .apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens))
            .unwrap();
        OrbOfAugmentation::new()
            .apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens))
            .unwrap();
        assert_eq!(item.rarity, Rarity::Magic);
        assert_eq!(item.prefixes.len() + item.suffixes.len(), 2);
        // Mod-group exclusivity: the two mods must be from different groups.
        let groups: std::collections::HashSet<_> = item
            .prefixes
            .iter()
            .chain(item.suffixes.iter())
            .map(|m| reg.group_of(&m.mod_id).unwrap().clone())
            .collect();
        assert_eq!(groups.len(), 2);
    }

    #[test]
    fn augment_rejects_when_both_slots_full() {
        // Magic = 1 prefix + 1 suffix max; saturated => Augment errors.
        let reg = fixture_registry();
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(7);
        let mut omens = crate::omen::OmenSet::new();
        let mut item = fixture_normal_boots();
        OrbOfTransmutation::new()
            .apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens))
            .unwrap();
        OrbOfAugmentation::new()
            .apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens))
            .unwrap();
        let r = OrbOfAugmentation::new().apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens));
        assert!(matches!(r, Err(EngineError::AffixSlotFull { .. })));
    }

    #[test]
    fn regal_promotes_magic_to_rare_with_3rd_mod() {
        let reg = fixture_registry();
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(13);
        let mut omens = crate::omen::OmenSet::new();
        let mut item = fixture_normal_boots();
        OrbOfTransmutation::new()
            .apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens))
            .unwrap();
        OrbOfAugmentation::new()
            .apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens))
            .unwrap();
        let before = item.prefixes.len() + item.suffixes.len();
        RegalOrb::new()
            .apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens))
            .unwrap();
        let after = item.prefixes.len() + item.suffixes.len();
        assert_eq!(item.rarity, Rarity::Rare);
        assert_eq!(after, before + 1);
    }

    #[test]
    fn regal_rejects_normal_or_rare() {
        let reg = fixture_registry();
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(99);
        let mut omens = crate::omen::OmenSet::new();
        let mut item = fixture_normal_boots();

        // Normal
        let r = RegalOrb::new().apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens));
        assert!(matches!(r, Err(EngineError::InvalidApplication(_))));

        // Rare
        item.rarity = Rarity::Rare;
        let r = RegalOrb::new().apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens));
        assert!(matches!(r, Err(EngineError::InvalidApplication(_))));
    }

    #[test]
    fn currencies_reject_corrupted_items() {
        let reg = fixture_registry();
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0);
        let mut omens = crate::omen::OmenSet::new();
        let mut item = fixture_normal_boots();
        item.corrupted = true;
        let r = OrbOfTransmutation::new().apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens));
        assert!(matches!(r, Err(EngineError::InvalidApplication(_))));
    }

    #[test]
    fn currencies_are_deterministic_given_same_seed() {
        let reg = fixture_registry();
        let make = || {
            let mut rng = Xoshiro256PlusPlus::seed_from_u64(0x00c0_ffee);
            let mut omens = crate::omen::OmenSet::new();
            let mut item = fixture_normal_boots();
            OrbOfTransmutation::new()
                .apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens))
                .unwrap();
            OrbOfAugmentation::new()
                .apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens))
                .unwrap();
            RegalOrb::new()
                .apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens))
                .unwrap();
            item
        };
        assert_eq!(make(), make());
    }

    // ---- Alchemy / Exalt / Chaos / Annul -----------------------------------

    #[test]
    fn alchemy_promotes_normal_to_rare_with_up_to_4_mods() {
        let reg = fixture_registry();
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0xa1c);
        let mut omens = crate::omen::OmenSet::new();
        let mut item = fixture_normal_boots();
        OrbOfAlchemy::new()
            .apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens))
            .unwrap();
        assert_eq!(item.rarity, Rarity::Rare);
        let n = item.prefixes.len() + item.suffixes.len();
        assert!((1..=4).contains(&n), "got {n} mods");
        // No mod-group conflicts.
        let groups: std::collections::HashSet<_> = item
            .prefixes
            .iter()
            .chain(item.suffixes.iter())
            .map(|m| reg.group_of(&m.mod_id).unwrap().clone())
            .collect();
        assert_eq!(groups.len(), n);
    }

    #[test]
    fn alchemy_rejects_non_normal() {
        let reg = fixture_registry();
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(1);
        let mut omens = crate::omen::OmenSet::new();
        let mut item = fixture_normal_boots();
        item.rarity = Rarity::Magic;
        let r = OrbOfAlchemy::new().apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens));
        assert!(matches!(r, Err(EngineError::InvalidApplication(_))));
    }

    #[test]
    fn exalt_adds_one_mod_to_rare() {
        let reg = fixture_registry();
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0xe7);
        let mut omens = crate::omen::OmenSet::new();
        let mut item = fixture_normal_boots();
        OrbOfAlchemy::new()
            .apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens))
            .unwrap();
        let before = item.prefixes.len() + item.suffixes.len();
        if before >= 6 {
            // Pool too small to add another; skip rather than fail.
            return;
        }
        ExaltedOrb::new()
            .apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens))
            .unwrap();
        let after = item.prefixes.len() + item.suffixes.len();
        assert_eq!(after, before + 1);
    }

    #[test]
    fn exalt_rejects_non_rare() {
        let reg = fixture_registry();
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(2);
        let mut omens = crate::omen::OmenSet::new();
        let mut item = fixture_normal_boots();
        let r = ExaltedOrb::new().apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens));
        assert!(matches!(r, Err(EngineError::InvalidApplication(_))));
    }

    #[test]
    fn annul_removes_exactly_one_mod() {
        let reg = fixture_registry();
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0xa9);
        let mut omens = crate::omen::OmenSet::new();
        let mut item = fixture_normal_boots();
        OrbOfAlchemy::new()
            .apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens))
            .unwrap();
        let before = item.prefixes.len() + item.suffixes.len();
        OrbOfAnnulment::new()
            .apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens))
            .unwrap();
        let after = item.prefixes.len() + item.suffixes.len();
        assert_eq!(after, before - 1);
    }

    #[test]
    fn annul_skips_fractured_mods() {
        // Build a Rare with 1 fractured + 1 non-fractured. Repeated annul
        // should never remove the fractured one.
        let reg = fixture_registry();
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0xfff);
        let mut omens = crate::omen::OmenSet::new();
        let mut item = fixture_normal_boots();
        item.rarity = Rarity::Rare;
        item.prefixes.push(ModRoll {
            mod_id: ModId::from("Life1"),
            affix_type: AffixType::Prefix,
            kind: ModKind::Explicit,
            values: smallvec![],
            is_fractured: true,
        });
        item.suffixes.push(ModRoll {
            mod_id: ModId::from("FireRes1"),
            affix_type: AffixType::Suffix,
            kind: ModKind::Explicit,
            values: smallvec![],
            is_fractured: false,
        });
        OrbOfAnnulment::new()
            .apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens))
            .unwrap();
        // Fractured prefix survives; suffix is gone.
        assert_eq!(item.prefixes.len(), 1);
        assert_eq!(item.suffixes.len(), 0);
        assert!(item.prefixes[0].is_fractured);

        // Second annul: nothing left to remove (only fractured).
        let r = OrbOfAnnulment::new().apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens));
        assert!(matches!(r, Err(EngineError::InvalidApplication(_))));
    }

    #[test]
    fn chaos_keeps_mod_count_constant() {
        let reg = fixture_registry();
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0xcaa05);
        let mut omens = crate::omen::OmenSet::new();
        let mut item = fixture_normal_boots();
        OrbOfAlchemy::new()
            .apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens))
            .unwrap();
        let before = item.prefixes.len() + item.suffixes.len();
        if before == 0 {
            return; // alch produced 0 mods; can't chaos
        }
        ChaosOrb::new()
            .apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens))
            .unwrap();
        let after = item.prefixes.len() + item.suffixes.len();
        // Chaos = -1 + 1 = same count
        assert_eq!(after, before);
    }

    // ---- Divine ------------------------------------------------------------

    #[test]
    fn divine_rerolls_non_fractured_values() {
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0xd1);
        let mut omens = crate::omen::OmenSet::new();
        // Build a Rare with one mod whose stats have a wide range.
        let mut item = fixture_normal_boots();
        item.rarity = Rarity::Rare;
        item.prefixes.push(ModRoll {
            mod_id: ModId::from("Life1"),
            affix_type: AffixType::Prefix,
            kind: ModKind::Explicit,
            values: smallvec![0.0],
            is_fractured: false,
        });

        // Stub the registry mod with a non-trivial range so reroll is observable.
        let reg = ModRegistry::from_mods(
            vec![ModDefinition {
                id: ModId::from("Life1"),
                name: None,
                mod_group: crate::mods::ModGroup(ModGroupId::from("Life")),
                affix_type: AffixType::Prefix,
                kind: ModKind::Explicit,
                domain: ModDomain::Item,
                tags: smallvec![],
                concept_set: smallvec![],
                spawn_weights: smallvec![SpawnWeight {
                    tag: TagId::from("Boots"),
                    weight: 1
                }],
                stats: smallvec![crate::mods::ModStat {
                    stat_id: "base_maximum_life".into(),
                    min: 100.0,
                    max: 200.0
                }],
                required_level: 1,
                tier: None,
                allowed_item_classes: smallvec![ItemClassId::from("Boots")],
                patch_range: PatchRange::ALL,
                flags: ModFlags::empty(),
                text_template: None,
            }],
            vec![],
        );

        DivineOrb::new()
            .apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens))
            .unwrap();
        let v = item.prefixes[0].values[0];
        assert!((100.0..=200.0).contains(&v), "got {v}");
    }

    #[test]
    fn divine_skips_fractured_mods() {
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0xd2);
        let mut omens = crate::omen::OmenSet::new();
        let reg = ModRegistry::from_mods(
            vec![ModDefinition {
                id: ModId::from("Life1"),
                name: None,
                mod_group: crate::mods::ModGroup(ModGroupId::from("Life")),
                affix_type: AffixType::Prefix,
                kind: ModKind::Explicit,
                domain: ModDomain::Item,
                tags: smallvec![],
                concept_set: smallvec![],
                spawn_weights: smallvec![SpawnWeight {
                    tag: TagId::from("Boots"),
                    weight: 1
                }],
                stats: smallvec![crate::mods::ModStat {
                    stat_id: "base_maximum_life".into(),
                    min: 100.0,
                    max: 200.0
                }],
                required_level: 1,
                tier: None,
                allowed_item_classes: smallvec![ItemClassId::from("Boots")],
                patch_range: PatchRange::ALL,
                flags: ModFlags::empty(),
                text_template: None,
            }],
            vec![],
        );

        let mut item = fixture_normal_boots();
        item.rarity = Rarity::Rare;
        item.prefixes.push(ModRoll {
            mod_id: ModId::from("Life1"),
            affix_type: AffixType::Prefix,
            kind: ModKind::Explicit,
            values: smallvec![123.0],
            is_fractured: true,
        });
        DivineOrb::new()
            .apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens))
            .unwrap();
        // Fractured value is unchanged.
        assert!((item.prefixes[0].values[0] - 123.0).abs() < 1e-9);
    }

    #[test]
    fn divine_rejects_normal_or_corrupted() {
        let reg = fixture_registry();
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0xd3);
        let mut omens = crate::omen::OmenSet::new();
        let mut item = fixture_normal_boots();
        let r = DivineOrb::new().apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens));
        assert!(matches!(r, Err(EngineError::InvalidApplication(_))));

        item.rarity = Rarity::Rare;
        item.corrupted = true;
        let r = DivineOrb::new().apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens));
        assert!(matches!(r, Err(EngineError::InvalidApplication(_))));
    }

    // ---- Omen interactions -------------------------------------------------

    fn fill_rare_with_groups(item: &mut Item, prefix_groups: &[&str], suffix_groups: &[&str]) {
        item.rarity = Rarity::Rare;
        for g in prefix_groups {
            item.prefixes.push(ModRoll {
                mod_id: ModId::from(*g),
                affix_type: AffixType::Prefix,
                kind: ModKind::Explicit,
                values: smallvec![],
                is_fractured: false,
            });
        }
        for g in suffix_groups {
            item.suffixes.push(ModRoll {
                mod_id: ModId::from(*g),
                affix_type: AffixType::Suffix,
                kind: ModKind::Explicit,
                values: smallvec![],
                is_fractured: false,
            });
        }
    }

    #[test]
    fn dextral_exaltation_forces_suffix() {
        let reg = fixture_registry();
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0xde0);
        let mut omens = crate::omen::OmenSet::new();
        omens.push(crate::omen::Omen::dextral_exaltation());

        let mut item = fixture_normal_boots();
        // Rare with one suffix open and one prefix open.
        fill_rare_with_groups(&mut item, &["Life1"], &[]);

        ExaltedOrb::new()
            .apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens))
            .unwrap();

        // The omen forced Suffix; we should have at least one suffix.
        assert!(!item.suffixes.is_empty());
        assert!(omens.is_empty(), "omen should be consumed");
    }

    #[test]
    fn sinistral_exaltation_forces_prefix() {
        let reg = fixture_registry();
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0x517);
        let mut omens = crate::omen::OmenSet::new();
        omens.push(crate::omen::Omen::sinistral_exaltation());

        let mut item = fixture_normal_boots();
        fill_rare_with_groups(&mut item, &[], &["FireRes1"]);

        ExaltedOrb::new()
            .apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens))
            .unwrap();

        assert!(!item.prefixes.is_empty(), "prefix should have been added");
    }

    #[test]
    fn greater_exaltation_adds_two_mods() {
        let reg = fixture_registry();
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0xa2);
        let mut omens = crate::omen::OmenSet::new();
        omens.push(crate::omen::Omen::greater_exaltation());

        let mut item = fixture_normal_boots();
        // Empty Rare so we have plenty of slots.
        item.rarity = Rarity::Rare;

        ExaltedOrb::new()
            .apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens))
            .unwrap();

        assert_eq!(
            item.prefixes.len() + item.suffixes.len(),
            2,
            "Greater Exaltation should add 2 mods"
        );
    }

    #[test]
    fn dextral_annulment_only_removes_suffix() {
        let reg = fixture_registry();
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0xab);
        let mut omens = crate::omen::OmenSet::new();
        omens.push(crate::omen::Omen::dextral_annulment());

        let mut item = fixture_normal_boots();
        fill_rare_with_groups(&mut item, &["Life1"], &["FireRes1"]);

        OrbOfAnnulment::new()
            .apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens))
            .unwrap();

        assert_eq!(item.prefixes.len(), 1, "prefix must survive");
        assert_eq!(item.suffixes.len(), 0, "suffix must be removed");
    }

    #[test]
    fn whittling_picks_lowest_required_level_mod_in_chaos() {
        // Two prefixes: high-req and low-req. Chaos with Whittling removes
        // the lowest-required-level one.
        let reg = ModRegistry::from_mods(
            vec![
                mk_mod_lvl("HighReqLife", "Life", AffixType::Prefix, "Boots", 80),
                mk_mod_lvl("LowReqMana", "Mana", AffixType::Prefix, "Boots", 5),
                mk_mod_lvl("FreshMod", "FreshGroup", AffixType::Suffix, "Boots", 1),
            ],
            vec![],
        );
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0xc1);
        let mut omens = crate::omen::OmenSet::new();
        omens.push(crate::omen::Omen::whittling());

        let mut item = fixture_normal_boots();
        item.rarity = Rarity::Rare;
        item.prefixes.push(ModRoll {
            mod_id: ModId::from("HighReqLife"),
            affix_type: AffixType::Prefix,
            kind: ModKind::Explicit,
            values: smallvec![],
            is_fractured: false,
        });
        item.prefixes.push(ModRoll {
            mod_id: ModId::from("LowReqMana"),
            affix_type: AffixType::Prefix,
            kind: ModKind::Explicit,
            values: smallvec![],
            is_fractured: false,
        });

        ChaosOrb::new()
            .apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens))
            .unwrap();

        // The low-req mod must be gone; the high-req must survive.
        assert!(!item
            .prefixes
            .iter()
            .any(|m| m.mod_id == ModId::from("LowReqMana")));
        assert!(item
            .prefixes
            .iter()
            .any(|m| m.mod_id == ModId::from("HighReqLife")));
    }

    // ---- Original tests ----------------------------------------------------

    #[test]
    fn chaos_rejects_non_rare() {
        let reg = fixture_registry();
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(3);
        let mut omens = crate::omen::OmenSet::new();
        let mut item = fixture_normal_boots();
        item.rarity = Rarity::Magic;
        item.prefixes.push(ModRoll {
            mod_id: ModId::from("Life1"),
            affix_type: AffixType::Prefix,
            kind: ModKind::Explicit,
            values: smallvec![],
            is_fractured: false,
        });
        let r = ChaosOrb::new().apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens));
        assert!(matches!(r, Err(EngineError::InvalidApplication(_))));
    }
}
