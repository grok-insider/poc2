//! Shared sampling / removal kernel for the currency modules.
//!
//! Every "add a mod" orb (basic orbs, Greater / Perfect variants, Vaal
//! brick replacement) samples through [`sample_eligible_mod`] or the
//! lower-level [`pick_weighted`] draw; removal-side orbs (Annul / Chaos)
//! use [`collect_removable_filtered`] / [`remove_mod_at`]. Everything here
//! is `pub(crate)`: the kernel is an implementation detail of the
//! `currency` modules, not engine API.

use rand::Rng;
use smallvec::SmallVec;

use crate::currency::ApplyContext;
use crate::ids::ItemClassId;
use crate::item::{AffixType, Item, ModRoll};
use crate::mods::{ModDefinition, ModFlags, ModKind};
use crate::registry::{ModIndex, ModRegistry};

/// Mod-flag exclusion mask for basic-orb sampling (M14.3).
///
/// Trans / Aug / Regal / Alch / Exalt / Chaos (every tier) cannot roll mods
/// flagged as essence-, desecrated-, or corrupted-only. The bundle tags
/// these mods at pipeline time; this constant is the runtime gate that
/// honors those tags during [`sample_eligible_mod`].
///
/// Per `docs/81-engine-training-and-rule-encoding-plan.md` §4.3 Tier 1.3
/// and rule R232.
pub(crate) const BASIC_ORB_EXCLUDES: ModFlags = ModFlags::ESSENCE_ONLY
    .union(ModFlags::DESECRATED_ONLY)
    .union(ModFlags::CORRUPTED_ONLY);

/// Compute the class of an item from its base.
///
/// Delegates to [`crate::base_registry::BaseRegistry::resolve_item_class`],
/// which prefers the registry when `item.base` is a real bundle id and
/// falls back to `ItemClassId::from(item.base.as_str())` for legacy
/// fixtures (see the method docs for the v3-transitional rationale).
pub(crate) fn class_for_item(
    item: &Item,
    base_registry: &crate::base_registry::BaseRegistry,
) -> ItemClassId {
    base_registry.resolve_item_class(item)
}

/// Sample a mod from the eligible set with CoE-derived numerical weights.
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
/// 7. **(M14.3)** The mod's `flags` does not intersect `excludes`. Basic
///    orbs pass [`BASIC_ORB_EXCLUDES`] so essence-only / desecrated-only /
///    corrupted-only mods never leak into Trans/Aug/Regal/Alch/Exalt/Chaos
///    pools. Other call sites (essence apply, bone reveal, Vaal corruption)
///    pass their own masks per `docs/34-heuristics-rulebook.md`.
///
/// Weights are resolved via [`ModRegistry::weight_for`], which consults
/// `bundle.weights` (CoE numerical weights) and falls back to RePoE-fork
/// eligibility flags. Mods with weight `<= 0.0` are excluded; weighted
/// random selection over the rest uses the f64 cumulative-distribution
/// trick.
#[allow(clippy::too_many_arguments)] // 8 args: 7 prior + the M14.3 `excludes` mask.
pub(crate) fn sample_eligible_mod<'r>(
    registry: &'r ModRegistry,
    base_registry: &crate::base_registry::BaseRegistry,
    item: &Item,
    affix: AffixType,
    rng: &mut dyn rand::RngCore,
    patch: crate::patch::PatchVersion,
    min_required_level: u32,
    excludes: ModFlags,
) -> Option<&'r ModDefinition> {
    let class = class_for_item(item, base_registry);
    // Base tags drive tag-intersection weighting (leftmost-tag-wins) when no
    // numeric CoE weight covers a mod. Empty when no BaseRegistry is threaded
    // (v2-transitional fixtures) — the registry then uses the binary
    // eligibility stub, preserving prior behavior.
    let base_tags = base_registry.tags_of(&item.base).to_vec();
    let candidates = registry.for_class_affix(&class, affix);

    // Build the list of (mod, weight) tuples after filtering.
    // SmallVec to avoid heap allocation in the small-eligibility-set common case.
    let mut eligible: SmallVec<[(ModIndex, f64); 64]> = SmallVec::new();

    let occupied_groups = collect_occupied_groups(registry, item);

    // Track which mod-groups were dropped *solely* because every rollable
    // tier was below `min_required_level`. These are candidates for the
    // "keep ≥1 tier per mod-type" exception (Min Modifier Level wording:
    // "at least one tier of each mod type will always be eligible,
    // respecting item level"). For each such group we remember the highest
    // `required_level` tier that is still `<= ilvl` and otherwise eligible,
    // and add exactly that one back if the group contributed nothing above
    // the floor. Keyed by group id → (best ModIndex, its required_level).
    let mut floor_exception: SmallVec<[(crate::ids::ModGroupId, ModIndex, u32); 16]> =
        SmallVec::new();

    for &idx in candidates {
        let Some(m) = registry.at(idx) else { continue };
        if m.kind != ModKind::Explicit {
            continue;
        }
        // ilvl ceiling, patch, group-occupancy and flag gates apply to every
        // tier regardless of the Min-Modifier-Level floor.
        if m.required_level > item.ilvl {
            continue;
        }
        if !m.patch_range.contains(patch) {
            continue;
        }
        if occupied_groups.contains(&m.mod_group.0) {
            continue;
        }
        if m.flags.intersects(excludes) {
            continue;
        }
        // Inclusive higher-tier weight (PoE2 mechanic): a tier inherits the
        // spawn weight of the same-group higher tiers rollable at this ilvl.
        let w = inclusive_weight_for_item(m, item, registry, &class, &base_tags);
        if w <= 0.0 {
            continue;
        }
        if m.required_level < min_required_level {
            // Below the variant's Min Modifier Level floor. Not directly
            // eligible, but record it as the group's fallback in case the
            // whole group is sub-floor (keep-≥1-tier exception). Keep the
            // strongest (highest required_level) such tier.
            match floor_exception
                .iter_mut()
                .find(|(g, _, _)| *g == m.mod_group.0)
            {
                Some((_, best_idx, best_rl)) => {
                    if m.required_level > *best_rl {
                        *best_idx = idx;
                        *best_rl = m.required_level;
                    }
                }
                None => floor_exception.push((m.mod_group.0.clone(), idx, m.required_level)),
            }
            continue;
        }
        eligible.push((idx, w));
    }

    // Apply the keep-≥1-tier exception: for any group that has a sub-floor
    // fallback recorded AND contributed nothing at/above the floor, add its
    // strongest sub-floor tier back into the pool. This guarantees no entire
    // mod-type is excluded by a Min-Modifier-Level floor.
    for (group, idx, _) in &floor_exception {
        let group_present_above_floor = eligible.iter().any(|(eidx, _)| {
            registry
                .at(*eidx)
                .is_some_and(|em| &em.mod_group.0 == group)
        });
        if group_present_above_floor {
            continue;
        }
        if let Some(m) = registry.at(*idx) {
            let w = inclusive_weight_for_item(m, item, registry, &class, &base_tags);
            if w > 0.0 {
                eligible.push((*idx, w));
            }
        }
    }

    pick_weighted(registry, &eligible, rng)
}

/// One weighted draw over `(mod, weight)` candidates using the f64
/// cumulative-distribution trick. Shared by [`sample_eligible_mod`] and the
/// Vaal corrupted-mod samplers so all weighted selection behaves uniformly.
///
/// Returns `None` for an empty candidate list or a non-positive total
/// weight. Defensive: floating-point summation may round the cumulative
/// just under `total` so the loop falls through — the last candidate wins.
pub(crate) fn pick_weighted<'r>(
    registry: &'r ModRegistry,
    eligible: &[(ModIndex, f64)],
    rng: &mut dyn rand::RngCore,
) -> Option<&'r ModDefinition> {
    if eligible.is_empty() {
        return None;
    }
    let total: f64 = eligible.iter().map(|(_, w)| *w).sum();
    if total <= 0.0 {
        return None;
    }
    let mut pick = rng.gen_range(0.0..total);
    for (idx, w) in eligible {
        if pick < *w {
            return registry.at(*idx);
        }
        pick -= *w;
    }
    eligible.last().and_then(|(i, _)| registry.at(*i))
}

/// Resolve the **inclusive** higher-tier spawn weight of a mod on this item.
///
/// Delegates to [`ModRegistry::inclusive_weight_for`], which sums the
/// per-tier `weight_for` over the same-group, same-affix tiers that are
/// rollable at the item's ilvl and are the same-or-stronger tier than `m`.
/// This is the PoE2 mechanic where a lower tier inherits the spawn weight of
/// higher tiers that can roll at the current item level (`m_t0` is
/// ilvl-dependent).
fn inclusive_weight_for_item(
    m: &ModDefinition,
    item: &Item,
    registry: &ModRegistry,
    class: &ItemClassId,
    base_tags: &[crate::ids::TagId],
) -> f64 {
    registry.inclusive_weight_for_on_base(m, &item.base, item.ilvl, class, base_tags)
}

/// Set of mod-groups already occupied on the item (any affix slot).
pub(crate) fn collect_occupied_groups(
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
pub(crate) fn collect_removable_filtered(
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
pub(crate) fn pick_lowest_mod_level(
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
pub(crate) fn remove_mod_at(item: &mut Item, affix: AffixType, idx: usize) -> Option<ModRoll> {
    match affix {
        AffixType::Prefix if idx < item.prefixes.len() => Some(item.prefixes.remove(idx)),
        AffixType::Suffix if idx < item.suffixes.len() => Some(item.suffixes.remove(idx)),
        _ => None,
    }
}

/// Roll a value `t ∈ [0,1]` for each stat in the mod, then linear-interpolate.
pub(crate) fn roll_values(m: &ModDefinition, rng: &mut dyn rand::RngCore) -> SmallVec<[f64; 4]> {
    m.stats
        .iter()
        .map(|s| {
            let t = rng.gen::<f64>();
            s.roll(t)
        })
        .collect()
}

/// Build a `ModRoll` from a sampled `ModDefinition`, rolling values.
pub(crate) fn roll_mod(m: &ModDefinition, rng: &mut dyn rand::RngCore) -> ModRoll {
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
/// no slot is opened (caller errors with
/// [`crate::error::EngineError::AffixSlotFull`]).
pub(crate) fn pick_open_affix(
    item: &Item,
    rng: &mut dyn rand::RngCore,
    max_slots: u8,
) -> Option<AffixType> {
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
pub(crate) fn pick_open_affix_with_omen(
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
pub(crate) fn affix_label(a: AffixType) -> &'static str {
    match a {
        AffixType::Prefix => "prefix",
        AffixType::Suffix => "suffix",
        AffixType::Implicit => "implicit",
        AffixType::Enchantment => "enchantment",
    }
}

/// Add a rolled mod to the appropriate affix slot.
pub(crate) fn push_mod(item: &mut Item, roll: ModRoll) {
    match roll.affix_type {
        AffixType::Prefix => item.prefixes.push(roll),
        AffixType::Suffix => item.suffixes.push(roll),
        // Implicit / Enchantment paths never come through here.
        _ => {}
    }
}

/// Shared item / registry / context fixtures for the currency test modules
/// (`basic`, `variants`, `vaal`). Test-only; lives with the kernel so the
/// split orb modules exercise identical fixture state.
#[cfg(test)]
pub(crate) mod test_fixtures {
    use rand_xoshiro::Xoshiro256PlusPlus;
    use smallvec::smallvec;

    use crate::currency::ApplyContext;
    use crate::ids::{ItemClassId, ModGroupId, ModId, TagId};
    use crate::item::{AffixType, Item, QualityKind, Rarity};
    use crate::mods::{ModDefinition, ModDomain, ModFlags, ModGroup, ModKind, SpawnWeight};
    use crate::patch::{PatchRange, PatchVersion};
    use crate::registry::ModRegistry;

    pub(crate) fn mk_mod(id: &str, group: &str, affix: AffixType, class: &str) -> ModDefinition {
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
            tier: None,
            allowed_item_classes: smallvec![ItemClassId::from(class)],
            patch_range: PatchRange::ALL,
            flags: ModFlags::empty(),
            text_template: None,
        }
    }

    pub(crate) fn mk_mod_lvl(
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
            tier: None,
            allowed_item_classes: smallvec![ItemClassId::from(class)],
            patch_range: PatchRange::ALL,
            flags: ModFlags::empty(),
            text_template: None,
        }
    }

    pub(crate) fn fixture_normal_boots() -> Item {
        Item {
            // Per the placeholder convention in
            // `BaseRegistry::resolve_item_class`, we use the class id as the
            // base id in tests until BaseRegistry lands.
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

    pub(crate) fn fixture_registry() -> ModRegistry {
        ModRegistry::from_mods(
            vec![
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
            ],
            vec![],
        )
    }

    pub(crate) fn fixture_tiered_registry() -> ModRegistry {
        // Multiple prefix and suffix groups, each with mods at varied
        // required_level so we can demonstrate min-mod-level filtering
        // regardless of which affix the orb picks.
        ModRegistry::from_mods(
            vec![
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
            ],
            vec![],
        )
    }

    pub(crate) fn ctx<'a>(
        registry: &'a ModRegistry,
        rng: &'a mut Xoshiro256PlusPlus,
        omens: &'a mut crate::omen::OmenSet,
    ) -> ApplyContext<'a> {
        ApplyContext::new_without_bases(registry, rng, PatchVersion::PATCH_0_4_0, omens)
    }
}

#[cfg(test)]
mod tests {
    use rand::SeedableRng;
    use rand_xoshiro::Xoshiro256PlusPlus;

    use super::test_fixtures::fixture_registry;
    use super::*;

    #[test]
    fn pick_weighted_returns_none_for_empty_or_zero_mass() {
        let reg = fixture_registry();
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(1);
        assert!(pick_weighted(&reg, &[], &mut rng).is_none());

        let idx = reg.for_class_affix(&ItemClassId::from("Boots"), AffixType::Prefix)[0];
        assert!(pick_weighted(&reg, &[(idx, 0.0)], &mut rng).is_none());
    }

    #[test]
    fn pick_weighted_returns_sole_positive_candidate() {
        let reg = fixture_registry();
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(2);
        let idx = reg.for_class_affix(&ItemClassId::from("Boots"), AffixType::Prefix)[0];
        let picked = pick_weighted(&reg, &[(idx, 1.0)], &mut rng).unwrap();
        assert_eq!(picked.id, reg.at(idx).unwrap().id);
    }
}
