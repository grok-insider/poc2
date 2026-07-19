//! Verisium Alloys (PoE2 0.5 "Return of the Ancients").
//!
//! ## Mechanic
//!
//! Per the 0.5 patch notes: the 13 Alloy currency items "add various new
//! crafted modifiers to items by replacing an existing modifier, similar to
//! Perfect Essences." So an Alloy:
//!
//! - applies to a **Rare** item,
//! - removes one random non-fractured modifier (Sinistral / Dextral
//!   Crystallisation force the removed side, exactly like Perfect Essences),
//! - adds the Alloy's specific **crafted** modifier (`ModKind::Explicit`,
//!   tagged via the bundle's Verisium / Runic-Ward mod pool).
//!
//! ## Cross-version gate
//!
//! Alloys did not exist before 0.5. The currency's `patch_range` is
//! `from(0.5.0)`; the advisor's candidate generator and `can_apply_to`
//! both refuse it on earlier patches. Unlike the Recombinator (Standard-only
//! in 0.5), Alloys are a *new* 0.5 system available in the challenge league,
//! so there is no `League` restriction.
//!
//! ## Determinism
//!
//! The Alloy carries the exact `ModId` it grants (the data binding lives in
//! the bundle once the Verisium pool is integrated by the pipeline). The
//! engine just enforces the remove-then-add mechanic and the family /
//! affix-fullness rules.

use rand::Rng;

use crate::currency::{ApplyContext, ApplyOutcome, CannotApply, Currency};
use crate::error::{EngineError, EngineResult};
use crate::ids::{CurrencyId, ModId};
use crate::item::{AffixType, Item, ModRoll, Rarity};
use crate::mods::ModKind;
use crate::patch::{PatchRange, PatchVersion};

/// Maximum modifiers a Rare item may carry on each affix side (3 prefixes /
/// 3 suffixes in PoE2). Keeps the remove-then-add path from overflowing.
const MAX_AFFIXES_PER_SIDE: usize = 3;

/// A Verisium Alloy currency — replaces one mod with a guaranteed crafted
/// modifier on a Rare item (0.5+).
#[derive(Debug, Clone)]
pub struct Alloy {
    /// Display id, e.g. `"AlloyOfRunicWard"`.
    pub id: CurrencyId,
    /// Display name, e.g. `"Verisium Alloy of Runic Ward"`.
    pub display_name: String,
    /// The mod this alloy guarantees to add (single-target alloys, and the
    /// fallback when `class_targets` carries no entry for the item's class).
    pub target_mod: ModId,
    /// Class-specific targets: real alloys grant a *different* crafted mod
    /// per item class (poe2db: e.g. Runic Alloy — Ring `+max Runic Ward`,
    /// Amulet `%max Runic Ward`, Belt `Ward regen`). When non-empty, the
    /// item's class selects the granted mod; a class with no entry cannot
    /// receive this alloy.
    pub class_targets: Vec<(crate::ids::ItemClassId, ModId)>,
    /// Base-name-specific targets — used by Liquid / Potent / Ancient
    /// Emotions (0.5), which grant a different crafted mod per **jewel
    /// base** ("Ruby" / "Sapphire" / "Emerald" / "Diamond", plus Time-Lost
    /// variants). Keys are matched case-insensitively against the item's
    /// base display name from the [`crate::base_registry::BaseRegistry`]
    /// (or the raw base id). Several entries may share a key — the engine
    /// samples uniformly among matching targets at apply time.
    pub base_targets: Vec<(String, ModId)>,
}

impl Alloy {
    pub fn new(
        id: impl Into<CurrencyId>,
        display_name: impl Into<String>,
        target_mod: impl Into<ModId>,
    ) -> Self {
        Self {
            id: id.into(),
            display_name: display_name.into(),
            target_mod: target_mod.into(),
            class_targets: Vec::new(),
            base_targets: Vec::new(),
        }
    }

    /// Construct a class-targeted alloy (the data-driven production shape).
    /// `targets` must be non-empty; the first entry doubles as the fallback
    /// `target_mod` for diagnostics.
    pub fn with_class_targets(
        id: impl Into<CurrencyId>,
        display_name: impl Into<String>,
        targets: Vec<(crate::ids::ItemClassId, ModId)>,
    ) -> Self {
        let fallback = targets
            .first()
            .map_or_else(|| ModId::from("UnboundAlloyTarget"), |(_, m)| m.clone());
        Self {
            id: id.into(),
            display_name: display_name.into(),
            target_mod: fallback,
            class_targets: targets,
            base_targets: Vec::new(),
        }
    }

    /// Construct a base-targeted "emotion" currency (Liquid / Potent /
    /// Ancient Emotions, 0.5): keys are jewel base names ("Ruby", …).
    pub fn with_base_targets(
        id: impl Into<CurrencyId>,
        display_name: impl Into<String>,
        targets: Vec<(String, ModId)>,
    ) -> Self {
        let fallback = targets
            .first()
            .map_or_else(|| ModId::from("UnboundEmotionTarget"), |(_, m)| m.clone());
        Self {
            id: id.into(),
            display_name: display_name.into(),
            target_mod: fallback,
            class_targets: Vec::new(),
            base_targets: targets,
        }
    }

    /// Resolve the granted mod for an item's class. `None` when this alloy
    /// has class targets but none match (the alloy is illegal on the class).
    pub fn target_for_class(&self, class: &crate::ids::ItemClassId) -> Option<&ModId> {
        if self.class_targets.is_empty() {
            return Some(&self.target_mod);
        }
        self.class_targets
            .iter()
            .find(|(c, _)| c == class)
            .map(|(_, m)| m)
    }

    /// Alloys exist only from patch 0.5.0 onward.
    pub const PATCH_RANGE: PatchRange = PatchRange::from(PatchVersion::PATCH_0_5_0);

    /// Resolve the granted mod for the item. Base-targeted emotions match
    /// the jewel base name (exact, case-insensitive; uniform sample among
    /// matching entries). Class-targeted alloys match the resolved item
    /// class. Single-target alloys always grant `target_mod`.
    fn resolve_target_mod(&self, item: &Item, ctx: &mut ApplyContext<'_>) -> EngineResult<ModId> {
        if self.base_targets.is_empty() {
            let class = ctx.base_registry.resolve_item_class(item);
            return self.target_for_class(&class).cloned().ok_or_else(|| {
                EngineError::InvalidApplication(format!(
                    "{}: no crafted modifier for item class {class}",
                    self.display_name
                ))
            });
        }
        let base_name = ctx
            .base_registry
            .get(&item.base)
            .map_or_else(|| item.base.as_str(), |b| b.name.as_str());
        let matches: smallvec::SmallVec<[&ModId; 4]> = self
            .base_targets
            .iter()
            .filter(|(key, _)| key.eq_ignore_ascii_case(base_name))
            .map(|(_, m)| m)
            .collect();
        if matches.is_empty() {
            return Err(EngineError::InvalidApplication(format!(
                "{}: no crafted modifier for base \"{base_name}\"",
                self.display_name
            )));
        }
        Ok(matches[ctx.rng.gen_range(0..matches.len())].clone())
    }
}

/// Resolve which affix the removal must target so the alloy's crafted mod
/// (which always lands in `target_affix`) has room afterward, composing with
/// any Sinistral/Dextral Crystallisation forced side.
///
/// On a Rare whose `target_affix` side is already full, the removal MUST free
/// a slot on that same side — otherwise the add would overflow to an illegal
/// 4th prefix/suffix. Returns the affix filter for the removable set, or an
/// error when the constraints are contradictory (Crystallisation forces the
/// other side while the target side is full).
fn resolve_removal_filter(
    crystallisation: Option<AffixType>,
    target_affix: AffixType,
    item: &Item,
) -> EngineResult<Option<AffixType>> {
    let target_full = match target_affix {
        AffixType::Prefix => item.prefixes.len() >= MAX_AFFIXES_PER_SIDE,
        AffixType::Suffix => item.suffixes.len() >= MAX_AFFIXES_PER_SIDE,
        _ => false,
    };
    match (crystallisation, target_full) {
        (Some(forced), true) if forced != target_affix => Err(EngineError::AffixSlotFull {
            affix_type:
                "Alloy: target affix slot is full and Crystallisation forces the other side",
        }),
        (Some(forced), _) => Ok(Some(forced)),
        (None, true) => Ok(Some(target_affix)),
        (None, false) => Ok(None),
    }
}

impl Currency for Alloy {
    fn id(&self) -> &CurrencyId {
        &self.id
    }

    fn name(&self) -> &'static str {
        // The trait requires a 'static name; the data-driven display name is
        // on `display_name`. Report the generic kind here.
        "Verisium Alloy"
    }

    fn valid_rarities(&self) -> crate::currency::RaritySet {
        crate::currency::RaritySet::RARE
    }

    fn can_apply_to(&self, item: &Item) -> Result<(), CannotApply> {
        let valid = self.valid_rarities();
        if !valid.contains(item.rarity) {
            return Err(CannotApply::WrongRarity {
                item_rarity: item.rarity,
                expected: valid,
            });
        }
        if item.corrupted {
            return Err(CannotApply::Corrupted);
        }
        if item.mirrored {
            return Err(CannotApply::Mirrored);
        }
        // Needs at least one non-fractured mod to remove.
        let removable = item
            .prefixes
            .iter()
            .chain(item.suffixes.iter())
            .any(|m| !m.is_fractured);
        if !removable {
            return Err(CannotApply::Other(
                "Alloy needs at least one non-fractured modifier to replace",
            ));
        }
        // 0.5 crafted-mod cap: "items can only have 1 crafted modifier at a
        // time". Alloys add a crafted mod, so an item that already carries
        // one is rejected up front (deterministic legality for the advisor).
        if item.has_crafted_mod() {
            return Err(CannotApply::Other(
                "item already has a crafted modifier (limit 1 in 0.5)",
            ));
        }
        Ok(())
    }

    fn apply(&self, item: &mut Item, ctx: &mut ApplyContext<'_>) -> EngineResult<ApplyOutcome> {
        // Patch gate: Alloys are 0.5+.
        if !Self::PATCH_RANGE.contains(ctx.patch) {
            return Err(EngineError::InvalidApplication(format!(
                "{}: Verisium Alloys are a 0.5 system; not available in patch {}",
                self.display_name, ctx.patch
            )));
        }
        if item.sanctified {
            return Err(EngineError::ItemSanctified);
        }
        if item.mirrored {
            return Err(EngineError::InvalidApplication(
                "Alloy cannot be applied to a mirrored item".into(),
            ));
        }
        if item.corrupted {
            return Err(EngineError::ItemCorrupted);
        }
        if item.rarity != Rarity::Rare {
            return Err(EngineError::InvalidApplication(format!(
                "{}: requires a Rare-rarity item",
                self.display_name
            )));
        }
        // 0.5 crafted-mod cap (see `can_apply_to`).
        if item.has_crafted_mod() {
            return Err(EngineError::InvalidApplication(format!(
                "{}: item already has a crafted modifier (limit 1 in 0.5)",
                self.display_name
            )));
        }

        let target_mod = self.resolve_target_mod(item, ctx)?;
        let target_def = ctx.registry.get(&target_mod).cloned().ok_or_else(|| {
            EngineError::Data(format!("Alloy: target mod `{target_mod}` not in registry"))
        })?;

        // Crystallisation forces which affix the removal targets (same as
        // Perfect Essences); on top of that we constrain the removal so the
        // crafted mod's side always has room (see `resolve_removal_filter`).
        let crystallisation = ctx.omens.consume_affix_only(ctx.patch);
        let affix_filter = resolve_removal_filter(crystallisation, target_def.affix_type, item)?;

        // Build the removable list per filter.
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
                self.display_name
            )));
        }

        let pick = ctx.rng.gen_range(0..removables.len());
        let (rm_affix, rm_idx) = removables[pick];
        match rm_affix {
            AffixType::Prefix => {
                item.prefixes.remove(rm_idx);
            }
            AffixType::Suffix => {
                item.suffixes.remove(rm_idx);
            }
            _ => unreachable!(),
        }

        // Family exclusivity against survivors (after removal).
        if let Some(g) = ctx.registry.group_of(&target_mod) {
            let occupied = item
                .prefixes
                .iter()
                .chain(item.suffixes.iter())
                .any(|m| ctx.registry.group_of(&m.mod_id) == Some(g));
            if occupied {
                return Err(EngineError::ModGroupExclusive(format!(
                    "Alloy's target mod-group `{g}` already on item after removal"
                )));
            }
        }

        let values = target_def
            .stats
            .iter()
            .map(|s| s.roll(ctx.rng.gen::<f64>()))
            .collect();
        let roll = ModRoll {
            mod_id: target_mod.clone(),
            affix_type: target_def.affix_type,
            // Alloy outputs are *crafted* modifiers (0.5: guaranteed, max 1
            // per item).
            kind: ModKind::Crafted,
            values,
            is_fractured: false,
        };
        match target_def.affix_type {
            AffixType::Prefix => item.prefixes.push(roll),
            AffixType::Suffix => item.suffixes.push(roll),
            _ => {}
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{ItemClassId, ModGroupId, StatId, TagId};
    use crate::item::QualityKind;
    use crate::mods::{ModDefinition, ModDomain, ModFlags, ModGroup, ModStat, SpawnWeight};
    use crate::omen::{Omen, OmenSet};
    use crate::registry::ModRegistry;
    use rand::SeedableRng;
    use rand_xoshiro::Xoshiro256PlusPlus;
    use smallvec::smallvec;

    fn alloy_mod() -> ModDefinition {
        ModDefinition {
            id: ModId::from("RunicWardCrafted"),
            name: Some("Verisium Runic Ward".into()),
            mod_group: ModGroup(ModGroupId::from("RunicWard")),
            affix_type: AffixType::Prefix,
            kind: ModKind::Explicit,
            domain: ModDomain::Item,
            tags: smallvec![TagId::from("runic_ward")],
            concept_set: smallvec![],
            spawn_weights: smallvec![SpawnWeight {
                tag: TagId::from("runic_ward"),
                weight: 1,
            }],
            stats: smallvec![ModStat {
                stat_id: StatId::from("runic_ward"),
                min: 20.0,
                max: 40.0,
            }],
            required_level: 1,
            tier: None,
            allowed_item_classes: smallvec![ItemClassId::from("BodyArmour")],
            patch_range: PatchRange::ALL,
            flags: ModFlags::empty(),
            text_template: None,
        }
    }

    fn rare_with(prefix: &str, suffix: &str) -> Item {
        Item {
            base: ItemClassId::from("BodyArmour").as_str().into(),
            ilvl: 82,
            rarity: Rarity::Rare,
            corrupted: false,
            sanctified: false,
            mirrored: false,
            quality: 0,
            quality_kind: QualityKind::Untagged,
            implicits: smallvec![],
            prefixes: smallvec![ModRoll {
                mod_id: ModId::from(prefix),
                affix_type: AffixType::Prefix,
                kind: ModKind::Explicit,
                values: smallvec![1.0],
                is_fractured: false,
            }],
            suffixes: smallvec![ModRoll {
                mod_id: ModId::from(suffix),
                affix_type: AffixType::Suffix,
                kind: ModKind::Explicit,
                values: smallvec![1.0],
                is_fractured: false,
            }],
            enchantments: smallvec![],
            hidden_desecrated: None,
            sockets: smallvec![],
            hinekora_lock: None,
        }
    }

    fn ctx<'a>(
        reg: &'a ModRegistry,
        rng: &'a mut Xoshiro256PlusPlus,
        omens: &'a mut OmenSet,
        patch: PatchVersion,
    ) -> ApplyContext<'a> {
        ApplyContext::new_without_bases(reg, rng, patch, omens)
    }

    #[test]
    fn alloy_replaces_a_mod_with_its_crafted_mod_in_0_5() {
        let reg = ModRegistry::from_mods(vec![alloy_mod()], vec![]);
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(1);
        let mut omens = OmenSet::new();
        let mut item = rare_with("OldPrefix", "OldSuffix");
        let before = item.prefixes.len() + item.suffixes.len();
        Alloy::new(
            "AlloyRunicWard",
            "Verisium Alloy of Runic Ward",
            "RunicWardCrafted",
        )
        .apply(
            &mut item,
            &mut ctx(&reg, &mut rng, &mut omens, PatchVersion::PATCH_0_5_0),
        )
        .unwrap();
        // Net mod count unchanged (remove 1, add 1).
        assert_eq!(item.prefixes.len() + item.suffixes.len(), before);
        assert!(item
            .prefixes
            .iter()
            .chain(item.suffixes.iter())
            .any(|m| m.mod_id.as_str() == "RunicWardCrafted"));
    }

    #[test]
    fn alloy_output_is_a_crafted_mod() {
        let reg = ModRegistry::from_mods(vec![alloy_mod()], vec![]);
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(7);
        let mut omens = OmenSet::new();
        let mut item = rare_with("OldPrefix", "OldSuffix");
        Alloy::new("A", "Alloy", "RunicWardCrafted")
            .apply(
                &mut item,
                &mut ctx(&reg, &mut rng, &mut omens, PatchVersion::PATCH_0_5_0),
            )
            .unwrap();
        let added = item
            .prefixes
            .iter()
            .chain(item.suffixes.iter())
            .find(|m| m.mod_id.as_str() == "RunicWardCrafted")
            .expect("crafted mod present");
        assert_eq!(
            added.kind,
            ModKind::Crafted,
            "Alloy output must carry ModKind::Crafted (0.5 crafted-mod rules)"
        );
    }

    #[test]
    fn base_targeted_emotion_matches_jewel_base_exactly() {
        use crate::base::{BaseType, ReleaseState};
        use crate::base_registry::BaseRegistry;
        use crate::ids::BaseTypeId;

        let reg = ModRegistry::from_mods(vec![alloy_mod()], vec![]);
        let mk_base = |id: &str, name: &str| BaseType {
            id: BaseTypeId::from(id),
            name: name.into(),
            item_class: ItemClassId::from("Jewel"),
            attribute_pool: crate::item_class::AttributePool::Str,
            drop_level: 1,
            tags: smallvec![],
            implicits: smallvec![],
            inventory: crate::base::InventorySize {
                width: 1,
                height: 1,
            },
            release_state: ReleaseState::Released,
            patch_range: PatchRange::ALL,
        };
        let bases = BaseRegistry::from_bases(vec![
            mk_base("Metadata/Jewels/Ruby", "Ruby"),
            mk_base("Metadata/Jewels/TimeLostRuby", "Time-Lost Ruby"),
        ]);
        let emotion = Alloy::with_base_targets(
            "LiquidParanoia",
            "Liquid Paranoia",
            vec![("Ruby".to_string(), ModId::from("RunicWardCrafted"))],
        );

        let mut omens = OmenSet::new();
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(3);

        // Ruby base → applies and grants the crafted mod.
        let mut ruby = rare_with("OldPrefix", "OldSuffix");
        ruby.base = "Metadata/Jewels/Ruby".into();
        let mut ctx = ApplyContext::new(
            &reg,
            &bases,
            &mut rng,
            PatchVersion::PATCH_0_5_0,
            &mut omens,
        );
        emotion.apply(&mut ruby, &mut ctx).unwrap();
        assert!(ruby
            .prefixes
            .iter()
            .chain(ruby.suffixes.iter())
            .any(|m| m.mod_id.as_str() == "RunicWardCrafted" && m.kind == ModKind::Crafted));

        // Time-Lost Ruby must NOT match the plain "Ruby" key (Ancient
        // emotions target Time-Lost jewels; plain ones must not).
        let mut tl = rare_with("OldPrefix", "OldSuffix");
        tl.base = "Metadata/Jewels/TimeLostRuby".into();
        let mut rng2 = Xoshiro256PlusPlus::seed_from_u64(4);
        let mut omens2 = OmenSet::new();
        let mut ctx2 = ApplyContext::new(
            &reg,
            &bases,
            &mut rng2,
            PatchVersion::PATCH_0_5_0,
            &mut omens2,
        );
        let r = emotion.apply(&mut tl, &mut ctx2);
        assert!(
            matches!(r, Err(EngineError::InvalidApplication(_))),
            "Time-Lost base must not accept a plain-Ruby emotion; got {r:?}"
        );
    }

    #[test]
    fn alloy_rejected_when_item_already_has_a_crafted_mod() {
        let reg = ModRegistry::from_mods(vec![alloy_mod()], vec![]);
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(9);
        let mut omens = OmenSet::new();
        let mut item = rare_with("OldPrefix", "OldSuffix");
        // Pre-existing crafted mod on the suffix side.
        item.suffixes[0].kind = ModKind::Crafted;
        let alloy = Alloy::new("A", "Alloy", "RunicWardCrafted");
        assert!(
            alloy.can_apply_to(&item).is_err(),
            "can_apply_to must reject a second crafted mod"
        );
        let r = alloy.apply(
            &mut item,
            &mut ctx(&reg, &mut rng, &mut omens, PatchVersion::PATCH_0_5_0),
        );
        assert!(
            matches!(r, Err(EngineError::InvalidApplication(_))),
            "apply must reject a second crafted mod; got {r:?}"
        );
    }

    #[test]
    fn alloy_rejected_before_0_5() {
        let reg = ModRegistry::from_mods(vec![alloy_mod()], vec![]);
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(2);
        let mut omens = OmenSet::new();
        let mut item = rare_with("OldPrefix", "OldSuffix");
        let r = Alloy::new("AlloyRunicWard", "Verisium Alloy", "RunicWardCrafted").apply(
            &mut item,
            &mut ctx(&reg, &mut rng, &mut omens, PatchVersion::PATCH_0_4_0),
        );
        assert!(
            matches!(r, Err(EngineError::InvalidApplication(_))),
            "Alloy must be rejected in 0.4; got {r:?}"
        );
    }

    #[test]
    fn alloy_requires_rare() {
        let reg = ModRegistry::from_mods(vec![alloy_mod()], vec![]);
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(3);
        let mut omens = OmenSet::new();
        let mut item = rare_with("OldPrefix", "OldSuffix");
        item.rarity = Rarity::Magic;
        let r = Alloy::new("A", "Alloy", "RunicWardCrafted").apply(
            &mut item,
            &mut ctx(&reg, &mut rng, &mut omens, PatchVersion::PATCH_0_5_0),
        );
        assert!(matches!(r, Err(EngineError::InvalidApplication(_))));
    }

    #[test]
    fn dextral_crystallisation_forces_suffix_removal() {
        let reg = ModRegistry::from_mods(vec![alloy_mod()], vec![]);
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(4);
        let mut omens = OmenSet::new();
        omens.push(Omen::dextral_crystallisation());
        let mut item = rare_with("KeepPrefix", "DropSuffix");
        Alloy::new("A", "Alloy", "RunicWardCrafted")
            .apply(
                &mut item,
                &mut ctx(&reg, &mut rng, &mut omens, PatchVersion::PATCH_0_5_0),
            )
            .unwrap();
        // Suffix removed (only suffix was removable), crafted prefix added.
        assert!(item.suffixes.is_empty(), "Dextral must remove the suffix");
        assert!(item
            .prefixes
            .iter()
            .any(|m| m.mod_id.as_str() == "KeepPrefix"));
        assert!(item
            .prefixes
            .iter()
            .any(|m| m.mod_id.as_str() == "RunicWardCrafted"));
    }

    #[test]
    fn can_apply_to_rejects_normal_and_fractured_only() {
        let alloy = Alloy::new("A", "Alloy", "RunicWardCrafted");
        let mut item = rare_with("P", "S");
        item.rarity = Rarity::Normal;
        assert!(alloy.can_apply_to(&item).is_err());
    }

    // ---- Additional edge cases (P5 fidelity coverage) ----------------------

    /// A non-RunicWard prefix mod registered so `group_of` resolves in
    /// family-collision and overflow scenarios.
    fn prefix_mod(id: &str, group: &str) -> ModDefinition {
        let mut m = alloy_mod();
        m.id = ModId::from(id);
        m.mod_group = ModGroup(ModGroupId::from(group));
        m.affix_type = AffixType::Prefix;
        m
    }

    fn roll_of(id: &str, affix: AffixType, fractured: bool) -> ModRoll {
        ModRoll {
            mod_id: ModId::from(id),
            affix_type: affix,
            kind: ModKind::Explicit,
            values: smallvec![1.0],
            is_fractured: fractured,
        }
    }

    #[test]
    fn sinistral_crystallisation_forces_prefix_removal() {
        // Mirror of the dextral test: Sinistral forces the removal to a
        // PREFIX, leaving the suffix intact. The crafted mod (a prefix) lands
        // in the freed prefix slot.
        let reg = ModRegistry::from_mods(vec![alloy_mod()], vec![]);
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(11);
        let mut omens = OmenSet::new();
        omens.push(Omen::sinistral_crystallisation());
        let mut item = rare_with("DropPrefix", "KeepSuffix");
        Alloy::new("A", "Alloy", "RunicWardCrafted")
            .apply(
                &mut item,
                &mut ctx(&reg, &mut rng, &mut omens, PatchVersion::PATCH_0_5_0),
            )
            .unwrap();
        assert!(
            item.suffixes
                .iter()
                .any(|m| m.mod_id.as_str() == "KeepSuffix"),
            "Sinistral must keep the suffix"
        );
        assert!(item
            .prefixes
            .iter()
            .any(|m| m.mod_id.as_str() == "RunicWardCrafted"));
        assert!(
            !item
                .prefixes
                .iter()
                .any(|m| m.mod_id.as_str() == "DropPrefix"),
            "the original prefix was removed"
        );
    }

    #[test]
    fn family_collision_after_removal_is_rejected() {
        // A surviving mod shares the crafted mod's group (RunicWard). The
        // removal targets the (only removable) suffix, so the same-group prefix
        // survives → ModGroupExclusive.
        let reg = ModRegistry::from_mods(
            vec![alloy_mod(), prefix_mod("OtherRunicWard", "RunicWard")],
            vec![],
        );
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(12);
        let mut omens = OmenSet::new();
        let mut item = rare_with("OtherRunicWard", "RemovableSuffix");
        // Fracture the same-group prefix so the removal cannot take it; the
        // suffix is the only removable, guaranteeing the collision.
        item.prefixes[0].is_fractured = true;
        let r = Alloy::new("A", "Alloy", "RunicWardCrafted").apply(
            &mut item,
            &mut ctx(&reg, &mut rng, &mut omens, PatchVersion::PATCH_0_5_0),
        );
        assert!(
            matches!(r, Err(EngineError::ModGroupExclusive(_))),
            "expected ModGroupExclusive; got {r:?}"
        );
    }

    #[test]
    fn missing_target_mod_in_registry_is_data_error() {
        // The alloy names a mod the registry doesn't carry.
        let reg = ModRegistry::from_mods(vec![alloy_mod()], vec![]);
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(13);
        let mut omens = OmenSet::new();
        let mut item = rare_with("P", "S");
        let r = Alloy::new("A", "Alloy", "NoSuchMod").apply(
            &mut item,
            &mut ctx(&reg, &mut rng, &mut omens, PatchVersion::PATCH_0_5_0),
        );
        assert!(matches!(r, Err(EngineError::Data(_))), "got {r:?}");
    }

    #[test]
    fn rejects_sanctified_corrupted_and_mirrored() {
        let reg = ModRegistry::from_mods(vec![alloy_mod()], vec![]);
        let mk = |mutate: fn(&mut Item)| {
            let mut item = rare_with("P", "S");
            mutate(&mut item);
            item
        };
        let apply = |item: &mut Item| {
            let mut rng = Xoshiro256PlusPlus::seed_from_u64(14);
            let mut omens = OmenSet::new();
            Alloy::new("A", "Alloy", "RunicWardCrafted").apply(
                item,
                &mut ctx(&reg, &mut rng, &mut omens, PatchVersion::PATCH_0_5_0),
            )
        };
        let mut sanctified = mk(|i| i.sanctified = true);
        assert!(matches!(
            apply(&mut sanctified),
            Err(EngineError::ItemSanctified)
        ));
        let mut corrupted = mk(|i| i.corrupted = true);
        assert!(matches!(
            apply(&mut corrupted),
            Err(EngineError::ItemCorrupted)
        ));
        let mut mirrored = mk(|i| i.mirrored = true);
        assert!(matches!(
            apply(&mut mirrored),
            Err(EngineError::InvalidApplication(_))
        ));
    }

    #[test]
    fn no_overflow_when_prefix_side_full() {
        // Rare with 3 prefixes + 1 suffix; a prefix-target alloy with no omen.
        // The removal is constrained to the prefix side so the crafted prefix
        // has room — the result never reaches 4 prefixes and the suffix
        // survives.
        let reg = ModRegistry::from_mods(
            vec![
                alloy_mod(),
                prefix_mod("P1", "G1"),
                prefix_mod("P2", "G2"),
                prefix_mod("P3", "G3"),
            ],
            vec![],
        );
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(15);
        let mut omens = OmenSet::new();
        let mut item = rare_with("P1", "KeepSuffix");
        item.prefixes.push(roll_of("P2", AffixType::Prefix, false));
        item.prefixes.push(roll_of("P3", AffixType::Prefix, false));
        assert_eq!(item.prefixes.len(), 3);
        Alloy::new("A", "Alloy", "RunicWardCrafted")
            .apply(
                &mut item,
                &mut ctx(&reg, &mut rng, &mut omens, PatchVersion::PATCH_0_5_0),
            )
            .expect("alloy on a full-prefix Rare must succeed without overflow");
        assert!(item.prefixes.len() <= 3, "no overflow to 4 prefixes");
        assert_eq!(item.prefixes.len() + item.suffixes.len(), 4);
        assert!(item
            .prefixes
            .iter()
            .any(|m| m.mod_id.as_str() == "RunicWardCrafted"));
        assert!(item
            .suffixes
            .iter()
            .any(|m| m.mod_id.as_str() == "KeepSuffix"));
    }

    #[test]
    fn crystallisation_contradiction_when_target_side_full_errors() {
        // 3 prefixes + 1 suffix; prefix-target alloy + Dextral Crystallisation
        // (forces suffix removal). The removal can't free a prefix slot, so the
        // add would overflow → AffixSlotFull.
        let reg = ModRegistry::from_mods(
            vec![
                alloy_mod(),
                prefix_mod("P1", "G1"),
                prefix_mod("P2", "G2"),
                prefix_mod("P3", "G3"),
            ],
            vec![],
        );
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(16);
        let mut omens = OmenSet::new();
        omens.push(Omen::dextral_crystallisation());
        let mut item = rare_with("P1", "OnlySuffix");
        item.prefixes.push(roll_of("P2", AffixType::Prefix, false));
        item.prefixes.push(roll_of("P3", AffixType::Prefix, false));
        let r = Alloy::new("A", "Alloy", "RunicWardCrafted").apply(
            &mut item,
            &mut ctx(&reg, &mut rng, &mut omens, PatchVersion::PATCH_0_5_0),
        );
        assert!(
            matches!(r, Err(EngineError::AffixSlotFull { .. })),
            "expected AffixSlotFull; got {r:?}"
        );
    }
}
