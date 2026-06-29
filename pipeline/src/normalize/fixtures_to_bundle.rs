//! Lower curated fixture mods (desecrated, Vaal-corruption implicits) into
//! the bundle's `mods` list with the right `kind` + `flags` set.
//!
//! See `crate::sources::fixtures` for the why/how of the fixture format.
//! This module is the bridge between a parsed [`FixtureSnapshot`] and the
//! engine's [`ModDefinition`] type that the registry consumes.

use poc2_data::Bundle;
use poc2_engine::item::AffixType;
use poc2_engine::mods::{ModDomain, ModFlags, ModKind, SpawnWeight};
use poc2_engine::{
    ItemClassId, ModDefinition, ModGroup, ModGroupId, ModId, ModStat, PatchRange, StatId,
};
use smallvec::SmallVec;
use tracing::info;

use crate::error::PipelineResult;
use crate::sources::fixtures::{
    DesecratedFixtureEntry, FixtureSnapshot, FixtureStat, VaalImplicitFixtureEntry,
};

/// Apply the fixture snapshot to a bundle in place. Adds desecrated and
/// Vaal-corruption implicit mods to `bundle.mods`. De-duplicates by id —
/// any entry that already exists in the bundle (e.g., shipped via RePoE-
/// fork's `is_essence_only` path with a colliding name) is skipped.
#[allow(clippy::unnecessary_wraps)] // forward-compat with future fallible joins
pub fn normalize_fixtures(snapshot: &FixtureSnapshot, bundle: &mut Bundle) -> PipelineResult<()> {
    info!("{}", snapshot.count_summary());

    let existing_ids: ahash::AHashSet<String> = bundle
        .mods
        .iter()
        .map(|m| m.id.as_str().to_string())
        .collect();

    let known_classes: ahash::AHashSet<String> = bundle
        .item_classes
        .iter()
        .map(|c| c.id.as_str().to_string())
        .collect();

    let mut added_desecrated = 0usize;
    let mut added_vaal = 0usize;

    for entry in &snapshot.desecrated {
        if existing_ids.contains(&entry.id) {
            continue;
        }
        let allowed = filter_allowed_classes(&entry.classes, &known_classes);
        if allowed.is_empty() {
            // No matching class in the bundle yet (e.g., synthetic test
            // bundles without item_classes populated) — accept the
            // declared classes verbatim so the mod is still queryable.
            // Falling back here keeps the desecrated catalogue intact
            // even when the bundle is partially constructed.
        }
        let allowed_final = if allowed.is_empty() {
            entry
                .classes
                .iter()
                .map(|c| ItemClassId::from(c.as_str()))
                .collect()
        } else {
            allowed
        };
        let affix = parse_affix(&entry.affix);
        bundle.mods.push(ModDefinition {
            id: ModId::from(entry.id.as_str()),
            name: Some(entry.name.clone()),
            mod_group: ModGroup(ModGroupId::from(group_for_desecrated(entry).as_str())),
            affix_type: affix,
            kind: ModKind::Desecrated,
            domain: ModDomain::Item,
            tags: SmallVec::new(),
            concept_set: SmallVec::new(),
            // Desecrated mods are added via bone reveals, not currency
            // sampling, so the registry's spawn_weight path doesn't gate
            // them. Leaving this empty keeps the bundle validator happy
            // (it would otherwise demand each tag exist in `bundle.tags`)
            // without any loss of correctness for the planner.
            spawn_weights: SmallVec::new(),
            stats: build_stats(&entry.stats),
            required_level: entry.required_level,
            // Desecrated mods are revealed, not tier-sampled; tier ordinal
            // is not meaningful for them.
            tier: None,
            allowed_item_classes: allowed_final,
            patch_range: PatchRange::ALL,
            flags: ModFlags::DESECRATED_ONLY,
            // Prefer the cleaned poe2db mod text; the affix name alone
            // ("Amanamu's") is shared by dozens of mods and useless for
            // display.
            text_template: Some(entry.text.clone().unwrap_or_else(|| entry.name.clone())),
        });
        added_desecrated += 1;
    }

    for entry in &snapshot.vaal_implicits {
        if existing_ids.contains(&entry.id) {
            continue;
        }
        let allowed = filter_allowed_classes(&entry.classes, &known_classes);
        let allowed_final = if allowed.is_empty() {
            entry
                .classes
                .iter()
                .map(|c| ItemClassId::from(c.as_str()))
                .collect()
        } else {
            allowed
        };
        bundle.mods.push(ModDefinition {
            id: ModId::from(entry.id.as_str()),
            name: Some(entry.name.clone()),
            mod_group: ModGroup(ModGroupId::from(group_for_vaal(entry).as_str())),
            affix_type: AffixType::Implicit,
            kind: ModKind::Corrupted,
            domain: ModDomain::Item,
            tags: SmallVec::new(),
            concept_set: SmallVec::new(),
            // Vaal implicits are stamped onto the implicit slot by
            // `VaalOrb` corruption, not by sampling the explicit pool;
            // empty spawn_weights mirror the desecrated reasoning above.
            spawn_weights: SmallVec::new(),
            stats: build_stats(&entry.stats),
            required_level: entry.required_level,
            // Vaal implicits occupy the implicit slot; no tier ladder.
            tier: None,
            allowed_item_classes: allowed_final,
            patch_range: PatchRange::ALL,
            flags: ModFlags::CORRUPTED_ONLY,
            text_template: Some(entry.name.clone()),
        });
        added_vaal += 1;
    }

    info!(
        added_desecrated,
        added_vaal, "fixtures normalized into bundle.mods"
    );

    // ---- Verisium Alloys (0.5) ------------------------------------------
    // Emit the curated 13-alloy catalogue into `bundle.alloys`. Each entry
    // binds the alloy currency id + display name to its per-class crafted
    // mod targets (`Alloy*` mods shipped via RePoE above). Targets whose
    // mod id is absent from the bundle are dropped with a count (partial
    // bundles degrade instead of crashing the resolver).
    let bundle_mod_ids: ahash::AHashSet<&str> = bundle.mods.iter().map(|m| m.id.as_str()).collect();
    let mut alloy_entries: Vec<serde_json::Value> = Vec::new();
    let mut dropped_targets = 0usize;
    for alloy in &snapshot.alloys {
        let targets: Vec<serde_json::Value> = alloy
            .targets
            .iter()
            .filter(|t| {
                let present = bundle_mod_ids.contains(t.engine_mod_id.as_str());
                if !present {
                    dropped_targets += 1;
                }
                present
            })
            .map(|t| {
                serde_json::json!({
                    "class": t.class,
                    "engine_mod_id": t.engine_mod_id,
                })
            })
            .collect();
        if targets.is_empty() {
            continue;
        }
        alloy_entries.push(serde_json::json!({
            "id": alloy.id,
            "name": alloy.name,
            "metadata_id": alloy.metadata_id,
            "drop_level": alloy.drop_level,
            "targets": targets,
        }));
    }
    let added_alloys = alloy_entries.len();
    if added_alloys > 0 {
        bundle.alloys.section_version = 2;
        bundle.alloys.entries = alloy_entries;
    }
    info!(
        added_alloys,
        dropped_targets, "alloy catalogue normalized into bundle.alloys"
    );

    // ---- Distilled Emotions (0.5 jewel crafting) -------------------------
    // Emit the 26-emotion catalogue (Liquid / Potent / Ancient). Targets
    // keep their verbatim modifier text for display; `engine_mod_id` may be
    // null when the granted mod isn't exported upstream yet (display-only).
    let mut emotion_entries: Vec<serde_json::Value> = Vec::new();
    let mut unbound_emotion_targets = 0usize;
    for emotion in &snapshot.emotions {
        let targets: Vec<serde_json::Value> = emotion
            .targets
            .iter()
            .map(|t| {
                let bound = t
                    .engine_mod_id
                    .as_deref()
                    .filter(|m| bundle_mod_ids.contains(*m));
                if bound.is_none() {
                    unbound_emotion_targets += 1;
                }
                serde_json::json!({
                    "base": t.base,
                    "affix": t.affix,
                    "modifier": t.modifier,
                    "engine_mod_id": bound,
                })
            })
            .collect();
        emotion_entries.push(serde_json::json!({
            "id": emotion.id,
            "name": emotion.name,
            "metadata_id": emotion.metadata_id,
            "drop_level": emotion.drop_level,
            "kind": emotion.kind,
            "targets": targets,
        }));
    }
    let added_emotions = emotion_entries.len();
    if added_emotions > 0 {
        bundle.emotions.section_version = 1;
        bundle.emotions.entries = emotion_entries;
    }
    info!(
        added_emotions,
        unbound_emotion_targets, "emotion catalogue normalized into bundle.emotions"
    );

    bundle.header.sources.0.push(snapshot.revision.clone());
    Ok(())
}

/// After CoE essence ingestion runs, mods that essences grant AND that
/// cannot spawn naturally should carry the `ESSENCE_ONLY` flag. RePoE-fork
/// already flags most of them via `is_essence_only`, but the join can drop
/// entries when CoE references a mod by name that RePoE-fork stores under a
/// different generation_type label. This pass fixes those drops by walking
/// the essence catalogue and flagging referenced mods **that have no
/// positive natural spawn weight**.
///
/// The natural-spawnability guard is load-bearing (M14 audit): essences
/// frequently grant a tier of the *natural* mod ladder (e.g. Essence of
/// Haste → `LocalIncreasedAttackSpeed3`). Force-flagging those bricked the
/// shared tiers out of every Transmute/Aug/Regal/Exalt/Chaos pool — 118 of
/// the bundle's 162 flagged mods were natural-roll tiers, and the
/// Min-Modifier-Level keep-≥1-tier exception then fell back to T1 because
/// the mid tiers were invisible to basic orbs.
///
/// Idempotent — flag is already a bitset OR.
pub fn flag_essence_target_mods(bundle: &mut Bundle) -> usize {
    let mut targeted: ahash::AHashSet<String> = ahash::AHashSet::new();
    for entry in &bundle.essences.entries {
        let Some(groups) = entry.get("tier_groups").and_then(|v| v.as_array()) else {
            continue;
        };
        for group in groups {
            let Some(tiers) = group.get("tiers").and_then(|v| v.as_array()) else {
                continue;
            };
            for tier in tiers {
                if let Some(id) = tier.get("engine_mod_id").and_then(|v| v.as_str()) {
                    targeted.insert(id.to_string());
                }
            }
        }
    }
    let mut promoted = 0usize;
    for m in &mut bundle.mods {
        let naturally_spawnable = m.spawn_weights.iter().any(|sw| sw.weight > 0);
        if targeted.contains(m.id.as_str())
            && !naturally_spawnable
            && !m.flags.contains(ModFlags::ESSENCE_ONLY)
        {
            m.flags |= ModFlags::ESSENCE_ONLY;
            promoted += 1;
        }
    }
    if promoted > 0 {
        info!(
            promoted,
            "promoted unspawnable essence-target mods to ESSENCE_ONLY"
        );
    }
    promoted
}

fn parse_affix(s: &str) -> AffixType {
    // The fixture authoring guide mandates one of the four canonical
    // affix names; the trailing wildcard arm catches typos by defaulting
    // to Prefix (the downstream test suite then highlights mis-spelled
    // affixes via mod placement assertions). Identity between the
    // unknown arm and the Prefix arm is intentional.
    match s {
        "Suffix" => AffixType::Suffix,
        "Implicit" => AffixType::Implicit,
        "Enchantment" => AffixType::Enchantment,
        // "Prefix" + unknown both default to Prefix.
        _ => AffixType::Prefix,
    }
}

fn group_for_desecrated(entry: &DesecratedFixtureEntry) -> String {
    // Group by lord+stat-stem so the engine's mod-group exclusivity stays
    // sensible (a single item shouldn't carry two Amanamu-life-on-hit
    // mods, nor two tiers of the same lord's spirit-reservation line).
    // The first stat's stat_id provides the stem. The lord-less jewel
    // pool groups under its affix name ("Lightless" / "of the Abyss").
    let lord = entry.lord.as_deref().unwrap_or(entry.name.as_str());
    let stem = entry
        .stats
        .first()
        .map_or("desecrated_anon", |s| s.stat_id.as_str());
    format!("Desecrated_{}_{}", lord.replace(' ', ""), stem)
}

fn group_for_vaal(entry: &VaalImplicitFixtureEntry) -> String {
    let stem = entry
        .stats
        .first()
        .map_or("vaal_anon", |s| s.stat_id.as_str());
    format!("VaalImplicit_{stem}")
}

fn build_stats(raw: &[FixtureStat]) -> SmallVec<[ModStat; 4]> {
    raw.iter()
        .map(|s| ModStat {
            stat_id: StatId::from(s.stat_id.as_str()),
            min: s.min,
            max: s.max,
        })
        .collect()
}

fn filter_allowed_classes(
    declared: &[String],
    known: &ahash::AHashSet<String>,
) -> SmallVec<[ItemClassId; 8]> {
    declared
        .iter()
        .filter(|c| known.contains(c.as_str()))
        .map(|c| ItemClassId::from(c.as_str()))
        .collect()
}

// `spawn_weights_for_classes` was an earlier draft that synthesized
// (class-name → unit-weight) entries so the registry's tag eligibility
// path would accept these mods for explicit rolls. That path doesn't apply
// to desecrated/corrupted kinds, and the synthesized class-name tags
// failed bundle validation (those names aren't in `bundle.tags`). Removed.
#[allow(dead_code)]
fn _phase_e_spawn_weights_placeholder(_: &[String]) -> SmallVec<[SpawnWeight; 6]> {
    SmallVec::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use poc2_engine::PatchVersion;

    #[test]
    fn fixtures_land_in_bundle_with_correct_kind_and_flags() {
        let snapshot = crate::sources::fixtures::load().expect("fixtures parse");
        let mut bundle = Bundle::empty(PatchVersion::PATCH_0_4_0, "test");
        normalize_fixtures(&snapshot, &mut bundle).unwrap();

        let desecrated_count = bundle
            .mods
            .iter()
            .filter(|m| m.kind == ModKind::Desecrated)
            .count();
        assert_eq!(desecrated_count, snapshot.desecrated.len());

        let vaal_count = bundle
            .mods
            .iter()
            .filter(|m| m.kind == ModKind::Corrupted)
            .count();
        assert_eq!(vaal_count, snapshot.vaal_implicits.len());

        for m in &bundle.mods {
            match m.kind {
                ModKind::Desecrated => {
                    assert!(
                        m.flags.contains(ModFlags::DESECRATED_ONLY),
                        "{} missing DESECRATED_ONLY flag",
                        m.id.as_str()
                    );
                }
                ModKind::Corrupted => {
                    assert!(
                        m.flags.contains(ModFlags::CORRUPTED_ONLY),
                        "{} missing CORRUPTED_ONLY flag",
                        m.id.as_str()
                    );
                }
                _ => {}
            }
        }
    }

    #[test]
    fn second_normalize_is_idempotent() {
        let snapshot = crate::sources::fixtures::load().expect("fixtures parse");
        let mut bundle = Bundle::empty(PatchVersion::PATCH_0_4_0, "test");
        normalize_fixtures(&snapshot, &mut bundle).unwrap();
        let after_first = bundle.mods.len();
        normalize_fixtures(&snapshot, &mut bundle).unwrap();
        assert_eq!(after_first, bundle.mods.len(), "duplicate insertion");
    }
}
