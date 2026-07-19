//! Load-time concept augmentation.
//!
//! The data pipeline tags most mods with a `concept_set`, but two
//! crafting-relevant concepts are not emitted yet: Spirit (the PoE2 minion
//! resource) and SkillLevel (the gem-level / "+Levels of … Skills" mods). Both
//! currently fall under the catch-all `Other` concept, so they can't be
//! targeted by a goal or surfaced in the eligible-mods palette.
//!
//! This pass derives those two concepts deterministically from each mod's group
//! and text template at registry-build time. It is additive (existing concepts,
//! incl. `Other`, are preserved) so it can't break planner goal matching, and
//! it makes the codified `*-spirit` strategies actually resolve.

use crate::ids::ConceptId;
use crate::mods::ModDefinition;

/// Add derived `Spirit` / `SkillLevel` concepts to mods that warrant them.
pub(crate) fn augment_concepts(mods: &mut [ModDefinition]) {
    for m in mods.iter_mut() {
        let (spirit, skill_level) = {
            let text = m.text_template.as_deref().unwrap_or("");
            let group = m.mod_group.0.as_str();
            (is_spirit(group, text), is_skill_level(group, text))
        };
        if spirit {
            add_concept(m, "Spirit");
        }
        if skill_level {
            add_concept(m, "SkillLevel");
        }
    }
}

fn add_concept(m: &mut ModDefinition, id: &str) {
    let c = ConceptId::from(id);
    if !m.concept_set.iter().any(|x| x == &c) {
        m.concept_set.push(c);
    }
}

/// Spirit mods (`+X to Spirit` / `% increased Spirit`) — identified by the
/// `[Spirit|…]` stat tag in the template (or the canonical `BaseSpirit` group).
/// Excludes Arcane-Surge "Spirited" effect mods, which carry a different tag.
fn is_spirit(group: &str, text: &str) -> bool {
    group == "BaseSpirit" || text.contains("[Spirit")
}

/// Gem-level mods (`+Levels of … Skills` / socketed-gem-level groups).
fn is_skill_level(group: &str, text: &str) -> bool {
    group.contains("GemLevel") || text.contains("Level of")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{ItemClassId, ModGroupId, ModId, StatId};
    use crate::item::AffixType;
    use crate::mods::{ModDomain, ModFlags, ModGroup, ModKind, ModStat};
    use crate::patch::PatchRange;
    use smallvec::smallvec;

    fn mk(group: &str, text: &str, concepts: &[&str]) -> ModDefinition {
        ModDefinition {
            id: ModId::from("m"),
            name: None,
            mod_group: ModGroup(ModGroupId::from(group)),
            affix_type: AffixType::Prefix,
            kind: ModKind::Explicit,
            domain: ModDomain::Item,
            tags: smallvec![],
            concept_set: concepts.iter().map(|c| ConceptId::from(*c)).collect(),
            spawn_weights: smallvec![],
            stats: smallvec![ModStat {
                stat_id: StatId::from("s"),
                min: 0.0,
                max: 1.0
            }],
            required_level: 1,
            tier: None,
            allowed_item_classes: smallvec![ItemClassId::from("BodyArmour")],
            patch_range: PatchRange::ALL,
            flags: ModFlags::empty(),
            text_template: Some(text.to_string()),
        }
    }

    fn has(m: &ModDefinition, c: &str) -> bool {
        m.concept_set.iter().any(|x| x == &ConceptId::from(c))
    }

    #[test]
    fn derives_spirit_and_skill_level_additively() {
        let mut mods = vec![
            mk("BaseSpirit", "+(10-15) to [Spirit|Spirit]", &["Other"]),
            mk(
                "IncreaseSocketedGemLevel",
                "+1 to Level of all [Spell|Spell] Skills",
                &["Other"],
            ),
            mk(
                "ArcaneSurgeEffect",
                "increased [ArcaneSurge|Arcane Surge]",
                &["Other"],
            ),
            mk(
                "LocalIncreasedEnergyShieldPercent",
                "(10-15)% increased [EnergyShield|Energy Shield]",
                &["EnergyShield"],
            ),
        ];
        augment_concepts(&mut mods);

        assert!(has(&mods[0], "Spirit"));
        assert!(has(&mods[0], "Other"), "existing concepts preserved");
        assert!(has(&mods[1], "SkillLevel"));
        assert!(!has(&mods[2], "Spirit"), "Arcane Surge is not a spirit mod");
        assert!(!has(&mods[2], "SkillLevel"));
        assert!(!has(&mods[3], "Spirit"));
        assert!(!has(&mods[3], "SkillLevel"));
        assert!(has(&mods[3], "EnergyShield"));
    }
}
