//! Tier-ordinal derivation post-pass (P6 / schema v3).
//!
//! RePoE-fork ships one `ModDefinition` per (group × tier) but does not
//! carry an explicit tier number. The engine's inclusive higher-tier
//! weighting works off `ModDefinition::tier_strength_key`, which falls back
//! to `required_level` when `tier` is `None`. Assigning explicit tier
//! ordinals here makes the ladder unambiguous (and lets the UI label tiers
//! as "T1 / T2 / …") without changing engine behavior.
//!
//! Convention (matches the engine's `tier_strength_key` and the in-game
//! display): **tier 1 is the strongest** — the member of the mod-group with
//! the highest `required_level`. Weaker tiers get larger numbers. Ties on
//! `required_level` are broken by `ModId` for determinism.

use std::collections::BTreeMap;

use poc2_data::Bundle;

/// Assign an explicit `tier` ordinal to every mod in the bundle, grouped by
/// `(mod_group, affix_type)`. Returns the number of mods that received a
/// tier (i.e. all mods whose group has ≥ 1 member).
///
/// Idempotent: re-running overwrites any previously assigned ordinals.
pub fn assign_tier_ordinals(bundle: &mut Bundle) -> usize {
    // Bucket mod indices by (group, affix). We key by owned ids/affix so we
    // don't hold a borrow on `bundle.mods` while mutating it.
    let mut buckets: BTreeMap<(String, u8), Vec<usize>> = BTreeMap::new();
    for (i, m) in bundle.mods.iter().enumerate() {
        let key = (m.mod_group.0.as_str().to_string(), affix_key(m.affix_type));
        buckets.entry(key).or_default().push(i);
    }

    let mut assigned = 0usize;
    for (_key, mut idxs) in buckets {
        // Sort strongest-first: highest required_level first, ties broken by
        // mod id ascending for determinism.
        idxs.sort_by(|&a, &b| {
            let ma = &bundle.mods[a];
            let mb = &bundle.mods[b];
            mb.required_level
                .cmp(&ma.required_level)
                .then_with(|| ma.id.as_str().cmp(mb.id.as_str()))
        });
        for (ordinal, idx) in idxs.iter().enumerate() {
            // tier 1 = strongest (first in the sorted order).
            let tier = u16::try_from(ordinal + 1).unwrap_or(u16::MAX);
            bundle.mods[*idx].tier = Some(tier);
            assigned += 1;
        }
    }
    assigned
}

fn affix_key(affix: poc2_engine::AffixType) -> u8 {
    use poc2_engine::AffixType;
    match affix {
        AffixType::Prefix => 0,
        AffixType::Suffix => 1,
        AffixType::Implicit => 2,
        AffixType::Enchantment => 3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use poc2_engine::{
        AffixType, ItemClassId, ModDefinition, ModDomain, ModFlags, ModGroup, ModGroupId, ModId,
        ModKind, PatchRange, PatchVersion,
    };
    use smallvec::smallvec;

    fn mk(id: &str, group: &str, affix: AffixType, req: u32) -> ModDefinition {
        ModDefinition {
            id: ModId::from(id),
            name: None,
            mod_group: ModGroup(ModGroupId::from(group)),
            affix_type: affix,
            kind: ModKind::Explicit,
            domain: ModDomain::Item,
            tags: smallvec![],
            concept_set: smallvec![],
            spawn_weights: smallvec![],
            stats: smallvec![],
            required_level: req,
            tier: None,
            allowed_item_classes: smallvec![ItemClassId::from("BodyArmour")],
            patch_range: PatchRange::ALL,
            flags: ModFlags::empty(),
            text_template: None,
        }
    }

    #[test]
    fn strongest_tier_is_one() {
        let mut bundle = Bundle::empty(PatchVersion::PATCH_0_5_0, "test");
        bundle.mods = vec![
            mk("Life_low", "Life", AffixType::Prefix, 1),
            mk("Life_mid", "Life", AffixType::Prefix, 40),
            mk("Life_top", "Life", AffixType::Prefix, 75),
        ];
        let n = assign_tier_ordinals(&mut bundle);
        assert_eq!(n, 3);
        let tier = |id: &str| {
            bundle
                .mods
                .iter()
                .find(|m| m.id.as_str() == id)
                .unwrap()
                .tier
        };
        assert_eq!(tier("Life_top"), Some(1), "highest req_level is tier 1");
        assert_eq!(tier("Life_mid"), Some(2));
        assert_eq!(tier("Life_low"), Some(3));
    }

    #[test]
    fn separate_groups_and_affixes_get_independent_ladders() {
        let mut bundle = Bundle::empty(PatchVersion::PATCH_0_5_0, "test");
        bundle.mods = vec![
            mk("Life_top", "Life", AffixType::Prefix, 75),
            mk("Res_top", "Resist", AffixType::Suffix, 60),
            mk("Res_low", "Resist", AffixType::Suffix, 1),
        ];
        assign_tier_ordinals(&mut bundle);
        let tier = |id: &str| {
            bundle
                .mods
                .iter()
                .find(|m| m.id.as_str() == id)
                .unwrap()
                .tier
        };
        // Each group's strongest is tier 1, independent of other groups.
        assert_eq!(tier("Life_top"), Some(1));
        assert_eq!(tier("Res_top"), Some(1));
        assert_eq!(tier("Res_low"), Some(2));
    }

    #[test]
    fn is_idempotent() {
        let mut bundle = Bundle::empty(PatchVersion::PATCH_0_5_0, "test");
        bundle.mods = vec![
            mk("A", "G", AffixType::Prefix, 10),
            mk("B", "G", AffixType::Prefix, 20),
        ];
        assign_tier_ordinals(&mut bundle);
        let first: Vec<_> = bundle.mods.iter().map(|m| m.tier).collect();
        assign_tier_ordinals(&mut bundle);
        let second: Vec<_> = bundle.mods.iter().map(|m| m.tier).collect();
        assert_eq!(first, second);
    }

    #[test]
    fn single_mod_group_gets_tier_one() {
        let mut bundle = Bundle::empty(PatchVersion::PATCH_0_5_0, "test");
        bundle.mods = vec![mk("Solo", "SoloGroup", AffixType::Prefix, 50)];
        let n = assign_tier_ordinals(&mut bundle);
        assert_eq!(n, 1);
        assert_eq!(
            bundle.mods[0].tier,
            Some(1),
            "the only member of a group is its strongest tier"
        );
    }

    #[test]
    fn ties_on_required_level_break_by_mod_id() {
        // Two mods in one group with identical required_level: the tier is
        // decided deterministically by ModId ascending, NOT by insertion order.
        let mut bundle = Bundle::empty(PatchVersion::PATCH_0_5_0, "test");
        // Inserted Zeta-before-Alpha to prove the sort (not insertion) decides.
        bundle.mods = vec![
            mk("Zeta", "G", AffixType::Prefix, 40),
            mk("Alpha", "G", AffixType::Prefix, 40),
        ];
        assign_tier_ordinals(&mut bundle);
        let tier = |id: &str| {
            bundle
                .mods
                .iter()
                .find(|m| m.id.as_str() == id)
                .unwrap()
                .tier
        };
        assert_eq!(tier("Alpha"), Some(1), "smaller id wins the tie → tier 1");
        assert_eq!(tier("Zeta"), Some(2));
    }
}
