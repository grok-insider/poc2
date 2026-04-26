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

/// Filtered variant for omen-aware Annul/Chaos paths.
///
/// - `affix_filter` (Some(_)) restricts the result to mods of that affix
///   type (Sinistral/Dextral Annulment, Sinistral/Dextral Erasure).
/// - `desecrated_only` restricts to mods of `kind = Desecrated` (Omen of
///   Light: next Annul removes only Desecrated mods).
fn collect_removable_filtered(
    item: &Item,
    affix_filter: Option<AffixType>,
    desecrated_only: bool,
) -> SmallVec<[(AffixType, usize); 8]> {
    let mut out = SmallVec::new();
    if affix_filter != Some(AffixType::Suffix) {
        for (i, m) in item.prefixes.iter().enumerate() {
            if m.is_fractured {
                continue;
            }
            if desecrated_only && m.kind != crate::mods::ModKind::Desecrated {
                continue;
            }
            out.push((AffixType::Prefix, i));
        }
    }
    if affix_filter != Some(AffixType::Prefix) {
        for (i, m) in item.suffixes.iter().enumerate() {
            if m.is_fractured {
                continue;
            }
            if desecrated_only && m.kind != crate::mods::ModKind::Desecrated {
                continue;
            }
            out.push((AffixType::Suffix, i));
        }
    }
    out
}

/// Find the lowest-required-level mod among the candidates. Used by
/// Omen of Whittling. Returns the index in `candidates` of the chosen mod,
/// or `None` if `candidates` is empty.
fn pick_lowest_mod_level(
    item: &Item,
    candidates: &[(AffixType, usize)],
    registry: &ModRegistry,
) -> Option<usize> {
    let mut best_idx = None;
    let mut best_lvl = u32::MAX;
    for (i, (slot, idx)) in candidates.iter().enumerate() {
        let roll = match slot {
            AffixType::Prefix => &item.prefixes[*idx],
            AffixType::Suffix => &item.suffixes[*idx],
            _ => continue,
        };
        if let Some(def) = registry.get(&roll.mod_id) {
            if def.required_level < best_lvl {
                best_lvl = def.required_level;
                best_idx = Some(i);
            }
        }
    }
    best_idx
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

/// Pick a Prefix-or-Suffix slot among empty slots.
///
/// Without omens: uniform random over open slots.
/// With Sinistral/Dextral *Exaltation* (or any AffixOnly omen): forced to
/// the omen's affix if a slot is open; otherwise the omen is consumed but
/// no slot is opened (caller errors with [`EngineError::AffixSlotFull`]).
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

/// Pick a Prefix-or-Suffix, consulting omens. Used by Exalt-class currencies
/// (Exalted, Greater/Perfect Exalted) where Sinistral/Dextral Exaltation
/// constrains the choice.
fn pick_open_affix_with_omen(
    item: &Item,
    ctx: &mut ApplyContext<'_>,
    max_slots: u8,
) -> Option<AffixType> {
    if let Some(forced) = ctx.omens.consume_affix_only(ctx.patch) {
        let occupied = match forced {
            AffixType::Prefix => item.prefixes.len() >= max_slots as usize,
            AffixType::Suffix => item.suffixes.len() >= max_slots as usize,
            _ => return None,
        };
        if occupied {
            return None;
        }
        return Some(forced);
    }
    pick_open_affix(item, ctx.rng, max_slots)
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
            let m = sample_eligible_mod(ctx.registry, item, affix, ctx.rng, ctx.patch, 0)
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

    fn apply(&self, item: &mut Item, ctx: &mut ApplyContext<'_>) -> EngineResult<ApplyOutcome> {
        chaos_apply(item, ctx, 0)
    }
}

/// Shared Chaos apply. Used by both vanilla [`ChaosOrb`] and the Greater /
/// Perfect variants (which thread a min-mod-level through).
fn chaos_apply(
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
    let m = sample_eligible_mod(ctx.registry, item, new_affix, ctx.rng, ctx.patch, min_level)
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
/// - Ranges come from the underlying [`ModDefinition::stats`]; we look up
///   each `ModRoll`'s `mod_id` in the registry and reroll uniformly within
///   `[min, max]` for every stat in the mod.
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
        reroll_explicit_values(item, ctx);
        Ok(())
    }
}

fn reroll_explicit_values(item: &mut Item, ctx: &mut ApplyContext<'_>) {
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

// =========================================================================
// Vaal Orb
// =========================================================================

/// What kind of corruption outcome did Vaal produce?
///
/// Reported by the engine for diagnostics / advisor explanation. Mirrors
/// the outcomes documented in `/docs/11-game-mechanics.md` (lands in M2.10):
///
/// - `NoChange` — item is corrupted but otherwise unchanged. Removable by
///   Omen of Corruption (M2.6).
/// - `RerollValues` — divine-like reroll across explicit mods.
/// - `BrickMods` — strips all explicit mods and replaces them with a
///   simulated brick (here: just rerolls one prefix to a "useless" state;
///   real brick semantics land when desecrated mod data is integrated).
/// - `AddEnchantment` — adds a corrupted enchantment (placeholder for now).
/// - `AddSocket` — adds a socket beyond the normal cap.
/// - `AddQuality` — bumps quality past the cap (caps at +30).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VaalOutcome {
    NoChange,
    RerollValues,
    BrickMods,
    AddEnchantment,
    AddSocket,
    AddQuality,
}

/// Vaal Orb: corrupt the item with one of several random outcomes.
///
/// Once corrupted, the item is locked from most further crafting (only
/// Architect's Orb double-corrupt, Vaal Cultivation Orb on uniques, and a
/// handful of omens still apply).
///
/// The outcome distribution is approximated for M2.4 — refined in M2.5
/// when omen-conditioning lands. For now we use uniform 1/6 across the
/// six outcomes; Omen of Corruption (M2.6) will remove `NoChange`.
#[derive(Debug)]
pub struct VaalOrb {
    id: CurrencyId,
}

impl VaalOrb {
    pub fn new() -> Self {
        Self {
            id: CurrencyId::from("VaalOrb"),
        }
    }
}

impl Default for VaalOrb {
    fn default() -> Self {
        Self::new()
    }
}

impl Currency for VaalOrb {
    fn id(&self) -> &CurrencyId {
        &self.id
    }
    fn name(&self) -> &'static str {
        "Vaal Orb"
    }

    fn apply(&self, item: &mut Item, ctx: &mut ApplyContext<'_>) -> EngineResult<ApplyOutcome> {
        if item.corrupted {
            return Err(EngineError::ItemCorrupted);
        }
        if item.sanctified {
            return Err(EngineError::ItemSanctified);
        }
        if item.mirrored {
            return Err(EngineError::InvalidApplication(
                "Vaal Orb cannot be applied to a mirrored item".into(),
            ));
        }

        let outcome = sample_vaal_outcome(ctx.rng);
        item.corrupted = true;
        match outcome {
            // No-change AND placeholder-enchantment both leave the rolled
            // mods alone. Real enchantment list comes via the corrupted
            // mod-domain data in M2.6; until then they're identical.
            VaalOutcome::NoChange | VaalOutcome::AddEnchantment => {}
            VaalOutcome::RerollValues => reroll_explicit_values(item, ctx),
            VaalOutcome::BrickMods => {
                // Approximation: clear non-fractured mods and add no replacement.
                // Full "brick" semantics (replace with corrupted-only mods)
                // lands when corrupted mod-domain data is integrated.
                item.prefixes.retain(|m| m.is_fractured);
                item.suffixes.retain(|m| m.is_fractured);
            }
            VaalOutcome::AddSocket => {
                // Vaal can add a socket beyond the cap; we just push an empty one.
                item.sockets.push(crate::item::Socket { augment: None });
            }
            VaalOutcome::AddQuality => {
                item.quality = item.quality.saturating_add(5).min(30);
            }
        }
        Ok(())
    }
}

fn sample_vaal_outcome(rng: &mut dyn rand::RngCore) -> VaalOutcome {
    match rng.gen_range(0u8..6u8) {
        0 => VaalOutcome::NoChange,
        1 => VaalOutcome::RerollValues,
        2 => VaalOutcome::BrickMods,
        3 => VaalOutcome::AddEnchantment,
        4 => VaalOutcome::AddSocket,
        _ => VaalOutcome::AddQuality,
    }
}

// =========================================================================
// Greater / Perfect tier variants
// =========================================================================
//
// Greater and Perfect variants of Transmute / Aug / Regal / Exalt / Chaos
// behave identically to their base counterparts EXCEPT that the added mod
// is constrained to `required_level >= min_mod_level`. This raises the
// expected tier of the added mod (the 'rules out the lower tiers' effect
// described in the apprentice blueprint).
//
// Min mod-level gates per planning research:
// - Greater Transmute / Aug:  ~35  (Aug = 55 is also seen; we use 35 for
//   Transmute and 55 for Aug per RePoE-fork conventions)
// - Greater Regal / Exalt / Chaos: ~50
// - Perfect (all variants):    ~70
//
// The actual numerical thresholds vary slightly across community tracking
// of patch 0.4. We codify the conservative-floor values here; a future
// refinement pass (M2.5+) can refine them from poe2db tier tables.

const MIN_LEVEL_GREATER_TRANSMUTE: u32 = 35;
const MIN_LEVEL_GREATER_AUGMENT: u32 = 55;
const MIN_LEVEL_GREATER_REGAL: u32 = 50;
const MIN_LEVEL_GREATER_EXALT: u32 = 50;
const MIN_LEVEL_GREATER_CHAOS: u32 = 50;
const MIN_LEVEL_PERFECT_ALL: u32 = 70;

/// Generic implementation of "promote rarity, add 1 mod ≥ min_level".
/// Shared by Transmute / Greater / Perfect Transmutation variants.
fn add_one_mod_with_min(
    item: &mut Item,
    ctx: &mut ApplyContext<'_>,
    require_rarity: Rarity,
    promote_to: Option<Rarity>,
    max_slots: u8,
    min_level: u32,
    name: &'static str,
) -> EngineResult<()> {
    if !item.is_modifiable() {
        return Err(EngineError::InvalidApplication(format!(
            "{name} requires a modifiable item"
        )));
    }
    if item.rarity != require_rarity {
        return Err(EngineError::InvalidApplication(format!(
            "{name} requires a {require_rarity:?}-rarity item"
        )));
    }
    let affix = pick_open_affix(item, ctx.rng, max_slots)
        .ok_or(EngineError::AffixSlotFull { affix_type: name })?;
    let m = sample_eligible_mod(ctx.registry, item, affix, ctx.rng, ctx.patch, min_level)
        .ok_or_else(|| EngineError::NoEligibleMods {
            base: item.base.to_string(),
            ilvl: item.ilvl,
            affix_type: affix_label(affix),
        })?;
    if let Some(rar) = promote_to {
        item.rarity = rar;
    }
    push_mod(item, roll_mod(m, ctx.rng));
    Ok(())
}

/// Generic Chaos-with-min-level used by Greater/Perfect Chaos variants.
/// Delegates to [`chaos_apply`] which already handles omens.
fn chaos_with_min(
    item: &mut Item,
    ctx: &mut ApplyContext<'_>,
    min_level: u32,
    _name: &'static str,
) -> EngineResult<()> {
    chaos_apply(item, ctx, min_level)
}

/// Defines a Greater/Perfect tier currency that wraps `add_one_mod_with_min`.
macro_rules! greater_perfect_add_currency {
    (
        $struct:ident,
        $id:literal,
        $disp:literal,
        $require:expr,
        $promote:expr,
        $max_slots:expr,
        $min_level:expr
    ) => {
        #[derive(Debug)]
        pub struct $struct {
            id: CurrencyId,
        }
        impl $struct {
            pub fn new() -> Self {
                Self {
                    id: CurrencyId::from($id),
                }
            }
        }
        impl Default for $struct {
            fn default() -> Self {
                Self::new()
            }
        }
        impl Currency for $struct {
            fn id(&self) -> &CurrencyId {
                &self.id
            }
            fn name(&self) -> &'static str {
                $disp
            }
            fn apply(
                &self,
                item: &mut Item,
                ctx: &mut ApplyContext<'_>,
            ) -> EngineResult<ApplyOutcome> {
                add_one_mod_with_min(item, ctx, $require, $promote, $max_slots, $min_level, $disp)
            }
        }
    };
}

/// Defines a Greater/Perfect tier Chaos.
macro_rules! greater_perfect_chaos {
    ($struct:ident, $id:literal, $disp:literal, $min_level:expr) => {
        #[derive(Debug)]
        pub struct $struct {
            id: CurrencyId,
        }
        impl $struct {
            pub fn new() -> Self {
                Self {
                    id: CurrencyId::from($id),
                }
            }
        }
        impl Default for $struct {
            fn default() -> Self {
                Self::new()
            }
        }
        impl Currency for $struct {
            fn id(&self) -> &CurrencyId {
                &self.id
            }
            fn name(&self) -> &'static str {
                $disp
            }
            fn apply(
                &self,
                item: &mut Item,
                ctx: &mut ApplyContext<'_>,
            ) -> EngineResult<ApplyOutcome> {
                chaos_with_min(item, ctx, $min_level, $disp)
            }
        }
    };
}

// Transmutation -----------------------------------------------------------
greater_perfect_add_currency!(
    GreaterOrbOfTransmutation,
    "GreaterOrbOfTransmutation",
    "Greater Orb of Transmutation",
    Rarity::Normal,
    Some(Rarity::Magic),
    1,
    MIN_LEVEL_GREATER_TRANSMUTE
);
greater_perfect_add_currency!(
    PerfectOrbOfTransmutation,
    "PerfectOrbOfTransmutation",
    "Perfect Orb of Transmutation",
    Rarity::Normal,
    Some(Rarity::Magic),
    1,
    MIN_LEVEL_PERFECT_ALL
);

// Augmentation ------------------------------------------------------------
greater_perfect_add_currency!(
    GreaterOrbOfAugmentation,
    "GreaterOrbOfAugmentation",
    "Greater Orb of Augmentation",
    Rarity::Magic,
    None,
    1,
    MIN_LEVEL_GREATER_AUGMENT
);
greater_perfect_add_currency!(
    PerfectOrbOfAugmentation,
    "PerfectOrbOfAugmentation",
    "Perfect Orb of Augmentation",
    Rarity::Magic,
    None,
    1,
    MIN_LEVEL_PERFECT_ALL
);

// Regal -------------------------------------------------------------------
greater_perfect_add_currency!(
    GreaterRegalOrb,
    "GreaterRegalOrb",
    "Greater Regal Orb",
    Rarity::Magic,
    Some(Rarity::Rare),
    3,
    MIN_LEVEL_GREATER_REGAL
);
greater_perfect_add_currency!(
    PerfectRegalOrb,
    "PerfectRegalOrb",
    "Perfect Regal Orb",
    Rarity::Magic,
    Some(Rarity::Rare),
    3,
    MIN_LEVEL_PERFECT_ALL
);

// Exalted -----------------------------------------------------------------
greater_perfect_add_currency!(
    GreaterExaltedOrb,
    "GreaterExaltedOrb",
    "Greater Exalted Orb",
    Rarity::Rare,
    None,
    3,
    MIN_LEVEL_GREATER_EXALT
);
greater_perfect_add_currency!(
    PerfectExaltedOrb,
    "PerfectExaltedOrb",
    "Perfect Exalted Orb",
    Rarity::Rare,
    None,
    3,
    MIN_LEVEL_PERFECT_ALL
);

// Chaos -------------------------------------------------------------------
greater_perfect_chaos!(
    GreaterChaosOrb,
    "GreaterChaosOrb",
    "Greater Chaos Orb",
    MIN_LEVEL_GREATER_CHAOS
);
greater_perfect_chaos!(
    PerfectChaosOrb,
    "PerfectChaosOrb",
    "Perfect Chaos Orb",
    MIN_LEVEL_PERFECT_ALL
);

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
            hinekora_lock: None,
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

    fn ctx<'a>(
        registry: &'a ModRegistry,
        rng: &'a mut Xoshiro256PlusPlus,
        omens: &'a mut crate::omen::OmenSet,
    ) -> ApplyContext<'a> {
        ApplyContext::new(registry, rng, PatchVersion::PATCH_0_4_0, omens)
    }

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
        let reg = ModRegistry::from_mods(vec![ModDefinition {
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
            allowed_item_classes: smallvec![ItemClassId::from("Boots")],
            patch_range: PatchRange::ALL,
            flags: ModFlags::empty(),
            text_template: None,
        }]);

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
        let reg = ModRegistry::from_mods(vec![ModDefinition {
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
            allowed_item_classes: smallvec![ItemClassId::from("Boots")],
            patch_range: PatchRange::ALL,
            flags: ModFlags::empty(),
            text_template: None,
        }]);

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

    // ---- Vaal --------------------------------------------------------------

    #[test]
    fn vaal_marks_item_corrupted() {
        let reg = fixture_registry();
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0x1);
        let mut omens = crate::omen::OmenSet::new();
        let mut item = fixture_normal_boots();
        item.rarity = Rarity::Rare;
        VaalOrb::new()
            .apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens))
            .unwrap();
        assert!(item.corrupted);
    }

    #[test]
    fn vaal_rejects_already_corrupted() {
        let reg = fixture_registry();
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0x2);
        let mut omens = crate::omen::OmenSet::new();
        let mut item = fixture_normal_boots();
        item.corrupted = true;
        let r = VaalOrb::new().apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens));
        assert!(matches!(r, Err(EngineError::ItemCorrupted)));
    }

    #[test]
    fn vaal_rejects_sanctified_or_mirrored() {
        let reg = fixture_registry();
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0x3);
        let mut omens = crate::omen::OmenSet::new();
        let mut item = fixture_normal_boots();
        item.sanctified = true;
        let r = VaalOrb::new().apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens));
        assert!(matches!(r, Err(EngineError::ItemSanctified)));

        let mut item = fixture_normal_boots();
        item.mirrored = true;
        let r = VaalOrb::new().apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens));
        assert!(matches!(r, Err(EngineError::InvalidApplication(_))));
    }

    #[test]
    fn vaal_outcome_distribution_covers_all_six_branches() {
        // Statistical: across 600 trials, every outcome variant should appear.
        use std::collections::HashSet;
        let mut seen: HashSet<u8> = HashSet::new();
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0x600);
        let mut _omens = crate::omen::OmenSet::new();
        for _ in 0..600 {
            let outcome = sample_vaal_outcome(&mut rng);
            seen.insert(outcome as u8);
        }
        assert_eq!(seen.len(), 6, "saw {} distinct outcomes", seen.len());
    }

    // ---- Greater / Perfect variants ----------------------------------------

    fn fixture_tiered_registry() -> ModRegistry {
        // Multiple prefix and suffix groups, each with mods at varied
        // required_level so we can demonstrate min-mod-level filtering
        // regardless of which affix the orb picks.
        ModRegistry::from_mods(vec![
            // Prefixes - Life group
            mk_mod_lvl("Life_T3", "Life", AffixType::Prefix, "Boots", 1),
            mk_mod_lvl("Life_T2", "Life", AffixType::Prefix, "Boots", 40),
            mk_mod_lvl("Life_T1", "Life", AffixType::Prefix, "Boots", 75),
            // Prefixes - ES group
            mk_mod_lvl("ES_T3", "ES", AffixType::Prefix, "Boots", 1),
            mk_mod_lvl("ES_T1", "ES", AffixType::Prefix, "Boots", 75),
            // Prefixes - Mana group
            mk_mod_lvl("Mana_T3", "Mana", AffixType::Prefix, "Boots", 1),
            mk_mod_lvl("Mana_T1", "Mana", AffixType::Prefix, "Boots", 75),
            // Suffixes - FireRes group
            mk_mod_lvl("FireRes_T3", "FireRes", AffixType::Suffix, "Boots", 1),
            mk_mod_lvl("FireRes_T1", "FireRes", AffixType::Suffix, "Boots", 75),
            // Suffixes - ColdRes group
            mk_mod_lvl("ColdRes_T3", "ColdRes", AffixType::Suffix, "Boots", 1),
            mk_mod_lvl("ColdRes_T1", "ColdRes", AffixType::Suffix, "Boots", 75),
            // Suffixes - Stamina group (movement-speed-equivalent)
            mk_mod_lvl("Stamina_T3", "Stamina", AffixType::Suffix, "Boots", 1),
            mk_mod_lvl("Stamina_T1", "Stamina", AffixType::Suffix, "Boots", 75),
        ])
    }

    fn mk_mod_lvl(
        id: &str,
        group: &str,
        affix: AffixType,
        class: &str,
        required_level: u32,
    ) -> ModDefinition {
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
            required_level,
            allowed_item_classes: smallvec![ItemClassId::from(class)],
            patch_range: PatchRange::ALL,
            flags: ModFlags::empty(),
            text_template: None,
        }
    }

    #[test]
    fn perfect_transmute_only_rolls_high_required_level_mods() {
        // Perfect demands required_level >= 70. With our fixture, the only
        // Life mod that qualifies is Life_T1 (req 75); Life_T2 (40) and
        // Life_T3 (1) are filtered out.
        let reg = fixture_tiered_registry();
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0x9001);
        let mut omens = crate::omen::OmenSet::new();
        let mut item = fixture_normal_boots();
        PerfectOrbOfTransmutation::new()
            .apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens))
            .unwrap();
        // The single rolled mod must be one of the T1 (req >= 70) candidates.
        let roll = item
            .prefixes
            .first()
            .or_else(|| item.suffixes.first())
            .unwrap();
        assert!(roll.mod_id.as_str().ends_with("_T1"), "got {}", roll.mod_id);
    }

    #[test]
    fn greater_regal_filters_below_min_level() {
        // Greater Regal: min level 50. Life_T1 (75) and FireRes_T1 (75)
        // qualify; nothing else does. With the seed below, we should still
        // land on a high-tier mod.
        let reg = fixture_tiered_registry();
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0x9002);
        let mut omens = crate::omen::OmenSet::new();
        let mut item = fixture_normal_boots();
        OrbOfTransmutation::new()
            .apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens))
            .unwrap();
        GreaterRegalOrb::new()
            .apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens))
            .unwrap();
        assert_eq!(item.rarity, Rarity::Rare);
        // Among the up-to-3 mods on the Rare, the Regal-added one must be a
        // _T1. Since the Transmute step had no min-level constraint we can
        // only assert *at least one* mod is a T1.
        let any_t1 = item
            .prefixes
            .iter()
            .chain(item.suffixes.iter())
            .any(|m| m.mod_id.as_str().ends_with("_T1"));
        assert!(any_t1, "expected at least one T1 mod after Greater Regal");
    }

    #[test]
    fn perfect_exalt_filters_below_70() {
        // Set up a Rare with one mod, then Perfect Exalt — the new mod
        // must be required_level >= 70.
        let reg = fixture_tiered_registry();
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0x9003);
        let mut omens = crate::omen::OmenSet::new();
        let mut item = fixture_normal_boots();
        item.rarity = Rarity::Rare;
        item.prefixes.push(ModRoll {
            mod_id: ModId::from("Life_T3"),
            affix_type: AffixType::Prefix,
            kind: ModKind::Explicit,
            values: smallvec![],
            is_fractured: false,
        });
        PerfectExaltedOrb::new()
            .apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens))
            .unwrap();
        // The newly added mod (the LAST one in either prefixes or suffixes)
        // must end with _T1 since that's the only required_level>=70 mod
        // available given mod-group exclusivity (Life is occupied by T3).
        let last = item
            .suffixes
            .last()
            .or_else(|| item.prefixes.last())
            .unwrap();
        assert!(last.mod_id.as_str().ends_with("_T1"), "got {}", last.mod_id);
    }

    #[test]
    fn perfect_chaos_replacement_is_high_tier() {
        // Build a Rare with a T3 mod, then Perfect Chaos: the replacement
        // mod must be required_level >= 70.
        let reg = fixture_tiered_registry();
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0x9004);
        let mut omens = crate::omen::OmenSet::new();
        let mut item = fixture_normal_boots();
        item.rarity = Rarity::Rare;
        item.prefixes.push(ModRoll {
            mod_id: ModId::from("Life_T3"),
            affix_type: AffixType::Prefix,
            kind: ModKind::Explicit,
            values: smallvec![],
            is_fractured: false,
        });
        PerfectChaosOrb::new()
            .apply(&mut item, &mut ctx(&reg, &mut rng, &mut omens))
            .unwrap();
        // After Chaos, the T3 mod is removed and a new high-level mod is added.
        let any_t3 = item
            .prefixes
            .iter()
            .chain(item.suffixes.iter())
            .any(|m| m.mod_id.as_str().ends_with("_T3"));
        let any_t1 = item
            .prefixes
            .iter()
            .chain(item.suffixes.iter())
            .any(|m| m.mod_id.as_str().ends_with("_T1"));
        assert!(!any_t3, "Perfect Chaos should not leave a T3 mod");
        assert!(any_t1, "Perfect Chaos should add a T1");
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
        let reg = ModRegistry::from_mods(vec![
            mk_mod_lvl("HighReqLife", "Life", AffixType::Prefix, "Boots", 80),
            mk_mod_lvl("LowReqMana", "Mana", AffixType::Prefix, "Boots", 5),
            mk_mod_lvl("FreshMod", "FreshGroup", AffixType::Suffix, "Boots", 1),
        ]);
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
