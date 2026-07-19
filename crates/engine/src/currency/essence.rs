//! Essences (Lesser / Normal / Greater / Perfect / Corrupted).
//!
//! ## Behavior
//!
//! - **Lesser / Normal / Greater** essences apply to a **Magic** item:
//!   the item is promoted to Rare and gains the essence's specific mod
//!   (plus the existing magic mods are preserved, just like a Regal Orb
//!   but with a guaranteed mod).
//!
//! - **Perfect / Corrupted** essences apply to a **Rare** item: a random
//!   non-fractured mod is removed, then the essence's specific mod is
//!   added. Sinistral / Dextral Crystallisation force the *removal* to
//!   target only Prefix or only Suffix.
//!
//! ## Determinism
//!
//! The essence carries the exact `ModId` it adds (the data binding lives
//! in the bundle, populated by the poe2db pipeline pass once data is
//! integrated). For the M2.5 milestone, callers construct an essence with
//! a [`ModId`] they've looked up themselves; the engine just enforces the
//! mechanic.

use rand::Rng;

use crate::currency::{ApplyContext, ApplyOutcome, Currency};
use crate::error::{EngineError, EngineResult};
use crate::ids::{CurrencyId, ItemClassId, ModId};
use crate::item::{AffixType, Item, ModRoll, Rarity};
use crate::item_class::AttributePool;
use crate::mods::{ModDefinition, ModKind};

/// Maximum modifiers a Rare item may carry on each affix side (3 prefixes /
/// 3 suffixes in PoE2). Used to keep the remove-then-add path from
/// overflowing a side.
const MAX_AFFIXES_PER_SIDE: usize = 3;

/// Quality tier of an essence — controls its apply behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EssenceQuality {
    Lesser,
    Normal,
    Greater,
    /// Operates on Rare; removes one random mod and adds the essence's mod.
    Perfect,
    /// Vaal-corrupted Perfect essence; same removal+add semantics but the
    /// added mod comes from the corrupted pool. Operates on Rare even when
    /// the item is already corrupted.
    Corrupted,
}

impl EssenceQuality {
    /// Does this essence variant promote Magic to Rare (the regal-style
    /// path)? True for Lesser/Normal/Greater.
    pub fn is_promoting(self) -> bool {
        matches!(self, Self::Lesser | Self::Normal | Self::Greater)
    }
    /// Does this essence variant remove + add on a Rare? True for
    /// Perfect/Corrupted.
    pub fn is_remove_add(self) -> bool {
        matches!(self, Self::Perfect | Self::Corrupted)
    }
}

/// One per-class target binding for an essence. The same essence grants a
/// *different* concrete mod per item class (poe2db/CoE: e.g. Essence of
/// Alacrity — Wand `IncreasedCastSpeed3`, Staff `IncreasedCastSpeedTwoHand3`),
/// and on armour the granted defence mod differs per attribute pool
/// (STR armour% vs INT energy-shield%).
#[derive(Debug, Clone)]
pub struct EssenceTarget {
    pub class: ItemClassId,
    /// `None` = any attribute pool of the class. `Some(pool)` entries take
    /// precedence over `None` entries for matching bases.
    pub attribute_pool: Option<AttributePool>,
    pub mod_id: ModId,
}

/// One essence — characterized by quality and the specific mod it grants.
///
/// In production, the full Essence catalogue (19 types × 4 tiers + Corrupted)
/// ships in the data bundle; this engine type is the currency-trait wrapper
/// that consumes a bundle entry at apply time.
#[derive(Debug, Clone)]
pub struct Essence {
    /// Display id, e.g. `"PerfectEssenceOfSeeking"` or
    /// `"GreaterEssenceOfBattle"`.
    pub id: CurrencyId,
    /// Display name, e.g. `"Perfect Essence of Seeking"`.
    pub display_name: &'static str,
    /// Quality tier (drives apply semantics).
    pub quality: EssenceQuality,
    /// The mod this essence guarantees to add when `class_targets` is empty
    /// (legacy single-target shape, kept for fixtures). The mod's
    /// `affix_type` dictates which slot it occupies; Crystallisation's
    /// affix-only filter is independent of this and applies to the *removal*
    /// step on Perfect/Corrupted essences.
    pub target_mod: ModId,
    /// Class-specific targets (the data-driven production shape). When
    /// non-empty, the item's class (and attribute pool) selects the granted
    /// mod, and a class with no entry **cannot receive this essence** —
    /// weapon essences must not land on jewellery.
    pub class_targets: Vec<EssenceTarget>,
}

impl Essence {
    pub fn new(
        id: impl Into<CurrencyId>,
        display_name: &'static str,
        quality: EssenceQuality,
        target_mod: impl Into<ModId>,
    ) -> Self {
        Self {
            id: id.into(),
            display_name,
            quality,
            target_mod: target_mod.into(),
            class_targets: Vec::new(),
        }
    }

    /// Construct a class-targeted essence (the data-driven production
    /// shape). `targets` must be non-empty; the first entry doubles as the
    /// legacy `target_mod` fallback for diagnostics.
    pub fn with_class_targets(
        id: impl Into<CurrencyId>,
        display_name: &'static str,
        quality: EssenceQuality,
        targets: Vec<EssenceTarget>,
    ) -> Self {
        let fallback = targets
            .first()
            .map_or_else(|| ModId::from("UnboundEssenceTarget"), |t| t.mod_id.clone());
        Self {
            id: id.into(),
            display_name,
            quality,
            target_mod: fallback,
            class_targets: targets,
        }
    }

    /// Resolve the granted mod for an item's class + attribute pool.
    /// `None` when this essence has class targets but none match (the
    /// essence is illegal on the class).
    pub fn resolve_target(&self, class: &ItemClassId, pool: AttributePool) -> Option<&ModId> {
        if self.class_targets.is_empty() {
            return Some(&self.target_mod);
        }
        // Pool-specific entry wins over the class-generic entry.
        self.class_targets
            .iter()
            .find(|t| &t.class == class && t.attribute_pool == Some(pool))
            .or_else(|| {
                self.class_targets
                    .iter()
                    .find(|t| &t.class == class && t.attribute_pool.is_none())
            })
            .map(|t| &t.mod_id)
    }
}

impl Currency for Essence {
    fn id(&self) -> &CurrencyId {
        &self.id
    }

    fn name(&self) -> &'static str {
        self.display_name
    }

    fn valid_rarities(&self) -> crate::currency::RaritySet {
        match self.quality {
            // Lesser / Normal / Greater all "Upgrade a Magic item to a Rare
            // item, adding a guaranteed modifier" (PoE2 0.3 essence rework,
            // stable in 0.5; per the wiki + poe2db). They therefore apply to
            // MAGIC items — matching `apply_promoting`, which requires Magic.
            // (Previously Lesser/Normal incorrectly declared NORMAL, which made
            // their success path unreachable: the rarity gate allowed only
            // Normal items but `apply_promoting` then rejected the non-Magic.)
            EssenceQuality::Lesser | EssenceQuality::Normal | EssenceQuality::Greater => {
                crate::currency::RaritySet::MAGIC
            }
            EssenceQuality::Perfect | EssenceQuality::Corrupted => crate::currency::RaritySet::RARE,
        }
    }

    /// Pre-flight class gate (mirrors `Bone`/`Catalyst`): rarity + mirrored
    /// via the default semantics, plus a best-effort class-target check when
    /// the item's class is resolvable without a registry (legacy PascalCase
    /// placeholders). Real bundle ids resolve at apply time via
    /// `ctx.base_registry` — `apply` re-checks with full fidelity.
    fn can_apply_to(&self, item: &Item) -> Result<(), crate::currency::CannotApply> {
        let valid = self.valid_rarities();
        if !valid.contains(item.rarity) {
            return Err(crate::currency::CannotApply::WrongRarity {
                item_rarity: item.rarity,
                expected: valid,
            });
        }
        if item.mirrored {
            return Err(crate::currency::CannotApply::Mirrored);
        }
        if !self.class_targets.is_empty() {
            if let Some(class) = crate::base_registry::EMPTY.resolve_item_class_opt(item) {
                if !self.class_targets.iter().any(|t| t.class == class) {
                    return Err(crate::currency::CannotApply::Other(
                        "essence has no modifier for this item class",
                    ));
                }
            }
        }
        Ok(())
    }

    fn apply(&self, item: &mut Item, ctx: &mut ApplyContext<'_>) -> EngineResult<ApplyOutcome> {
        // Sanctified items reject all essences.
        if item.sanctified {
            return Err(EngineError::ItemSanctified);
        }
        // Mirrored items reject everything.
        if item.mirrored {
            return Err(EngineError::InvalidApplication(
                "Essence cannot be applied to a mirrored item".into(),
            ));
        }
        // Corrupted items: only Corrupted essences are allowed.
        if item.corrupted && self.quality != EssenceQuality::Corrupted {
            return Err(EngineError::ItemCorrupted);
        }

        // Registry-backed class gate: the essence grants a class-specific
        // mod; a class with no target rejects the essence (weapon essences
        // must not land on jewellery).
        let class = ctx.base_registry.resolve_item_class(item);
        let pool = ctx
            .base_registry
            .get(&item.base)
            .map_or(AttributePool::None, |b| b.attribute_pool);
        let target_id = self.resolve_target(&class, pool).ok_or_else(|| {
            EngineError::InvalidApplication(format!(
                "{}: no modifier for item class {}",
                self.display_name,
                class.as_str()
            ))
        })?;

        let target_def = ctx.registry.get(target_id).ok_or_else(|| {
            EngineError::Data(format!("Essence: target mod `{target_id}` not in registry"))
        })?;

        if self.quality.is_promoting() {
            apply_promoting(self, item, ctx, target_def)
        } else {
            apply_remove_add(self, item, ctx, target_def)
        }
    }
}

/// Lesser/Normal/Greater path: Magic → Rare with the specific mod added,
/// existing mods preserved. Like a Regal Orb but with a guaranteed mod.
fn apply_promoting(
    essence: &Essence,
    item: &mut Item,
    ctx: &mut ApplyContext<'_>,
    target_def: &ModDefinition,
) -> EngineResult<ApplyOutcome> {
    if item.rarity != Rarity::Magic {
        return Err(EngineError::InvalidApplication(format!(
            "{}: requires a Magic-rarity item",
            essence.display_name
        )));
    }
    // Refuse if the slot of the target mod's affix type is already full.
    let slot_full = match target_def.affix_type {
        AffixType::Prefix => item.prefixes.len() >= 3,
        AffixType::Suffix => item.suffixes.len() >= 3,
        _ => true,
    };
    if slot_full {
        return Err(EngineError::AffixSlotFull {
            affix_type: "Essence's target affix slot is full",
        });
    }
    // Refuse if the target mod's group is already occupied.
    if let Some(g) = ctx.registry.group_of(&target_def.id) {
        let occupied = item
            .prefixes
            .iter()
            .chain(item.suffixes.iter())
            .any(|m| ctx.registry.group_of(&m.mod_id) == Some(g));
        if occupied {
            return Err(EngineError::ModGroupExclusive(format!(
                "Essence's target mod-group `{g}` is already on the item"
            )));
        }
    }

    item.rarity = Rarity::Rare;
    push_essence_roll(item, essence, target_def, ctx);
    Ok(())
}

/// Perfect/Corrupted path: remove one random non-fractured mod, then add
/// the essence's specific mod. Sinistral/Dextral Crystallisation constrain
/// which affix the removal targets.
fn apply_remove_add(
    essence: &Essence,
    item: &mut Item,
    ctx: &mut ApplyContext<'_>,
    target_def: &ModDefinition,
) -> EngineResult<ApplyOutcome> {
    if item.rarity != Rarity::Rare {
        return Err(EngineError::InvalidApplication(format!(
            "{}: requires a Rare-rarity item",
            essence.display_name
        )));
    }

    let crystallisation = ctx.omens.consume_affix_only(ctx.patch);

    // The essence's guaranteed mod always lands in `target_def.affix_type`.
    // On a Rare that already fills that side (3 mods), the removal MUST free a
    // slot on the *same* side, otherwise the add overflows to a 4th
    // prefix/suffix — an illegal item state. Constrain the removal to keep the
    // result legal, composing with Sinistral/Dextral Crystallisation.
    let target_affix = target_def.affix_type;
    let target_full = match target_affix {
        AffixType::Prefix => item.prefixes.len() >= MAX_AFFIXES_PER_SIDE,
        AffixType::Suffix => item.suffixes.len() >= MAX_AFFIXES_PER_SIDE,
        _ => false,
    };
    let affix_filter = match (crystallisation, target_full) {
        // Crystallisation forces the opposite side while the target side is
        // full: the removal cannot open a slot for the new mod.
        (Some(forced), true) if forced != target_affix => {
            return Err(EngineError::AffixSlotFull {
                affix_type:
                    "Essence: target affix slot is full and Crystallisation forces the other side",
            });
        }
        (Some(forced), _) => Some(forced),
        // No Crystallisation but the target side is full → remove from the
        // target side so the new mod has room.
        (None, true) => Some(target_affix),
        (None, false) => None,
    };

    // Build removable list per filter.
    let mut removables: smallvec::SmallVec<[(AffixType, usize); 8]> = smallvec::SmallVec::new();
    if affix_filter != Some(AffixType::Suffix) {
        for (i, m) in item.prefixes.iter().enumerate() {
            if !m.is_fractured {
                removables.push((AffixType::Prefix, i));
            }
        }
    }
    if affix_filter != Some(AffixType::Prefix) {
        for (i, m) in item.suffixes.iter().enumerate() {
            if !m.is_fractured {
                removables.push((AffixType::Suffix, i));
            }
        }
    }
    if removables.is_empty() {
        return Err(EngineError::InvalidApplication(format!(
            "{}: no eligible mod to remove given Crystallisation/fractures",
            essence.display_name
        )));
    }

    // Mod-group exclusivity: target_def's group must not collide with a
    // surviving mod. If it would, refuse upfront. (Real game behavior:
    // the engine already removed a mod, so the slot is open. We mirror
    // that by removing FIRST then checking.)
    let pick = ctx.rng.gen_range(0..removables.len());
    let (rm_affix, rm_idx) = removables[pick];
    let _removed = match rm_affix {
        AffixType::Prefix => item.prefixes.remove(rm_idx),
        AffixType::Suffix => item.suffixes.remove(rm_idx),
        _ => unreachable!(),
    };

    // Now check group exclusivity against survivors.
    if let Some(g) = ctx.registry.group_of(&target_def.id) {
        let occupied = item
            .prefixes
            .iter()
            .chain(item.suffixes.iter())
            .any(|m| ctx.registry.group_of(&m.mod_id) == Some(g));
        if occupied {
            return Err(EngineError::ModGroupExclusive(format!(
                "Essence's target mod-group `{g}` already on item after removal"
            )));
        }
    }

    push_essence_roll(item, essence, target_def, ctx);
    Ok(())
}

fn push_essence_roll(
    item: &mut Item,
    essence: &Essence,
    target_def: &ModDefinition,
    ctx: &mut ApplyContext<'_>,
) {
    let kind = match essence.quality {
        EssenceQuality::Corrupted => ModKind::Corrupted,
        _ => ModKind::Explicit,
    };
    let values = target_def
        .stats
        .iter()
        .map(|s| s.roll(ctx.rng.gen::<f64>()))
        .collect();
    let roll = ModRoll {
        mod_id: target_def.id.clone(),
        affix_type: target_def.affix_type,
        kind,
        values,
        is_fractured: false,
    };
    match target_def.affix_type {
        AffixType::Prefix => item.prefixes.push(roll),
        AffixType::Suffix => item.suffixes.push(roll),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use rand::SeedableRng;
    use rand_xoshiro::Xoshiro256PlusPlus;
    use smallvec::smallvec;

    use super::*;
    use crate::ids::{ItemClassId, ModGroupId, StatId, TagId};
    use crate::item::QualityKind;
    use crate::mods::{ModDomain, ModFlags, ModGroup, ModStat, SpawnWeight};
    use crate::omen::{Omen, OmenSet};
    use crate::patch::{PatchRange, PatchVersion};
    use crate::registry::ModRegistry;

    fn mk_target_mod(id: &str, group: &str, affix: AffixType) -> ModDefinition {
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
                tag: TagId::from("BodyArmour"),
                weight: 1
            }],
            stats: smallvec![ModStat {
                stat_id: StatId::from("test_stat"),
                min: 40.0,
                max: 50.0,
            }],
            required_level: 1,
            tier: None,
            allowed_item_classes: smallvec![ItemClassId::from("BodyArmour")],
            patch_range: PatchRange::ALL,
            flags: ModFlags::ESSENCE_ONLY,
            text_template: None,
        }
    }

    fn fixture_armour() -> Item {
        Item {
            base: ItemClassId::from("BodyArmour").as_str().into(),
            ilvl: 82,
            rarity: Rarity::Magic,
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

    #[test]
    fn greater_essence_promotes_magic_to_rare_with_target_mod() {
        let target = mk_target_mod("EssMod_Life_Greater", "Life", AffixType::Prefix);
        let reg = ModRegistry::from_mods(vec![target], vec![]);
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0x1);
        let mut omens = OmenSet::new();

        let mut item = fixture_armour();
        // Existing Magic mod (suffix) should be preserved.
        item.suffixes.push(ModRoll {
            mod_id: ModId::from("ExistingSuffix"),
            affix_type: AffixType::Suffix,
            kind: ModKind::Explicit,
            values: smallvec![],
            is_fractured: false,
        });

        let ess = Essence::new(
            "GreaterEssenceOfBody",
            "Greater Essence of Body",
            EssenceQuality::Greater,
            "EssMod_Life_Greater",
        );

        ess.apply(
            &mut item,
            &mut ApplyContext::new_without_bases(
                &reg,
                &mut rng,
                PatchVersion::PATCH_0_4_0,
                &mut omens,
            ),
        )
        .unwrap();

        assert_eq!(item.rarity, Rarity::Rare);
        assert_eq!(item.prefixes.len(), 1);
        assert_eq!(item.prefixes[0].mod_id, ModId::from("EssMod_Life_Greater"));
        assert_eq!(item.suffixes.len(), 1, "existing suffix preserved");
    }

    #[test]
    fn perfect_essence_with_dextral_crystallisation_removes_only_suffix() {
        // The user's worked example: Perfect Essence of Seeking +
        // Omen of Dextral Crystallisation = removes a suffix, adds Seeking
        // (Body Armour: reduced Critical Damage Bonus).
        let target = mk_target_mod(
            "EssMod_Seeking_Perfect",
            "ReducedCritDmg",
            AffixType::Suffix,
        );
        let life = mk_target_mod("LifeMod", "Life", AffixType::Prefix);
        let res = mk_target_mod("FireResMod", "FireRes", AffixType::Suffix);
        let reg = ModRegistry::from_mods(vec![target, life, res], vec![]);
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0x2);
        let mut omens = OmenSet::new();
        omens.push(Omen::dextral_crystallisation());

        let mut item = fixture_armour();
        item.rarity = Rarity::Rare;
        // 2 prefixes + 1 suffix
        item.prefixes.push(ModRoll {
            mod_id: ModId::from("LifeMod"),
            affix_type: AffixType::Prefix,
            kind: ModKind::Explicit,
            values: smallvec![],
            is_fractured: false,
        });
        item.prefixes.push(ModRoll {
            mod_id: ModId::from("ESMod"),
            affix_type: AffixType::Prefix,
            kind: ModKind::Explicit,
            values: smallvec![],
            is_fractured: false,
        });
        item.suffixes.push(ModRoll {
            mod_id: ModId::from("FireResMod"),
            affix_type: AffixType::Suffix,
            kind: ModKind::Explicit,
            values: smallvec![],
            is_fractured: false,
        });

        let ess = Essence::new(
            "PerfectEssenceOfSeeking",
            "Perfect Essence of Seeking",
            EssenceQuality::Perfect,
            "EssMod_Seeking_Perfect",
        );

        ess.apply(
            &mut item,
            &mut ApplyContext::new_without_bases(
                &reg,
                &mut rng,
                PatchVersion::PATCH_0_4_0,
                &mut omens,
            ),
        )
        .unwrap();

        // Both prefixes survive (Crystallisation forced suffix removal).
        assert_eq!(item.prefixes.len(), 2);
        // The original suffix was removed; the essence-added suffix replaces it.
        assert_eq!(item.suffixes.len(), 1);
        assert_eq!(
            item.suffixes[0].mod_id,
            ModId::from("EssMod_Seeking_Perfect")
        );
    }

    #[test]
    fn perfect_essence_rejects_when_no_removable_mods() {
        // Crystallisation forces suffix removal but there are no suffixes.
        let target = mk_target_mod("EssMod_X", "GA", AffixType::Suffix);
        let reg = ModRegistry::from_mods(vec![target], vec![]);
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0x3);
        let mut omens = OmenSet::new();
        omens.push(Omen::dextral_crystallisation());

        let mut item = fixture_armour();
        item.rarity = Rarity::Rare;
        item.prefixes.push(ModRoll {
            mod_id: ModId::from("LifeMod"),
            affix_type: AffixType::Prefix,
            kind: ModKind::Explicit,
            values: smallvec![],
            is_fractured: false,
        });

        let ess = Essence::new(
            "PerfectEssenceOfX",
            "Perfect X",
            EssenceQuality::Perfect,
            "EssMod_X",
        );
        let r = ess.apply(
            &mut item,
            &mut ApplyContext::new_without_bases(
                &reg,
                &mut rng,
                PatchVersion::PATCH_0_4_0,
                &mut omens,
            ),
        );
        assert!(matches!(r, Err(EngineError::InvalidApplication(_))));
    }

    #[test]
    fn essence_rejects_corrupted_unless_corrupted_essence() {
        let target = mk_target_mod("EssMod_X", "GA", AffixType::Suffix);
        let reg = ModRegistry::from_mods(vec![target], vec![]);
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0x4);
        let mut omens = OmenSet::new();

        let mut item = fixture_armour();
        item.rarity = Rarity::Rare;
        item.corrupted = true;

        let perfect = Essence::new("X", "X", EssenceQuality::Perfect, "EssMod_X");
        let r = perfect.apply(
            &mut item,
            &mut ApplyContext::new_without_bases(
                &reg,
                &mut rng,
                PatchVersion::PATCH_0_4_0,
                &mut omens,
            ),
        );
        assert!(matches!(r, Err(EngineError::ItemCorrupted)));

        // Corrupted essence accepts corrupted item (assuming a removable mod
        // exists; we add one).
        item.suffixes.push(ModRoll {
            mod_id: ModId::from("OldSuffix"),
            affix_type: AffixType::Suffix,
            kind: ModKind::Explicit,
            values: smallvec![],
            is_fractured: false,
        });
        let corrupted = Essence::new("X", "X", EssenceQuality::Corrupted, "EssMod_X");
        corrupted
            .apply(
                &mut item,
                &mut ApplyContext::new_without_bases(
                    &reg,
                    &mut rng,
                    PatchVersion::PATCH_0_4_0,
                    &mut omens,
                ),
            )
            .unwrap();
        assert_eq!(item.suffixes.len(), 1);
        assert_eq!(item.suffixes[0].kind, ModKind::Corrupted);
    }

    /// Class-targeted essences must reject classes they carry no mod for —
    /// a weapon essence cannot land on jewellery (M14 audit regression).
    #[test]
    fn class_targeted_essence_rejects_unlisted_class() {
        let target = mk_target_mod("EssMod_AttackSpeed", "IAS", AffixType::Suffix);
        let reg = ModRegistry::from_mods(vec![target], vec![]);
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0x5);
        let mut omens = OmenSet::new();

        let ess = Essence::with_class_targets(
            "EssenceOfHaste",
            "Essence of Haste",
            EssenceQuality::Greater,
            vec![EssenceTarget {
                class: ItemClassId::from("Wand"),
                attribute_pool: None,
                mod_id: ModId::from("EssMod_AttackSpeed"),
            }],
        );

        // can_apply_to: legacy PascalCase base resolves via the EMPTY
        // registry fallback → class gate fires pre-flight.
        let mut item = fixture_armour(); // base "BodyArmour", Magic
        assert!(ess.can_apply_to(&item).is_err(), "pre-flight class gate");

        // apply: registry-backed gate fires even if pre-flight is bypassed.
        let r = ess.apply(
            &mut item,
            &mut ApplyContext::new_without_bases(
                &reg,
                &mut rng,
                PatchVersion::PATCH_0_5_0,
                &mut omens,
            ),
        );
        assert!(
            matches!(r, Err(EngineError::InvalidApplication(ref m)) if m.contains("no modifier for item class")),
            "apply-time class gate; got {r:?}"
        );
        assert_eq!(item.prefixes.len() + item.suffixes.len(), 0);
    }

    /// Attribute-pool-specific targets beat the class-generic entry; a
    /// class with no entry resolves to `None`.
    #[test]
    fn resolve_target_prefers_pool_specific_entry() {
        use crate::item_class::AttributePool;
        let ess = Essence::with_class_targets(
            "EssenceOfEnhancement",
            "Essence of Enhancement",
            EssenceQuality::Greater,
            vec![
                EssenceTarget {
                    class: ItemClassId::from("BodyArmour"),
                    attribute_pool: None,
                    mod_id: ModId::from("DefencesGeneric"),
                },
                EssenceTarget {
                    class: ItemClassId::from("BodyArmour"),
                    attribute_pool: Some(AttributePool::Int),
                    mod_id: ModId::from("EnergyShieldPercent"),
                },
            ],
        );
        let body = ItemClassId::from("BodyArmour");
        assert_eq!(
            ess.resolve_target(&body, AttributePool::Int),
            Some(&ModId::from("EnergyShieldPercent")),
            "pool-specific entry wins"
        );
        assert_eq!(
            ess.resolve_target(&body, AttributePool::Str),
            Some(&ModId::from("DefencesGeneric")),
            "generic entry covers other pools"
        );
        assert_eq!(
            ess.resolve_target(&ItemClassId::from("Ring"), AttributePool::None),
            None,
            "unlisted class rejects"
        );
    }
}
