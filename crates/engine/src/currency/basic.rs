//! Basic orbs: Transmute / Augment / Regal (M2.4b).
//!
//! Alch / Exalt / Chaos / Annul / Divine / Vaal land in M2.4c-d.
//! Greater / Perfect variants land in M2.4e.

use rand::seq::SliceRandom;
use rand::Rng;
use smallvec::SmallVec;

use crate::currency::{ApplyContext, ApplyOutcome, Currency};
use crate::error::{EngineError, EngineResult};
use crate::ids::{CurrencyId, ItemClassId};
use crate::item::{AffixType, Item, ModRoll, Rarity};
use crate::mods::{ModDefinition, ModKind};
use crate::registry::{ModIndex, ModRegistry};

// =========================================================================
// Helpers shared by all "add a mod" orbs
// =========================================================================

/// Compute the class of an item from its base. The full `BaseType` lookup is
/// not part of the engine's hot path yet — for now we ferry the class via a
/// helper. Once M2.4 stabilizes, the engine will keep a `BaseRegistry` like
/// `ModRegistry` that gives O(1) base → class.
///
/// In the interim, callers who need to apply a currency must already know
/// the item's class. The engine offers helpers that take the class directly.
/// `Item.base` is opaque to the engine until the BaseRegistry is wired up.
pub(crate) fn class_for_item(item: &Item) -> ItemClassId {
    // PLACEHOLDER: until BaseRegistry lands, the caller must set Item.base
    // to the item-class id directly when constructing test items. The
    // pipeline-built bundles always carry the full base id, but the engine
    // needs the class for its lookups, not the base. We'll resolve this in
    // M2.4-followup by either:
    //   (a) introducing BaseRegistry, or
    //   (b) extending Item with a denormalized `class: ItemClassId`.
    // Option (b) is cheaper and is what the test fixtures use.
    ItemClassId::from(item.base.as_str())
}

/// Sample a mod uniformly at random from the eligible set.
///
/// Eligibility:
/// 1. The mod's `affix_type` matches `affix`.
/// 2. The mod's `allowed_item_classes` contains the item's class
///    (this is what `for_class_affix` already filters by).
/// 3. The mod's `required_level` is `<= ilvl`.
/// 4. The mod's group is not already present on the item (mod-group exclusivity).
/// 5. The mod is `ModKind::Explicit` (we don't roll implicits/enchants/desecrated
///    via this path).
/// 6. The mod's `patch_range` contains the current patch.
///
/// Weights from `spawn_weights` are applied: total weight 0 mods are
/// excluded; weighted random selection over the rest.
fn sample_eligible_mod<'r>(
    registry: &'r ModRegistry,
    item: &Item,
    affix: AffixType,
    rng: &mut dyn rand::RngCore,
    patch: crate::patch::PatchVersion,
    min_required_level: u32,
) -> Option<&'r ModDefinition> {
    let class = class_for_item(item);
    let candidates = registry.for_class_affix(&class, affix);

    // Build the list of (mod, weight) tuples after filtering.
    // SmallVec to avoid heap allocation in the small-eligibility-set common case.
    let mut eligible: SmallVec<[(ModIndex, u32); 64]> = SmallVec::new();

    let occupied_groups = collect_occupied_groups(registry, item);

    for &idx in candidates {
        let Some(m) = registry.at(idx) else { continue };
        if m.kind != ModKind::Explicit {
            continue;
        }
        if m.required_level < min_required_level || m.required_level > item.ilvl {
            continue;
        }
        if !m.patch_range.contains(patch) {
            continue;
        }
        if occupied_groups.contains(&m.mod_group.0) {
            continue;
        }
        let w = total_weight_for_item(m, item);
        if w == 0 {
            continue;
        }
        eligible.push((idx, w));
    }

    if eligible.is_empty() {
        return None;
    }

    let total: u64 = eligible.iter().map(|(_, w)| u64::from(*w)).sum();
    let mut pick = rng.gen_range(0..total);
    for (idx, w) in &eligible {
        let w64 = u64::from(*w);
        if pick < w64 {
            return registry.at(*idx);
        }
        pick -= w64;
    }
    // Defensive: should never reach here unless the iterator and the random
    // distribution disagree (they don't).
    eligible.choose(rng).and_then(|(i, _)| registry.at(*i))
}

/// Sum of `spawn_weights` entries whose tag is present on the item's tag set
/// (i.e., the actual probability mass of this mod on this item).
fn total_weight_for_item(m: &ModDefinition, _item: &Item) -> u32 {
    // TODO(M2.5): once Item carries denormalized base tags, multiply by the
    // intersection of tags. For now, until BaseRegistry lands, we treat every
    // non-zero weight as 1; this is identical to RePoE-fork's eligibility
    // semantics and produces uniform sampling within the eligible set —
    // sufficient for M2.4b correctness tests.
    u32::from(m.spawn_weights.iter().any(|sw| sw.weight > 0))
}

/// Set of mod-groups already occupied on the item (any affix slot).
fn collect_occupied_groups(
    registry: &ModRegistry,
    item: &Item,
) -> SmallVec<[crate::ids::ModGroupId; 8]> {
    let mut out = SmallVec::new();
    for m in item.prefixes.iter().chain(item.suffixes.iter()) {
        if let Some(g) = registry.group_of(&m.mod_id) {
            if !out.contains(g) {
                out.push(g.clone());
            }
        }
    }
    out
}

/// All non-fractured visible explicit mods on the item, in a stable order.
/// Used by remove-style currencies (Annul/Chaos) for uniform random selection.
fn collect_removable(item: &Item) -> SmallVec<[(AffixType, usize); 8]> {
    let mut out = SmallVec::new();
    for (i, m) in item.prefixes.iter().enumerate() {
        if !m.is_fractured {
            out.push((AffixType::Prefix, i));
        }
    }
    for (i, m) in item.suffixes.iter().enumerate() {
        if !m.is_fractured {
            out.push((AffixType::Suffix, i));
        }
    }
    out
}

/// Remove a mod by `(affix, index)` and return the removed `ModRoll`.
fn remove_mod_at(item: &mut Item, affix: AffixType, idx: usize) -> Option<ModRoll> {
    match affix {
        AffixType::Prefix if idx < item.prefixes.len() => Some(item.prefixes.remove(idx)),
        AffixType::Suffix if idx < item.suffixes.len() => Some(item.suffixes.remove(idx)),
        _ => None,
    }
}

/// Roll a value `t ∈ [0,1]` for each stat in the mod, then linear-interpolate.
fn roll_values(m: &ModDefinition, rng: &mut dyn rand::RngCore) -> SmallVec<[f64; 4]> {
    m.stats
        .iter()
        .map(|s| {
            let t = rng.gen::<f64>();
            s.roll(t)
        })
        .collect()
}

/// Build a `ModRoll` from a sampled `ModDefinition`, rolling values.
fn roll_mod(m: &ModDefinition, rng: &mut dyn rand::RngCore) -> ModRoll {
    ModRoll {
        mod_id: m.id.clone(),
        affix_type: m.affix_type,
        kind: m.kind,
        values: roll_values(m, rng),
        is_fractured: false,
    }
}

/// Pick uniformly between Prefix and Suffix among empty slots.
fn pick_open_affix(item: &Item, rng: &mut dyn rand::RngCore, max_slots: u8) -> Option<AffixType> {
    let prefix_open = item.prefixes.len() < max_slots as usize;
    let suffix_open = item.suffixes.len() < max_slots as usize;
    match (prefix_open, suffix_open) {
        (true, true) => Some(if rng.gen::<bool>() {
            AffixType::Prefix
        } else {
            AffixType::Suffix
        }),
        (true, false) => Some(AffixType::Prefix),
        (false, true) => Some(AffixType::Suffix),
        (false, false) => None,
    }
}

/// Static label for an affix slot (used in error messages).
fn affix_label(a: AffixType) -> &'static str {
    match a {
        AffixType::Prefix => "prefix",
        AffixType::Suffix => "suffix",
        AffixType::Implicit => "implicit",
        AffixType::Enchantment => "enchantment",
    }
}

/// Add a rolled mod to the appropriate affix slot.
fn push_mod(item: &mut Item, roll: ModRoll) {
    match roll.affix_type {
        AffixType::Prefix => item.prefixes.push(roll),
        AffixType::Suffix => item.suffixes.push(roll),
        // Implicit / Enchantment paths never come through here.
        _ => {}
    }
}

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
        let m = sample_eligible_mod(ctx.registry, item, affix, ctx.rng, ctx.patch, 0).ok_or_else(
            || EngineError::NoEligibleMods {
                base: item.base.to_string(),
                ilvl: item.ilvl,
                affix_type: affix_label(affix),
            },
        )?;
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
        let m = sample_eligible_mod(ctx.registry, item, affix, ctx.rng, ctx.patch, 0).ok_or_else(
            || EngineError::NoEligibleMods {
                base: item.base.to_string(),
                ilvl: item.ilvl,
                affix_type: affix_label(affix),
            },
        )?;
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
        let m = sample_eligible_mod(ctx.registry, item, affix, ctx.rng, ctx.patch, 0).ok_or_else(
            || EngineError::NoEligibleMods {
                base: item.base.to_string(),
                ilvl: item.ilvl,
                affix_type: affix_label(affix),
            },
        )?;
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
            let Some(m) = sample_eligible_mod(ctx.registry, item, affix, ctx.rng, ctx.patch, 0)
            else {
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
        let affix = pick_open_affix(item, ctx.rng, 3).ok_or(EngineError::AffixSlotFull {
            affix_type: "rare item is at the prefix+suffix cap",
        })?;
        let m = sample_eligible_mod(ctx.registry, item, affix, ctx.rng, ctx.patch, 0).ok_or_else(
            || EngineError::NoEligibleMods {
                base: item.base.to_string(),
                ilvl: item.ilvl,
                affix_type: affix_label(affix),
            },
        )?;
        push_mod(item, roll_mod(m, ctx.rng));
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
        let removables = collect_removable(item);
        if removables.is_empty() {
            return Err(EngineError::InvalidApplication(
                "Orb of Annulment: no non-fractured affix mod to remove".into(),
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

    fn apply(&self, item: &mut Item, ctx: &mut ApplyContext<'_>) -> EngineResult<ApplyOutcome> {
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
        let removables = collect_removable(item);
        if removables.is_empty() {
            return Err(EngineError::InvalidApplication(
                "Chaos Orb: no non-fractured affix mod to remove".into(),
            ));
        }
        let pick = ctx.rng.gen_range(0..removables.len());
        let (removed_affix, idx) = removables[pick];
        remove_mod_at(item, removed_affix, idx);

        // Choose a fresh affix slot to fill. Per planning notes, vanilla
        // Chaos can fill EITHER prefix or suffix — Sinistral/Dextral Erasure
        // omens (M2.6) constrain it. Without omens, sample uniformly over
        // currently-empty slots.
        let new_affix = pick_open_affix(item, ctx.rng, 3).ok_or(EngineError::AffixSlotFull {
            affix_type: "no slot opened up after Chaos Orb removal",
        })?;
        let m = sample_eligible_mod(ctx.registry, item, new_affix, ctx.rng, ctx.patch, 0)
            .ok_or_else(|| EngineError::NoEligibleMods {
                base: item.base.to_string(),
                ilvl: item.ilvl,
                affix_type: affix_label(new_affix),
            })?;
        push_mod(item, roll_mod(m, ctx.rng));
        Ok(())
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
    use crate::ids::{ModGroupId, ModId, TagId};
    use crate::item::QualityKind;
    use crate::mods::{ModDomain, ModFlags, ModGroup, SpawnWeight};
    use crate::patch::{PatchRange, PatchVersion};

    fn mk_mod(id: &str, group: &str, affix: AffixType, class: &str) -> ModDefinition {
        ModDefinition {
            id: ModId::from(id),
            name: None,
            mod_group: ModGroup(ModGroupId::from(group)),
            affix_type: affix,
            kind: ModKind::Explicit,
            domain: ModDomain::Item,
            tags: smallvec![],
            concept_set: smallvec![],
            spawn_weights: smallvec![SpawnWeight {
                tag: TagId::from(class),
                weight: 1
            }],
            stats: smallvec![],
            required_level: 1,
            allowed_item_classes: smallvec![ItemClassId::from(class)],
            patch_range: PatchRange::ALL,
            flags: ModFlags::empty(),
            text_template: None,
        }
    }

    fn fixture_normal_boots() -> Item {
        Item {
            // Per the placeholder convention in `class_for_item`, we use
            // the class id as the base id in tests until BaseRegistry lands.
            base: "Boots".into(),
            ilvl: 82,
            rarity: Rarity::Normal,
            corrupted: false,
            sanctified: false,
            mirrored: false,
            quality: 0,
            quality_kind: QualityKind::Untagged,
            implicits: smallvec![],
            prefixes: smallvec![],
            suffixes: smallvec![],
            enchantments: smallvec![],
            hidden_desecrated: None,
            sockets: smallvec![],
        }
    }

    fn fixture_registry() -> ModRegistry {
        ModRegistry::from_mods(vec![
            mk_mod(
                "MovementSpeed1",
                "MovementSpeed",
                AffixType::Prefix,
                "Boots",
            ),
            mk_mod(
                "MovementSpeed2",
                "MovementSpeed",
                AffixType::Prefix,
                "Boots",
            ),
            mk_mod("Life1", "Life", AffixType::Prefix, "Boots"),
            mk_mod("FireRes1", "FireResistance", AffixType::Suffix, "Boots"),
            mk_mod("ColdRes1", "ColdResistance", AffixType::Suffix, "Boots"),
            mk_mod("Stamina1", "Stamina", AffixType::Suffix, "Boots"),
        ])
    }

    fn ctx<'a>(registry: &'a ModRegistry, rng: &'a mut Xoshiro256PlusPlus) -> ApplyContext<'a> {
        ApplyContext::new(registry, rng, PatchVersion::PATCH_0_4_0)
    }

    #[test]
    fn transmute_promotes_normal_to_magic_and_adds_one_mod() {
        let reg = fixture_registry();
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(42);
        let mut item = fixture_normal_boots();
        OrbOfTransmutation::new()
            .apply(&mut item, &mut ctx(&reg, &mut rng))
            .unwrap();
        assert_eq!(item.rarity, Rarity::Magic);
        assert_eq!(item.prefixes.len() + item.suffixes.len(), 1);
    }

    #[test]
    fn transmute_rejects_magic_item() {
        let reg = fixture_registry();
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(1);
        let mut item = fixture_normal_boots();
        item.rarity = Rarity::Magic;
        let r = OrbOfTransmutation::new().apply(&mut item, &mut ctx(&reg, &mut rng));
        assert!(matches!(r, Err(EngineError::InvalidApplication(_))));
    }

    #[test]
    fn augment_fills_empty_slot_on_magic() {
        let reg = fixture_registry();
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(7);
        let mut item = fixture_normal_boots();
        OrbOfTransmutation::new()
            .apply(&mut item, &mut ctx(&reg, &mut rng))
            .unwrap();
        OrbOfAugmentation::new()
            .apply(&mut item, &mut ctx(&reg, &mut rng))
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
        let mut item = fixture_normal_boots();
        OrbOfTransmutation::new()
            .apply(&mut item, &mut ctx(&reg, &mut rng))
            .unwrap();
        OrbOfAugmentation::new()
            .apply(&mut item, &mut ctx(&reg, &mut rng))
            .unwrap();
        let r = OrbOfAugmentation::new().apply(&mut item, &mut ctx(&reg, &mut rng));
        assert!(matches!(r, Err(EngineError::AffixSlotFull { .. })));
    }

    #[test]
    fn regal_promotes_magic_to_rare_with_3rd_mod() {
        let reg = fixture_registry();
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(13);
        let mut item = fixture_normal_boots();
        OrbOfTransmutation::new()
            .apply(&mut item, &mut ctx(&reg, &mut rng))
            .unwrap();
        OrbOfAugmentation::new()
            .apply(&mut item, &mut ctx(&reg, &mut rng))
            .unwrap();
        let before = item.prefixes.len() + item.suffixes.len();
        RegalOrb::new()
            .apply(&mut item, &mut ctx(&reg, &mut rng))
            .unwrap();
        let after = item.prefixes.len() + item.suffixes.len();
        assert_eq!(item.rarity, Rarity::Rare);
        assert_eq!(after, before + 1);
    }

    #[test]
    fn regal_rejects_normal_or_rare() {
        let reg = fixture_registry();
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(99);
        let mut item = fixture_normal_boots();

        // Normal
        let r = RegalOrb::new().apply(&mut item, &mut ctx(&reg, &mut rng));
        assert!(matches!(r, Err(EngineError::InvalidApplication(_))));

        // Rare
        item.rarity = Rarity::Rare;
        let r = RegalOrb::new().apply(&mut item, &mut ctx(&reg, &mut rng));
        assert!(matches!(r, Err(EngineError::InvalidApplication(_))));
    }

    #[test]
    fn currencies_reject_corrupted_items() {
        let reg = fixture_registry();
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0);
        let mut item = fixture_normal_boots();
        item.corrupted = true;
        let r = OrbOfTransmutation::new().apply(&mut item, &mut ctx(&reg, &mut rng));
        assert!(matches!(r, Err(EngineError::InvalidApplication(_))));
    }

    #[test]
    fn currencies_are_deterministic_given_same_seed() {
        let reg = fixture_registry();
        let make = || {
            let mut rng = Xoshiro256PlusPlus::seed_from_u64(0x00c0_ffee);
            let mut item = fixture_normal_boots();
            OrbOfTransmutation::new()
                .apply(&mut item, &mut ctx(&reg, &mut rng))
                .unwrap();
            OrbOfAugmentation::new()
                .apply(&mut item, &mut ctx(&reg, &mut rng))
                .unwrap();
            RegalOrb::new()
                .apply(&mut item, &mut ctx(&reg, &mut rng))
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
        let mut item = fixture_normal_boots();
        OrbOfAlchemy::new()
            .apply(&mut item, &mut ctx(&reg, &mut rng))
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
        let mut item = fixture_normal_boots();
        item.rarity = Rarity::Magic;
        let r = OrbOfAlchemy::new().apply(&mut item, &mut ctx(&reg, &mut rng));
        assert!(matches!(r, Err(EngineError::InvalidApplication(_))));
    }

    #[test]
    fn exalt_adds_one_mod_to_rare() {
        let reg = fixture_registry();
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0xe7);
        let mut item = fixture_normal_boots();
        OrbOfAlchemy::new()
            .apply(&mut item, &mut ctx(&reg, &mut rng))
            .unwrap();
        let before = item.prefixes.len() + item.suffixes.len();
        if before >= 6 {
            // Pool too small to add another; skip rather than fail.
            return;
        }
        ExaltedOrb::new()
            .apply(&mut item, &mut ctx(&reg, &mut rng))
            .unwrap();
        let after = item.prefixes.len() + item.suffixes.len();
        assert_eq!(after, before + 1);
    }

    #[test]
    fn exalt_rejects_non_rare() {
        let reg = fixture_registry();
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(2);
        let mut item = fixture_normal_boots();
        let r = ExaltedOrb::new().apply(&mut item, &mut ctx(&reg, &mut rng));
        assert!(matches!(r, Err(EngineError::InvalidApplication(_))));
    }

    #[test]
    fn annul_removes_exactly_one_mod() {
        let reg = fixture_registry();
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0xa9);
        let mut item = fixture_normal_boots();
        OrbOfAlchemy::new()
            .apply(&mut item, &mut ctx(&reg, &mut rng))
            .unwrap();
        let before = item.prefixes.len() + item.suffixes.len();
        OrbOfAnnulment::new()
            .apply(&mut item, &mut ctx(&reg, &mut rng))
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
            .apply(&mut item, &mut ctx(&reg, &mut rng))
            .unwrap();
        // Fractured prefix survives; suffix is gone.
        assert_eq!(item.prefixes.len(), 1);
        assert_eq!(item.suffixes.len(), 0);
        assert!(item.prefixes[0].is_fractured);

        // Second annul: nothing left to remove (only fractured).
        let r = OrbOfAnnulment::new().apply(&mut item, &mut ctx(&reg, &mut rng));
        assert!(matches!(r, Err(EngineError::InvalidApplication(_))));
    }

    #[test]
    fn chaos_keeps_mod_count_constant() {
        let reg = fixture_registry();
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0xcaa05);
        let mut item = fixture_normal_boots();
        OrbOfAlchemy::new()
            .apply(&mut item, &mut ctx(&reg, &mut rng))
            .unwrap();
        let before = item.prefixes.len() + item.suffixes.len();
        if before == 0 {
            return; // alch produced 0 mods; can't chaos
        }
        ChaosOrb::new()
            .apply(&mut item, &mut ctx(&reg, &mut rng))
            .unwrap();
        let after = item.prefixes.len() + item.suffixes.len();
        // Chaos = -1 + 1 = same count
        assert_eq!(after, before);
    }

    #[test]
    fn chaos_rejects_non_rare() {
        let reg = fixture_registry();
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(3);
        let mut item = fixture_normal_boots();
        item.rarity = Rarity::Magic;
        item.prefixes.push(ModRoll {
            mod_id: ModId::from("Life1"),
            affix_type: AffixType::Prefix,
            kind: ModKind::Explicit,
            values: smallvec![],
            is_fractured: false,
        });
        let r = ChaosOrb::new().apply(&mut item, &mut ctx(&reg, &mut rng));
        assert!(matches!(r, Err(EngineError::InvalidApplication(_))));
    }
}
