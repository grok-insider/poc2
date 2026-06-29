//! Normalize a `RepoeSnapshot` into typed bundle entities.
//!
//! What this does:
//! - Maps `domain != "item"` mods out (we don't ship Map/Atlas/AbyssJewel
//!   mod data in the gear-crafting bundle).
//! - Maps `release_state != "released"` bases to `Legacy` / `Unreleased`
//!   (kept in the bundle so old items still parse).
//! - Translates `generation_type` strings into [`AffixType`] + [`ModKind`].
//! - Computes attribute pool from base tags.
//! - Builds [`ItemClass`] entries from the union of `item_class` strings
//!   appearing on bases. Class slot caps are assigned heuristically based
//!   on the class name; refinements (e.g. quivers having no sockets) come
//!   in later passes.
//! - Runs the [`poc2_engine::analyzer`] over every mod to populate its
//!   `concept_set` and `ModFlags::HYBRID` flag.
//!
//! Weight numerical values and the synergy graph are deferred to later
//! pipeline passes (Craft of Exile + poe2db scrape).

use ahash::AHashSet;
use indexmap::IndexMap;
use poc2_data::Bundle;
use poc2_engine::base::{InventorySize, ReleaseState};
use poc2_engine::item::AffixType;
use poc2_engine::item_class::AttributePool;
use poc2_engine::mods::{ModDomain, ModFlags, ModKind, SpawnWeight};
use poc2_engine::tag::TagCategory;
use poc2_engine::{
    BaseType, BaseTypeId, ItemClass, ItemClassId, ModDefinition, ModGroup, ModGroupId, ModId,
    ModStat, PatchRange, StatId, Tag, TagId,
};
use smallvec::SmallVec;
use tracing::{debug, info};

use crate::error::{PipelineError, PipelineResult};

/// Normalize an upstream item-class string to the codebase-wide
/// PascalCase no-space convention.
///
/// RePoE-fork ships a few class names with embedded spaces
/// (`"Body Armour"`, `"One Hand Sword"`, `"Two Hand Mace"`, etc.).
/// Strategies, rules, training corpus, and curated fixtures all use
/// the no-space form (`"BodyArmour"`, `"OneHandSword"`,
/// `"TwoHandMace"`). The downstream consumers in this very file —
/// [`class_caps`] and [`human_class_name`] — also implicitly assume
/// the no-space form. Normalizing at ingestion keeps every downstream
/// consumer in agreement.
///
/// Implementation: strip ASCII whitespace. RePoE-fork class names are
/// already correctly capitalized, so removing spaces is sufficient.
fn normalize_class_id(raw: &str) -> String {
    raw.chars().filter(|c| !c.is_whitespace()).collect()
}
use crate::sources::repoe::{RepoeMod, RepoeSnapshot, RepoeStat};

/// Apply a RePoE-fork snapshot to a bundle in place.
pub fn normalize_repoe(snapshot: &RepoeSnapshot, bundle: &mut Bundle) -> PipelineResult<()> {
    info!("{}", snapshot.count_summary());

    // 1. Tags first (everything else references them)
    for tag_id in &snapshot.tags {
        bundle.tags.push(Tag {
            id: TagId::from(tag_id.as_str()),
            category: classify_tag(tag_id),
            display_name: None,
        });
    }
    info!("normalized {} tags", bundle.tags.len());

    // 2. Bases — also collect item_class names along the way.
    // Class names are normalized at ingestion (`"Body Armour"` →
    // `"BodyArmour"`) so all downstream consumers see one canonical
    // form. See [`normalize_class_id`] for context.
    let mut item_classes_seen: AHashSet<String> = AHashSet::new();
    let mut bases_kept = 0usize;
    let mut bases_skipped = 0usize;
    for (id, raw) in &snapshot.base_items {
        if !is_gear_class(&raw.item_class) {
            bases_skipped += 1;
            continue;
        }
        let class_id = normalize_class_id(&raw.item_class);
        item_classes_seen.insert(class_id.clone());
        bundle.base_items.push(BaseType {
            id: BaseTypeId::from(id.as_str()),
            name: raw.name.clone(),
            item_class: ItemClassId::from(class_id.as_str()),
            attribute_pool: derive_attribute_pool(&raw.tags),
            drop_level: raw.drop_level,
            tags: raw
                .tags
                .iter()
                .map(|t| TagId::from(t.as_str()))
                .collect::<SmallVec<_>>(),
            implicits: raw
                .implicits
                .iter()
                .map(|m| ModId::from(m.as_str()))
                .collect::<SmallVec<_>>(),
            inventory: InventorySize {
                width: raw.inventory_width,
                height: raw.inventory_height,
            },
            release_state: derive_release_state(id, &raw.release_state),
            patch_range: PatchRange::ALL,
        });
        bases_kept += 1;
    }
    info!("normalized {bases_kept} bases ({bases_skipped} skipped — non-gear domain)");

    // 3. Item classes (synthesized from collected names)
    let class_tag_lookup = lookup_class_tags(&snapshot.tags);
    for class_name in &item_classes_seen {
        let tags_for_class = class_tag_lookup
            .get(class_name)
            .cloned()
            .unwrap_or_default();
        let class_caps = class_caps(class_name);
        bundle.item_classes.push(ItemClass {
            id: ItemClassId::from(class_name.as_str()),
            name: human_class_name(class_name),
            max_implicits: class_caps.implicits,
            max_prefixes: class_caps.prefixes,
            max_suffixes: class_caps.suffixes,
            max_sockets: class_caps.sockets,
            class_tags: tags_for_class,
            patch_range: PatchRange::ALL,
        });
    }
    info!("synthesized {} item classes", bundle.item_classes.len());

    // 4. Mods. We filter to `domain == "item"` (RePoE has thousands of
    //    map/atlas/jewel mods we don't ship in the gear bundle), with one
    //    exception: any mod referenced by a base's `implicits` array is
    //    kept regardless of its `generation_type`. RePoE labels some
    //    implicits as `generation_type: "unique"` rather than `"implicit"`.
    let referenced_implicits: AHashSet<&str> = bundle
        .base_items
        .iter()
        .flat_map(|b| b.implicits.iter().map(ModId::as_str))
        .collect();

    let mut mods_kept = 0usize;
    let mut mods_skipped = 0usize;
    let known_tags: AHashSet<&str> = snapshot.tags.iter().map(String::as_str).collect();
    // RePoE-fork mod `spawn_weights` reference base-tag names (e.g.
    // `body_armour`, `str_armour`, `weapon`, `default`), not class ids.
    // Build a `tag → {class_id}` map from the bases so
    // `derive_allowed_classes` can resolve each spawn-weight tag to the
    // class(es) that carry it. Without this map every mod would land
    // with empty `allowed_item_classes` and the engine's
    // `for_class_affix(...)` would never find them — every Transmute
    // would fail with `NoEligibleMods`.
    let tag_to_classes = build_tag_to_classes_index(&bundle.base_items);
    for (id, raw) in &snapshot.mods {
        let is_referenced_implicit = referenced_implicits.contains(id.as_str());
        // 0.5: the Jewel mod pool lives under domain "misc" in PoE2's
        // export (tags `strjewel` / `dex_radius_jewel` / …). Keep those so
        // jewel crafting (catalysts, Liquid/Ancient Emotions) has a
        // registry pool; their `allowed_item_classes` resolve to Jewel via
        // the tag→class map, so gear pools are unaffected.
        let is_jewel_pool = raw.domain == "misc"
            && (raw
                .spawn_weights
                .iter()
                .any(|sw| sw.weight > 0 && sw.tag.contains("jewel"))
                // Emotion-granted out-of-pool jewel mods (0.5): crafted
                // (`CraftedJewel*`) and radius (`JewelRadius*`) lines carry
                // weight 0 everywhere — they only enter items via Liquid /
                // Potent / Ancient Emotions, but the engine still needs
                // their definitions to roll values.
                || id.starts_with("CraftedJewel")
                || id.starts_with("JewelRadius"));
        if raw.domain != "item" && !is_jewel_pool && !is_referenced_implicit {
            mods_skipped += 1;
            continue;
        }
        // For mods referenced by a base's implicit list, we coerce to Implicit
        // even when the generation_type is something exotic like "unique".
        let affix = if let Some(a) = parse_generation_type_to_affix(&raw.generation_type) {
            a
        } else if is_referenced_implicit {
            AffixType::Implicit
        } else if let Some((a, _)) = super::alloy_fixups::alloy_affix_lookup(id) {
            // RePoE-fork has shipped the 0.5 `Alloy*` mods with an empty
            // generation_type; without this fallback they'd be dropped here
            // and the alloy catalogue's targets would dangle. The curated
            // poe2db table supplies the affix; `apply_alloy_fixups` later
            // back-fills required_level.
            a
        } else {
            mods_skipped += 1;
            continue;
        };
        let kind = if is_referenced_implicit {
            ModKind::Implicit
        } else {
            parse_generation_type_to_kind(&raw.generation_type)
        };
        let group = raw
            .groups
            .first()
            .cloned()
            .unwrap_or_else(|| format!("anonymous:{id}"));

        // Filter spawn-weight tags to only those present in the bundle's tag set.
        // RePoE-fork sometimes references tags absent from tags.json.
        let spawn_weights = raw
            .spawn_weights
            .iter()
            .filter(|sw| known_tags.contains(sw.tag.as_str()))
            .map(|sw| SpawnWeight {
                tag: TagId::from(sw.tag.as_str()),
                weight: sw.weight,
            })
            .collect::<SmallVec<_>>();

        let allowed_classes = derive_allowed_classes(&spawn_weights, &tag_to_classes);
        let stats = raw
            .stats
            .iter()
            .map(stat_from_repoe)
            .collect::<SmallVec<_>>();
        let flags = derive_flags(raw, &stats);

        let tags: SmallVec<_> = raw
            .implicit_tags
            .iter()
            .filter(|t| known_tags.contains(t.as_str()))
            .map(|t| TagId::from(t.as_str()))
            .collect();

        bundle.mods.push(ModDefinition {
            id: ModId::from(id.as_str()),
            name: if raw.name.is_empty() {
                None
            } else {
                Some(raw.name.clone())
            },
            mod_group: ModGroup(ModGroupId::from(group.as_str())),
            affix_type: affix,
            kind,
            domain: if is_jewel_pool {
                ModDomain::Jewel
            } else {
                ModDomain::Item
            },
            tags,
            concept_set: SmallVec::new(), // populated by mod analyzer (M2.7)
            spawn_weights,
            stats,
            required_level: raw.required_level,
            // Tier ordinal is derived post-hoc by `assign_tier_ordinals`
            // after all mods are loaded (it needs the full group ladder).
            tier: None,
            allowed_item_classes: allowed_classes,
            patch_range: PatchRange::ALL,
            flags,
            text_template: if raw.text.is_empty() {
                None
            } else {
                Some(raw.text.clone())
            },
        });
        mods_kept += 1;
    }
    info!("normalized {mods_kept} mods ({mods_skipped} skipped — non-item domain)");

    // 5. mods_by_base — derive by intersecting each base's tags with each
    //    mod's spawn_weights (where weight > 0). This is the engine's
    //    eligibility rule, applied as a static index for fast advisor lookup.
    bundle.mods_by_base = derive_mods_by_base(&bundle.base_items, &bundle.mods);
    info!(
        "derived mods_by_base ({} entries)",
        bundle.mods_by_base.len()
    );

    // 5b. Concept classification (M2.7) — populate concept_set on each
    //     mod and toggle the HYBRID flag for multi-concept mods.
    let classifier = poc2_engine::analyzer::BuiltInClassifier;
    poc2_engine::analyzer::analyze(&mut bundle.mods, &classifier);
    let hybrid_count = bundle
        .mods
        .iter()
        .filter(|m| m.flags.contains(poc2_engine::ModFlags::HYBRID))
        .count();
    info!(
        "classified concepts on {} mods ({hybrid_count} hybrid)",
        bundle.mods.len()
    );

    // 5c. Synthesize concept definitions for the concepts that actually
    //     appear in the bundle.
    bundle.concepts = collect_concepts(&bundle.mods);
    info!("collected {} distinct concepts", bundle.concepts.len());

    // 6. Provenance
    bundle.header.sources.0.extend(snapshot.revisions.0.clone());

    if mods_kept == 0 {
        return Err(PipelineError::Normalize(
            "no item-domain mods retained — RePoE-fork shape may have changed".into(),
        ));
    }
    Ok(())
}

// -------------------------------------------------------------------------
// Helpers
// -------------------------------------------------------------------------

fn classify_tag(id: &str) -> TagCategory {
    // Heuristic; refined as we encounter edge cases.
    if matches!(
        id,
        "boots"
            | "body_armour"
            | "helmet"
            | "gloves"
            | "ring"
            | "amulet"
            | "belt"
            | "shield"
            | "quiver"
            | "focus"
            | "staff"
            | "wand"
            | "bow"
            | "crossbow"
            | "sceptre"
            | "spear"
            | "talisman"
    ) {
        return TagCategory::ItemClass;
    }
    if id.contains("_armour") || id.starts_with("armour_") || id.contains("attribute") {
        return TagCategory::AttributePool;
    }
    if matches!(
        id,
        "physical" | "fire" | "cold" | "lightning" | "chaos" | "elemental" | "damage"
    ) {
        return TagCategory::Damage;
    }
    if matches!(id, "life" | "mana" | "energy_shield" | "resource") {
        return TagCategory::Resource;
    }
    if matches!(
        id,
        "defences" | "resistance" | "armour" | "evasion" | "block"
    ) {
        return TagCategory::Defence;
    }
    if matches!(
        id,
        "essence_only" | "desecrated" | "fractured" | "corrupted_only" | "implicit"
    ) {
        return TagCategory::ModKind;
    }
    if matches!(
        id,
        "attack" | "caster" | "minion" | "speed" | "critical" | "skill" | "spell"
    ) {
        return TagCategory::Skill;
    }
    TagCategory::Other
}

fn is_gear_class(class: &str) -> bool {
    // Currencies are also "items" in RePoE; we filter them out since they
    // don't roll mods. Heuristic.
    !class.is_empty()
        && !matches!(
            class,
            "StackableCurrency"
                | "Currency"
                | "DelveStackableSocketableCurrency"
                | "DivinationCard"
                | "QuestItem"
                | "Map"
                | "MapFragment"
                | "Microtransaction"
                | "Heist"
                | "HiddenItem"
                | "Gem"
                | "ActiveSkillGem"
                | "SupportSkillGem"
                | "MiscMapItem"
                | "Incubator"
        )
}

/// Release state for a base, with the 0.5 unique-runeforging override.
///
/// poe2db /us/Runeforging "Unique Runeforging /308": "Unique Verisium
/// Runeforging" upgrades unique items into "Runemastered <name>" variants,
/// and GGG models each as a separate base whose metadata id contains
/// "VerisiumUnique" (e.g.
/// `Metadata/Items/Amulets/FourAmuletUnique1VerisiumUnique3`, name
/// "Runemastered Veridical Chain"). RePoE-fork exports them
/// `release_state="released"`, which leaks ~274 unique bases into the
/// craftable base lists — these are runeforged *unique* variants, not
/// craftable bases, so they are forced to `Unique` regardless of the raw
/// state.
fn derive_release_state(base_id: &str, raw_state: &str) -> ReleaseState {
    if base_id.contains("VerisiumUnique") {
        return ReleaseState::Unique;
    }
    parse_release_state(raw_state)
}

fn parse_release_state(s: &str) -> ReleaseState {
    match s {
        "released" => ReleaseState::Released,
        "unreleased" => ReleaseState::Unreleased,
        "legacy" => ReleaseState::Legacy,
        "unique_only" => ReleaseState::Unique,
        other => {
            debug!(
                state = other,
                "unknown release_state, defaulting to Released"
            );
            ReleaseState::Released
        }
    }
}

fn parse_generation_type_to_affix(s: &str) -> Option<AffixType> {
    match s {
        "prefix" => Some(AffixType::Prefix),
        "suffix" => Some(AffixType::Suffix),
        "implicit" | "exarch_implicit" | "eater_implicit" | "synthesis_implicit" => {
            Some(AffixType::Implicit)
        }
        "enchantment" | "corrupted" => Some(AffixType::Enchantment),
        // "unique", "monster", and similar generation_types are not affixes
        // we model — return None to skip.
        _ => None,
    }
}

fn parse_generation_type_to_kind(s: &str) -> ModKind {
    match s {
        "implicit" | "exarch_implicit" | "eater_implicit" | "synthesis_implicit" => {
            ModKind::Implicit
        }
        "enchantment" => ModKind::Enchantment,
        "corrupted" => ModKind::Corrupted,
        _ => ModKind::Explicit,
    }
}

fn stat_from_repoe(s: &RepoeStat) -> ModStat {
    ModStat {
        stat_id: StatId::from(s.id.as_str()),
        min: s.min,
        max: s.max,
    }
}

fn derive_flags(raw: &RepoeMod, stats: &[ModStat]) -> ModFlags {
    let mut f = ModFlags::empty();
    if raw.is_essence_only {
        f |= ModFlags::ESSENCE_ONLY;
    }
    // Hybrid heuristic: more than one stat with a *different* concept stem.
    // Real concept-aware classification lands in M2.7. Until then we use a
    // string-prefix heuristic which catches obvious cases like ES + Life.
    if has_multiple_concepts_heuristic(stats) {
        f |= ModFlags::HYBRID;
    }
    f
}

/// Heuristic placeholder for the M2.7 concept-aware analyzer. Returns true
/// iff the mod's stats span ≥ 2 distinct concept *stems* (the part before
/// the first `_` after a known prefix).
fn has_multiple_concepts_heuristic(stats: &[ModStat]) -> bool {
    if stats.len() < 2 {
        return false;
    }
    // Two added-damage min/max stats that share a damage-element stem are NOT a hybrid.
    // E.g. minimum_added_fire_damage + maximum_added_fire_damage = 1 concept.
    let mut stems: AHashSet<&str> = AHashSet::with_capacity(stats.len());
    for s in stats {
        let stem = concept_stem_of(s.stat_id.as_str());
        stems.insert(stem);
    }
    stems.len() > 1
}

/// Coarse concept-stem extractor for the heuristic. A real `concept_map`
/// lookup replaces this in M2.7.
fn concept_stem_of(id: &str) -> &str {
    // Strip trailing modifiers like `_+%`, leading `local_` / `base_` /
    // `minimum_added_` / `maximum_added_` so we land on the "noun".
    let stripped = id
        .trim_start_matches("local_")
        .trim_start_matches("base_")
        .trim_start_matches("minimum_added_")
        .trim_start_matches("maximum_added_")
        .trim_start_matches("added_")
        .trim_start_matches("maximum_")
        .trim_start_matches("minimum_");
    let head = stripped.split('_').next().unwrap_or(stripped);
    if head.is_empty() {
        stripped
    } else {
        head
    }
}

/// Build the `tag → {class_id}` index from the bundle's bases.
///
/// For every `(base.item_class, tag in base.tags)` pair, register that
/// the tag carries the class. RePoE-fork mod `spawn_weights` reference
/// these tags (e.g. `body_armour`, `str_armour`, `weapon`,
/// `default`); the index lets [`derive_allowed_classes`] expand a
/// spawn-weight tag into the set of classes the mod can roll on.
fn build_tag_to_classes_index(
    bases: &[BaseType],
) -> std::collections::HashMap<String, SmallVec<[ItemClassId; 8]>> {
    let mut out: std::collections::HashMap<String, SmallVec<[ItemClassId; 8]>> =
        std::collections::HashMap::new();
    for b in bases {
        for tag in &b.tags {
            let entry = out.entry(tag.as_str().to_string()).or_default();
            if !entry.iter().any(|c| c == &b.item_class) {
                entry.push(b.item_class.clone());
            }
        }
        // The class id itself also acts as a tag — some downstream
        // consumers (and the v1 `derive_allowed_classes` behaviour)
        // expected exact id matches. Keeping this synonym preserves
        // back-compat for mods that reference the class name directly
        // in their spawn_weights.
        let class_str = b.item_class.as_str().to_string();
        let entry = out.entry(class_str).or_default();
        if !entry.iter().any(|c| c == &b.item_class) {
            entry.push(b.item_class.clone());
        }
    }
    out
}

fn derive_allowed_classes(
    weights: &[SpawnWeight],
    tag_to_classes: &std::collections::HashMap<String, SmallVec<[ItemClassId; 8]>>,
) -> SmallVec<[ItemClassId; 8]> {
    let mut out: SmallVec<[ItemClassId; 8]> = SmallVec::new();
    for sw in weights.iter().filter(|sw| sw.weight > 0) {
        let tag = sw.tag.as_str();
        if let Some(classes) = tag_to_classes.get(tag) {
            for c in classes {
                if !out.iter().any(|existing| existing == c) {
                    out.push(c.clone());
                }
            }
        }
    }
    out
}

fn lookup_class_tags(_tags: &[String]) -> std::collections::HashMap<String, SmallVec<[TagId; 4]>> {
    // Class-specific tags (e.g., "boots" on the Boots class) are derived
    // when we synthesize ItemClass entries. For the M2.3 minimum, we set
    // the class-name itself as a tag if it's also present in the tag list.
    // This is refined when poe2db scrape lands.
    std::collections::HashMap::new()
}

/// Synthesize a [`ConceptDefinition`] entry per distinct concept that
/// shows up in the analyzed mod list. Family classification is heuristic;
/// downstream UI consumers can refine via overrides.
fn collect_concepts(mods: &[ModDefinition]) -> Vec<poc2_data::ConceptDefinition> {
    let mut seen: AHashSet<String> = AHashSet::new();
    let mut out: Vec<poc2_data::ConceptDefinition> = Vec::new();
    for m in mods {
        for c in &m.concept_set {
            let id_str = c.as_str();
            if seen.insert(id_str.to_string()) {
                out.push(poc2_data::ConceptDefinition {
                    id: c.clone(),
                    display_name: humanize_concept(id_str),
                    family: concept_family(id_str).to_string(),
                });
            }
        }
    }
    out.sort_by(|a, b| a.id.as_str().cmp(b.id.as_str()));
    out
}

fn humanize_concept(id: &str) -> String {
    // CamelCase -> "Camel Case".
    let mut s = String::with_capacity(id.len() + 2);
    for (i, c) in id.chars().enumerate() {
        if i > 0 && c.is_ascii_uppercase() {
            s.push(' ');
        }
        s.push(c);
    }
    s
}

fn concept_family(id: &str) -> &'static str {
    match id {
        "Life" | "Mana" | "EnergyShield" => "Resource",
        "FireResistance"
        | "ColdResistance"
        | "LightningResistance"
        | "ChaosResistance"
        | "AllResistances"
        | "Armour"
        | "Evasion"
        | "Block" => "Defence",
        "Strength" | "Dexterity" | "Intelligence" | "AllAttributes" => "Attribute",
        "AttackSpeed" | "CastSpeed" | "MovementSpeed" | "ProjectileSpeed" => "Speed",
        "CritChance" | "CritDamage" => "Critical",
        "ItemRarity" | "ItemQuantity" => "Find",
        s if s.starts_with("Added") || s.starts_with("Increased") => "Damage",
        s if s.ends_with("SkillLevel") => "Skill",
        s if s.ends_with("Charges") => "Charges",
        _ => "Other",
    }
}

/// Derive `base_id → [mod_id]` by intersecting each base's tag set with each
/// mod's positive `spawn_weights`. We don't apply level gating here — the
/// advisor's eligibility check at query time already filters by `required_level`
/// and `allowed_item_classes`. This index is just a fast first cut.
fn derive_mods_by_base(
    bases: &[BaseType],
    mods: &[ModDefinition],
) -> IndexMap<String, Vec<String>> {
    // Build, per mod, the set of tags with weight > 0.
    let mod_eligible_tags: Vec<(&str, AHashSet<&str>)> = mods
        .iter()
        .map(|m| {
            let tags: AHashSet<&str> = m
                .spawn_weights
                .iter()
                .filter(|sw| sw.weight > 0)
                .map(|sw| sw.tag.as_str())
                .collect();
            (m.id.as_str(), tags)
        })
        .collect();

    let mut out: IndexMap<String, Vec<String>> = IndexMap::with_capacity(bases.len());
    for base in bases {
        let base_tags: AHashSet<&str> = base.tags.iter().map(TagId::as_str).collect();
        let mut eligible: Vec<String> = Vec::new();
        for (mod_id, mod_tags) in &mod_eligible_tags {
            if mod_tags.iter().any(|t| base_tags.contains(*t)) {
                eligible.push((*mod_id).to_string());
            }
        }
        if !eligible.is_empty() {
            out.insert(base.id.to_string(), eligible);
        }
    }
    out
}

struct ClassCaps {
    implicits: u8,
    prefixes: u8,
    suffixes: u8,
    sockets: u8,
}

/// Heuristic slot caps per item class. Refined in later passes.
#[allow(clippy::match_same_arms)]
fn class_caps(class: &str) -> ClassCaps {
    let (sockets, max_implicits) = match class {
        // No sockets, no implicit: caster weapons, foci, quivers
        "Wand" | "Staff" | "Sceptre" | "Focus" | "Quiver" => (0, 0),
        // Jewelry: implicits but no sockets
        "Ring" | "Amulet" | "Belt" => (0, 1),
        // Armour with 1 socket
        "Helmet" | "Gloves" | "Boots" => (1, 0),
        // Armour with 2 sockets
        "BodyArmour" => (2, 0),
        // Two-hand martial weapons
        "TwoHandSword" | "TwoHandAxe" | "TwoHandMace" | "Bow" | "Crossbow" | "Quarterstaff"
        | "Spear" | "Talisman" => (2, 0),
        // One-hand martial weapons
        _ if class.starts_with("OneHand") => (1, 0),
        _ => (0, 0),
    };
    ClassCaps {
        implicits: max_implicits,
        prefixes: 3,
        suffixes: 3,
        sockets,
    }
}

fn human_class_name(id: &str) -> String {
    // Insert spaces before capital letters in CamelCase.
    let mut out = String::with_capacity(id.len() + 4);
    for (i, c) in id.chars().enumerate() {
        if i > 0 && c.is_ascii_uppercase() {
            out.push(' ');
        }
        out.push(c);
    }
    out
}

/// Heuristic attribute-pool derivation from a base's tag set.
fn derive_attribute_pool(tags: &[String]) -> AttributePool {
    let mut s = false;
    let mut d = false;
    let mut i = false;
    for t in tags {
        match t.as_str() {
            "str_armour" => s = true,
            "dex_armour" => d = true,
            "int_armour" => i = true,
            "str_dex_armour" => {
                s = true;
                d = true;
            }
            "str_int_armour" => {
                s = true;
                i = true;
            }
            "dex_int_armour" => {
                d = true;
                i = true;
            }
            "str_dex_int_armour" => {
                s = true;
                d = true;
                i = true;
            }
            _ => {}
        }
    }
    match (s, d, i) {
        (true, true, true) => AttributePool::StrDexInt,
        (true, true, false) => AttributePool::StrDex,
        (true, false, true) => AttributePool::StrInt,
        (false, true, true) => AttributePool::DexInt,
        (true, false, false) => AttributePool::Str,
        (false, true, false) => AttributePool::Dex,
        (false, false, true) => AttributePool::Int,
        (false, false, false) => {
            // Many bases (rings/amulets/quivers/foci/weapons) have no
            // armour-tag — that's expected, not a warning.
            debug!("base has no attribute-pool tag; defaulting to None");
            AttributePool::None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_class_inserts_spaces() {
        assert_eq!(human_class_name("BodyArmour"), "Body Armour");
        assert_eq!(human_class_name("OneHandSword"), "One Hand Sword");
        assert_eq!(human_class_name("Boots"), "Boots");
    }

    #[test]
    fn parse_generation_type() {
        assert_eq!(
            parse_generation_type_to_affix("prefix"),
            Some(AffixType::Prefix)
        );
        assert_eq!(
            parse_generation_type_to_affix("suffix"),
            Some(AffixType::Suffix)
        );
        assert_eq!(
            parse_generation_type_to_affix("implicit"),
            Some(AffixType::Implicit)
        );
        assert_eq!(parse_generation_type_to_affix("monster"), None);
    }

    #[test]
    fn verisium_unique_bases_forced_to_unique() {
        // Runeforged unique variants override whatever RePoE exports.
        assert_eq!(
            derive_release_state(
                "Metadata/Items/Amulets/FourAmuletUnique1VerisiumUnique3",
                "released"
            ),
            ReleaseState::Unique
        );
        assert_eq!(
            derive_release_state(
                "Metadata/Items/Weapons/OneHandWeapons/OneHandMaces/FourOneHandMace13VerisiumUnique1",
                "unreleased"
            ),
            ReleaseState::Unique
        );
        // Ordinary bases keep the raw release_state mapping.
        assert_eq!(
            derive_release_state("Metadata/Items/Amulets/Amulet1", "released"),
            ReleaseState::Released
        );
        assert_eq!(
            derive_release_state("Metadata/Items/Amulets/Amulet2", "unique_only"),
            ReleaseState::Unique
        );
    }

    #[test]
    fn attribute_pool_derivation() {
        assert_eq!(
            derive_attribute_pool(&["int_armour".to_string()]),
            AttributePool::Int
        );
        assert_eq!(
            derive_attribute_pool(&["str_int_armour".to_string()]),
            AttributePool::StrInt
        );
        assert_eq!(
            derive_attribute_pool(&["boots".to_string()]),
            AttributePool::None
        );
    }

    #[test]
    fn hybrid_heuristic_distinguishes_real_hybrids() {
        // Real hybrid: ES + Life
        let stats = vec![
            ModStat {
                stat_id: "local_energy_shield_+%".into(),
                min: 0.0,
                max: 0.0,
            },
            ModStat {
                stat_id: "base_maximum_life".into(),
                min: 0.0,
                max: 0.0,
            },
        ];
        assert!(has_multiple_concepts_heuristic(&stats));

        // Non-hybrid: added fire damage min/max
        let stats = vec![
            ModStat {
                stat_id: "minimum_added_fire_damage".into(),
                min: 0.0,
                max: 0.0,
            },
            ModStat {
                stat_id: "maximum_added_fire_damage".into(),
                min: 0.0,
                max: 0.0,
            },
        ];
        assert!(!has_multiple_concepts_heuristic(&stats));
    }
}
