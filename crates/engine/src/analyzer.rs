//! Concept-based mod analyzer.
//!
//! Classifies a mod by mapping each of its [`crate::mods::ModStat::stat_id`]
//! values to a [`ConceptId`]. The set of concepts produced is the mod's
//! `concept_set`; mods with `|concept_set| > 1` are flagged
//! [`crate::mods::ModFlags::HYBRID`].
//!
//! ## Two layers of classification
//!
//! 1. **Curated taxonomy** ([`built_in_classifier`]): a hand-rolled pattern
//!    matcher for the stat-id stems we know about (resources, resistances,
//!    attributes, damages, speeds, critical, item-rarity, etc.). This
//!    handles the ~20 most common concepts that drive the advisor's target
//!    language.
//! 2. **Bundle override** ([`ConceptMapClassifier`]): exact `stat_id →
//!    concept_id` lookups loaded from the data bundle's `concept_map`. The
//!    pipeline owns this map; it can refine or correct the curated rules
//!    per patch without touching engine code.
//!
//! A composite [`Classifier`] tries the bundle map first, then falls back
//! to the curated rules, then to `Concept::Other`.
//!
//! ## Hybrid detection
//!
//! After classification, the analyzer compares the resulting concepts. If
//! all of a mod's stats map to the same concept the mod is **atomic**;
//! otherwise it is **hybrid** and gets `ModFlags::HYBRID`. Concrete cases:
//! `local_energy_shield` + `local_energy_shield_+%` both produce
//! `EnergyShield` (atomic). `minimum_added_fire_damage` +
//! `maximum_added_fire_damage` both produce `AddedFireDamage` (atomic).
//! `local_energy_shield_+%` + `base_maximum_life` produce `EnergyShield`
//! plus `Life` (hybrid).

use ahash::AHashMap;
use smallvec::SmallVec;

use crate::ids::{ConceptId, StatId};
use crate::mods::{ModDefinition, ModFlags};

/// A classifier that turns a `stat_id` into a concept.
pub trait Classifier {
    fn classify(&self, stat_id: &StatId) -> ConceptId;
}

/// Built-in curated rules. Matches the most common PoE2 stat-id stems.
///
/// Returns [`ConceptId::from("Other")`] when no rule matches; bundle
/// overrides cover those cases.
pub struct BuiltInClassifier;

impl Classifier for BuiltInClassifier {
    fn classify(&self, stat_id: &StatId) -> ConceptId {
        ConceptId::from(built_in_classifier(stat_id.as_str()))
    }
}

/// Bundle-supplied `stat_id → concept_id` lookup.
pub struct ConceptMapClassifier {
    map: AHashMap<StatId, ConceptId>,
}

impl ConceptMapClassifier {
    pub fn new(map: AHashMap<StatId, ConceptId>) -> Self {
        Self { map }
    }
    pub fn empty() -> Self {
        Self {
            map: AHashMap::new(),
        }
    }
    pub fn len(&self) -> usize {
        self.map.len()
    }
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

impl Classifier for ConceptMapClassifier {
    fn classify(&self, stat_id: &StatId) -> ConceptId {
        self.map
            .get(stat_id)
            .cloned()
            .unwrap_or_else(|| ConceptId::from(built_in_classifier(stat_id.as_str())))
    }
}

/// Composite classifier: bundle map first, then curated rules, then `Other`.
pub struct CompositeClassifier {
    pub bundle: ConceptMapClassifier,
    pub builtin: BuiltInClassifier,
}

impl CompositeClassifier {
    pub fn new(bundle_map: AHashMap<StatId, ConceptId>) -> Self {
        Self {
            bundle: ConceptMapClassifier::new(bundle_map),
            builtin: BuiltInClassifier,
        }
    }
}

impl Classifier for CompositeClassifier {
    fn classify(&self, stat_id: &StatId) -> ConceptId {
        if let Some(c) = self.bundle.map.get(stat_id) {
            return c.clone();
        }
        self.builtin.classify(stat_id)
    }
}

// ---------------------------------------------------------------------------
// Curated rules
// ---------------------------------------------------------------------------

/// Match a raw stat-id string to a curated concept name.
///
/// The function is deliberately ordered from most-specific to most-generic
/// so a hit on, say, `"all_resistance_%"` doesn't get swallowed by a
/// generic `"resistance"` pattern.
#[allow(clippy::too_many_lines)] // single big match, clearer flat than nested
pub fn built_in_classifier(stat_id: &str) -> &'static str {
    let s = stat_id;

    // ---- Resources: Life / Mana / Energy Shield ----
    if contains_any(s, &["energy_shield"]) {
        return "EnergyShield";
    }
    if s.contains("life_regeneration")
        || s.contains("life_recovery")
        || s == "base_maximum_life"
        || s.contains("maximum_life")
    {
        return "Life";
    }
    if s.contains("mana_regeneration") || s.contains("maximum_mana") || s == "base_maximum_mana" {
        return "Mana";
    }

    // ---- Resistances ----
    if s.contains("all_") && s.contains("resistance") {
        return "AllResistances";
    }
    if s.contains("fire") && s.contains("resistance") {
        return "FireResistance";
    }
    if s.contains("cold") && s.contains("resistance") {
        return "ColdResistance";
    }
    if s.contains("lightning") && s.contains("resistance") {
        return "LightningResistance";
    }
    if s.contains("chaos") && s.contains("resistance") {
        return "ChaosResistance";
    }

    // ---- Attributes ----
    if s.contains("all_attributes") {
        return "AllAttributes";
    }
    if s.contains("strength") && !s.contains("requirement") {
        return "Strength";
    }
    if s.contains("dexterity") && !s.contains("requirement") {
        return "Dexterity";
    }
    if s.contains("intelligence") && !s.contains("requirement") {
        return "Intelligence";
    }

    // ---- Defences ----
    if s.contains("armour_+%") || s == "base_physical_damage_reduction_rating" {
        return "Armour";
    }
    if s.contains("evasion") {
        return "Evasion";
    }
    if s.contains("block") {
        return "Block";
    }

    // ---- Damage ----
    // Added damage families. Note: minimum_added_X + maximum_added_X resolve
    // to the SAME concept ("AddedXDamage") so mods with both are NOT hybrid.
    if s.contains("added_fire_damage") || s.contains("added_minimum_fire") {
        return "AddedFireDamage";
    }
    if s.contains("added_cold_damage") || s.contains("added_minimum_cold") {
        return "AddedColdDamage";
    }
    if s.contains("added_lightning_damage") || s.contains("added_minimum_lightning") {
        return "AddedLightningDamage";
    }
    if s.contains("added_chaos_damage") || s.contains("added_minimum_chaos") {
        return "AddedChaosDamage";
    }
    if s.contains("added_physical_damage") || s.contains("added_minimum_physical") {
        return "AddedPhysicalDamage";
    }

    // ---- Damage % increases ----
    if s.contains("physical_damage_+%") {
        return "IncreasedPhysicalDamage";
    }
    if s.contains("fire_damage_+%") {
        return "IncreasedFireDamage";
    }
    if s.contains("cold_damage_+%") {
        return "IncreasedColdDamage";
    }
    if s.contains("lightning_damage_+%") {
        return "IncreasedLightningDamage";
    }
    if s.contains("chaos_damage_+%") {
        return "IncreasedChaosDamage";
    }
    if s.contains("elemental_damage_+%") {
        return "IncreasedElementalDamage";
    }
    if s.contains("spell_damage_+%") {
        return "IncreasedSpellDamage";
    }
    if s.contains("attack_damage_+%") || s.contains("damage_with_attack_skills") {
        return "IncreasedAttackDamage";
    }
    if s.contains("minion_damage_+%") {
        return "MinionDamage";
    }

    // ---- Speeds ----
    if s.contains("attack_speed") {
        return "AttackSpeed";
    }
    if s.contains("cast_speed") {
        return "CastSpeed";
    }
    if s.contains("movement_velocity") || s.contains("movement_speed") {
        return "MovementSpeed";
    }
    if s.contains("projectile_speed") {
        return "ProjectileSpeed";
    }

    // ---- Critical ----
    if s.contains("critical_strike_chance") || s.contains("critical_hit_chance") {
        return "CritChance";
    }
    if s.contains("critical_strike_multiplier") || s.contains("critical_damage_bonus") {
        return "CritDamage";
    }

    // ---- Quantity / Rarity ----
    if s.contains("rarity_of_items") {
        return "ItemRarity";
    }
    if s.contains("quantity_of_items") {
        return "ItemQuantity";
    }

    // ---- Skill levels ----
    if s.contains("level_of_all_attack_skills") {
        return "AttackSkillLevel";
    }
    if s.contains("level_of_all_spell_skills") {
        return "SpellSkillLevel";
    }
    if s.contains("level_of_all_minion_skills") {
        return "MinionSkillLevel";
    }
    if s.contains("level_of_all_projectile_skills") {
        return "ProjectileSkillLevel";
    }
    if s.contains("level_of_all") && s.contains("skills") {
        return "AllSkillLevel";
    }

    // ---- Charges ----
    if s.contains("frenzy_charge") {
        return "FrenzyCharges";
    }
    if s.contains("power_charge") {
        return "PowerCharges";
    }
    if s.contains("endurance_charge") {
        return "EnduranceCharges";
    }

    // ---- Misc / fallback ----
    if s.contains("accuracy") {
        return "Accuracy";
    }
    if s.contains("stun") {
        return "Stun";
    }

    "Other"
}

fn contains_any(s: &str, needles: &[&str]) -> bool {
    needles.iter().any(|n| s.contains(n))
}

// ---------------------------------------------------------------------------
// Whole-mod analysis
// ---------------------------------------------------------------------------

/// Compute the concept set for a mod's stats.
///
/// Returns the set of distinct concepts produced by the mod's `stats`
/// array, deduplicated and stable-ordered (matches insertion order of
/// first occurrence).
pub fn concept_set_for(
    mod_def: &ModDefinition,
    classifier: &dyn Classifier,
) -> SmallVec<[ConceptId; 4]> {
    let mut out: SmallVec<[ConceptId; 4]> = SmallVec::new();
    for stat in &mod_def.stats {
        let c = classifier.classify(&stat.stat_id);
        if !out.contains(&c) {
            out.push(c);
        }
    }
    out
}

/// Apply concept classification + hybrid flag to every mod in `mods`.
/// Mutates `concept_set` and toggles `ModFlags::HYBRID` accordingly.
pub fn analyze(mods: &mut [ModDefinition], classifier: &dyn Classifier) {
    for m in mods.iter_mut() {
        m.concept_set = concept_set_for(m, classifier);
        if m.concept_set.len() > 1 {
            m.flags |= ModFlags::HYBRID;
        } else {
            m.flags.remove(ModFlags::HYBRID);
        }
    }
}

#[cfg(test)]
mod tests {
    use smallvec::smallvec;

    use super::*;
    use crate::ids::{ItemClassId, ModGroupId, ModId, TagId};
    use crate::item::AffixType;
    use crate::mods::{ModDomain, ModGroup, ModKind, ModStat, SpawnWeight};
    use crate::patch::PatchRange;

    fn mk(id: &str, stat_ids: &[&str]) -> ModDefinition {
        ModDefinition {
            id: ModId::from(id),
            name: None,
            mod_group: ModGroup(ModGroupId::from(id)),
            affix_type: AffixType::Prefix,
            kind: ModKind::Explicit,
            domain: ModDomain::Item,
            tags: smallvec![],
            concept_set: smallvec![],
            spawn_weights: smallvec![SpawnWeight {
                tag: TagId::from("x"),
                weight: 1
            }],
            stats: stat_ids
                .iter()
                .map(|s| ModStat {
                    stat_id: (*s).into(),
                    min: 0.0,
                    max: 0.0,
                })
                .collect(),
            required_level: 1,
            tier: None,
            allowed_item_classes: smallvec![ItemClassId::from("x")],
            patch_range: PatchRange::ALL,
            flags: ModFlags::empty(),
            text_template: None,
        }
    }

    #[test]
    fn classifier_recognizes_energy_shield() {
        let c = BuiltInClassifier;
        assert_eq!(
            c.classify(&"local_energy_shield".into()).as_str(),
            "EnergyShield"
        );
        assert_eq!(
            c.classify(&"base_maximum_energy_shield".into()).as_str(),
            "EnergyShield"
        );
        assert_eq!(
            c.classify(&"local_energy_shield_+%".into()).as_str(),
            "EnergyShield"
        );
    }

    #[test]
    fn classifier_recognizes_life_and_mana() {
        let c = BuiltInClassifier;
        assert_eq!(c.classify(&"base_maximum_life".into()).as_str(), "Life");
        assert_eq!(
            c.classify(&"base_life_regeneration_rate_per_minute".into())
                .as_str(),
            "Life"
        );
        assert_eq!(c.classify(&"base_maximum_mana".into()).as_str(), "Mana");
    }

    #[test]
    fn classifier_recognizes_resistances() {
        let c = BuiltInClassifier;
        assert_eq!(
            c.classify(&"base_fire_damage_resistance_%".into()).as_str(),
            "FireResistance"
        );
        assert_eq!(
            c.classify(&"base_cold_damage_resistance_%".into()).as_str(),
            "ColdResistance"
        );
        assert_eq!(
            c.classify(&"base_chaos_damage_resistance_%".into())
                .as_str(),
            "ChaosResistance"
        );
        assert_eq!(
            c.classify(&"all_resistance_%".into()).as_str(),
            "AllResistances"
        );
    }

    #[test]
    fn classifier_recognizes_attributes_and_speeds() {
        let c = BuiltInClassifier;
        assert_eq!(
            c.classify(&"additional_strength".into()).as_str(),
            "Strength"
        );
        assert_eq!(
            c.classify(&"additional_dexterity".into()).as_str(),
            "Dexterity"
        );
        assert_eq!(
            c.classify(&"additional_intelligence".into()).as_str(),
            "Intelligence"
        );
        assert_eq!(
            c.classify(&"attack_speed_+%".into()).as_str(),
            "AttackSpeed"
        );
        assert_eq!(c.classify(&"cast_speed_+%".into()).as_str(), "CastSpeed");
        assert_eq!(
            c.classify(&"movement_velocity_+%".into()).as_str(),
            "MovementSpeed"
        );
    }

    #[test]
    fn added_damage_min_max_resolve_to_one_concept() {
        // Important: a mod with both minimum_added_fire_damage AND
        // maximum_added_fire_damage is NOT hybrid — both stats share the
        // AddedFireDamage concept.
        let mut m = mk(
            "FlatFire1",
            &["minimum_added_fire_damage", "maximum_added_fire_damage"],
        );
        analyze(std::slice::from_mut(&mut m), &BuiltInClassifier);
        assert_eq!(m.concept_set.len(), 1);
        assert_eq!(m.concept_set[0].as_str(), "AddedFireDamage");
        assert!(!m.flags.contains(ModFlags::HYBRID));
    }

    #[test]
    fn es_plus_life_is_hybrid() {
        // The user's worked-example "T1 ES flat or hybrid" target case.
        // A single mod with ES stats + Life stats => 2 concepts => HYBRID.
        let mut m = mk(
            "ESLifeHybrid1",
            &["local_energy_shield_+%", "base_maximum_life"],
        );
        analyze(std::slice::from_mut(&mut m), &BuiltInClassifier);
        assert_eq!(m.concept_set.len(), 2);
        assert!(m.concept_set.iter().any(|c| c.as_str() == "EnergyShield"));
        assert!(m.concept_set.iter().any(|c| c.as_str() == "Life"));
        assert!(m.flags.contains(ModFlags::HYBRID));
    }

    #[test]
    fn atomic_es_mod_is_not_hybrid() {
        let mut m = mk(
            "ESOnly1",
            &["local_energy_shield", "local_energy_shield_+%"],
        );
        analyze(std::slice::from_mut(&mut m), &BuiltInClassifier);
        assert_eq!(m.concept_set.len(), 1);
        assert!(!m.flags.contains(ModFlags::HYBRID));
    }

    #[test]
    fn unknown_stat_falls_back_to_other() {
        let c = BuiltInClassifier;
        assert_eq!(
            c.classify(&"made_up_obscure_stat_id".into()).as_str(),
            "Other"
        );
    }

    #[test]
    fn bundle_override_wins_over_builtin() {
        let mut m = AHashMap::new();
        m.insert(
            StatId::from("local_energy_shield"),
            ConceptId::from("CustomConcept"),
        );
        let composite = CompositeClassifier::new(m);
        assert_eq!(
            composite.classify(&"local_energy_shield".into()).as_str(),
            "CustomConcept"
        );
        // Stat not in the bundle map falls through to the built-in.
        assert_eq!(
            composite.classify(&"base_maximum_life".into()).as_str(),
            "Life"
        );
    }

    #[test]
    fn analyze_clears_stale_hybrid_flag() {
        // If a mod was previously flagged HYBRID but is now atomic, analyze
        // must clear the flag.
        let mut m = mk("X", &["local_energy_shield"]);
        m.flags |= ModFlags::HYBRID;
        analyze(std::slice::from_mut(&mut m), &BuiltInClassifier);
        assert!(!m.flags.contains(ModFlags::HYBRID));
    }
}
