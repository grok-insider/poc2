//! `database` command — browse craftable base items and crafting materials.
//!
//! Ported from the Tauri desktop `list_bases` / `list_database_entries` /
//! `database_entry_detail` commands. Pure compute over a [`poc2_data::Bundle`]:
//! it reads `bundle.base_items` plus the `omens` / `essences` / `bones` /
//! `catalysts` / `currencies` sections and produces serializable view structs.
//!
//! Icon/asset URL fields (`icon_url`, `detail_url`) are kept in the view contract
//! for TS parity but are always `None` here — fetching/serving images is a browser
//! concern, not engine compute.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------
// Args
// ---------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DatabaseSection {
    Bases,
    Materials,
}

#[derive(Debug, Deserialize)]
pub struct DatabaseListArgs {
    pub section: DatabaseSection,
    #[serde(default)]
    pub search: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct DatabaseDetailArgs {
    pub section: DatabaseSection,
    pub id: String,
}

#[derive(Debug, Deserialize)]
pub struct BasesArgs {
    /// PascalCase class id like "BodyArmour". When `None`, returns every base.
    #[serde(default)]
    pub class_pascal: Option<String>,
    /// Include legacy/unreleased bases. Defaults to false.
    #[serde(default)]
    pub include_legacy: bool,
}

// ---------------------------------------------------------------------
// View structs (serde field names must match the desktop / TS contract)
// ---------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct BaseSummary {
    pub id: String,
    pub name: String,
    pub class_pascal: String,
    pub class_display: String,
    pub drop_level: u32,
    pub attribute_pool: String,
    pub tags: Vec<String>,
    pub release_state: String,
}

#[derive(Debug, Serialize)]
pub struct DatabaseEntrySummary {
    pub id: String,
    pub name: String,
    pub section: String,
    pub category: String,
    pub kind: String,
    pub icon_url: Option<String>,
    pub detail_url: Option<String>,
    pub tags: Vec<String>,
    pub description: Option<String>,
    pub base: Option<BaseSummary>,
}

#[derive(Debug, Serialize)]
pub struct DatabaseEntryDetail {
    pub summary: DatabaseEntrySummary,
    pub base: Option<DatabaseBaseDetail>,
    pub material: Option<DatabaseMaterialDetail>,
}

#[derive(Debug, Serialize)]
pub struct DatabaseBaseDetail {
    pub metadata_type: String,
    pub drop_level: u32,
    pub class_display: String,
    pub attribute_pool: String,
    pub inventory_width: u8,
    pub inventory_height: u8,
    pub tags: Vec<String>,
    pub derived_stats: Vec<DatabaseStatLine>,
    pub requirements: Vec<String>,
    pub granted_effects: Vec<DatabaseStatLine>,
    pub class_notes: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct DatabaseMaterialDetail {
    pub source_section: String,
    pub description: String,
    pub applies_to: Vec<String>,
    pub tags: Vec<String>,
    pub raw_fields: Vec<DatabaseStatLine>,
}

#[derive(Debug, Serialize)]
pub struct DatabaseStatLine {
    pub label: String,
    pub value: String,
    pub help: Option<String>,
}

// ---------------------------------------------------------------------
// Public free functions — the command entry points
// ---------------------------------------------------------------------

/// List craftable base items the user can pick from, optionally filtered by
/// PascalCase class id and including legacy/unreleased bases.
pub fn list_bases(bundle: &poc2_data::Bundle, args: &BasesArgs) -> Vec<BaseSummary> {
    let mut out = Vec::with_capacity(bundle.base_items.len());
    for base in &bundle.base_items {
        if !is_inspectable_base(base) {
            continue;
        }
        let summary = base_summary(base);
        let pascal = summary.class_pascal.clone();
        if let Some(filter) = &args.class_pascal {
            if filter != &pascal {
                continue;
            }
        }
        if !args.include_legacy
            && !matches!(
                base.release_state,
                poc2_engine::base::ReleaseState::Released
            )
        {
            continue;
        }
        out.push(summary);
    }
    out.sort_by(|a, b| a.drop_level.cmp(&b.drop_level).then(a.name.cmp(&b.name)));
    out
}

/// List database entries for the requested section, optionally filtered by a
/// free-text search query (matched against name/category/kind/id/description/tags).
pub fn list_database_entries(
    bundle: &poc2_data::Bundle,
    args: &DatabaseListArgs,
) -> Vec<DatabaseEntrySummary> {
    let mut entries = match args.section {
        DatabaseSection::Bases => database_base_summaries(bundle),
        DatabaseSection::Materials => database_material_summaries(bundle),
    };
    if let Some(search) = args
        .search
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        let q = search.to_ascii_lowercase();
        entries.retain(|entry| database_entry_matches(entry, &q));
    }
    entries
}

/// Resolve the full detail view for a single database entry.
pub fn database_entry_detail(
    bundle: &poc2_data::Bundle,
    args: &DatabaseDetailArgs,
) -> Result<DatabaseEntryDetail, String> {
    match args.section {
        DatabaseSection::Bases => {
            let base = bundle
                .base_items
                .iter()
                .find(|base| base.id.as_str() == args.id && is_inspectable_base(base))
                .ok_or_else(|| format!("unknown database base {}", args.id))?;
            let summary = database_base_summary(base);
            Ok(DatabaseEntryDetail {
                summary,
                base: Some(database_base_detail(base)),
                material: None,
            })
        }
        DatabaseSection::Materials => database_material_detail(bundle, &args.id)
            .ok_or_else(|| format!("unknown database material {}", args.id)),
    }
}

// ---------------------------------------------------------------------
// Bases
// ---------------------------------------------------------------------

fn database_base_summaries(bundle: &poc2_data::Bundle) -> Vec<DatabaseEntrySummary> {
    let mut out: Vec<_> = bundle
        .base_items
        .iter()
        .filter(|base| is_inspectable_base(base))
        .map(database_base_summary)
        .collect();
    out.sort_by(|a, b| {
        a.base
            .as_ref()
            .map_or(0, |base| base.drop_level)
            .cmp(&b.base.as_ref().map_or(0, |base| base.drop_level))
            .then(a.name.cmp(&b.name))
    });
    out
}

fn database_base_summary(base: &poc2_engine::BaseType) -> DatabaseEntrySummary {
    let base = base_summary(base);
    DatabaseEntrySummary {
        id: base.id.clone(),
        name: base.name.clone(),
        section: "bases".into(),
        category: base.class_display.clone(),
        kind: base.class_pascal.clone(),
        icon_url: None,
        detail_url: None,
        tags: base.tags.clone(),
        description: Some(format!(
            "{} base item, drop level {}, {} attribute pool.",
            base.class_display, base.drop_level, base.attribute_pool
        )),
        base: Some(base),
    }
}

fn database_base_detail(base: &poc2_engine::BaseType) -> DatabaseBaseDetail {
    let class_display = human_class_name(base.item_class.as_str());
    DatabaseBaseDetail {
        metadata_type: base.id.as_str().to_string(),
        drop_level: base.drop_level,
        class_display,
        attribute_pool: format!("{:?}", base.attribute_pool).to_ascii_lowercase(),
        inventory_width: base.inventory.width,
        inventory_height: base.inventory.height,
        tags: base.tags.iter().map(|t| t.as_str().to_string()).collect(),
        derived_stats: derived_base_stats(base),
        requirements: derived_requirements(base),
        granted_effects: derived_granted_effects(base),
        class_notes: derived_class_notes(base),
    }
}

fn base_summary(base: &poc2_engine::BaseType) -> BaseSummary {
    let display = human_class_name(base.item_class.as_str());
    let pascal = base.item_class.as_str().to_string();
    BaseSummary {
        id: base.id.as_str().to_string(),
        name: base.name.clone(),
        class_pascal: pascal,
        class_display: display,
        drop_level: base.drop_level,
        attribute_pool: format!("{:?}", base.attribute_pool).to_ascii_lowercase(),
        tags: base.tags.iter().map(|t| t.as_str().to_string()).collect(),
        release_state: format!("{:?}", base.release_state).to_ascii_lowercase(),
    }
}

// ---------------------------------------------------------------------
// Materials
// ---------------------------------------------------------------------

fn database_material_summaries(bundle: &poc2_data::Bundle) -> Vec<DatabaseEntrySummary> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for (id, name) in known_currency_assets() {
        if seen.insert(id.to_string()) {
            out.push(material_summary(
                id,
                name,
                "currency",
                known_material_description(id, name),
                Vec::new(),
                None,
            ));
        }
    }
    push_material_section(&mut out, &mut seen, &bundle.omens.entries, "omen");
    push_material_section(&mut out, &mut seen, &bundle.essences.entries, "essence");
    push_material_section(&mut out, &mut seen, &bundle.bones.entries, "bone");
    push_material_section(&mut out, &mut seen, &bundle.catalysts.entries, "catalyst");
    push_material_section(&mut out, &mut seen, &bundle.currencies.entries, "currency");
    out.sort_by(|a, b| a.category.cmp(&b.category).then(a.name.cmp(&b.name)));
    out
}

fn push_material_section(
    out: &mut Vec<DatabaseEntrySummary>,
    seen: &mut HashSet<String>,
    entries: &[serde_json::Value],
    kind: &str,
) {
    for entry in entries {
        let Some(name) = entry.get("name").and_then(serde_json::Value::as_str) else {
            continue;
        };
        let id = entry
            .get("id")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| slug_name(name));
        if !is_inspectable_material(kind, &id, name) || !seen.insert(id.clone()) {
            continue;
        }
        out.push(material_summary(
            &id,
            name,
            kind,
            material_description_from_json(entry)
                .unwrap_or_else(|| known_material_description(&id, name)),
            tags_from_json(entry),
            None,
        ));
    }
}

fn material_summary(
    id: &str,
    name: &str,
    kind: &str,
    description: String,
    tags: Vec<String>,
    icon_url: Option<String>,
) -> DatabaseEntrySummary {
    DatabaseEntrySummary {
        id: id.to_string(),
        name: name.to_string(),
        section: "materials".into(),
        category: material_category(kind),
        kind: kind.to_string(),
        icon_url,
        detail_url: None,
        tags,
        description: Some(description),
        base: None,
    }
}

fn database_material_detail(bundle: &poc2_data::Bundle, id: &str) -> Option<DatabaseEntryDetail> {
    let summaries = database_material_summaries(bundle);
    let summary = summaries.into_iter().find(|entry| entry.id == id)?;
    let raw = find_material_json(bundle, id, &summary.name, &summary.kind);
    let description = raw
        .and_then(material_description_from_json)
        .or_else(|| summary.description.clone())
        .unwrap_or_else(|| known_material_description(&summary.id, &summary.name));
    let tags = raw
        .map(tags_from_json)
        .filter(|tags| !tags.is_empty())
        .unwrap_or_else(|| summary.tags.clone());
    Some(DatabaseEntryDetail {
        material: Some(DatabaseMaterialDetail {
            source_section: summary.kind.clone(),
            description,
            applies_to: material_applies_to(&summary.id, &summary.kind),
            tags,
            raw_fields: raw.map(raw_json_fields).unwrap_or_default(),
        }),
        summary,
        base: None,
    })
}

fn find_material_json<'a>(
    bundle: &'a poc2_data::Bundle,
    id: &str,
    name: &str,
    kind: &str,
) -> Option<&'a serde_json::Value> {
    let entries = match kind {
        "omen" => &bundle.omens.entries,
        "essence" => &bundle.essences.entries,
        "bone" => &bundle.bones.entries,
        "catalyst" => &bundle.catalysts.entries,
        "currency" => &bundle.currencies.entries,
        _ => return None,
    };
    entries.iter().find(|entry| {
        entry.get("id").and_then(serde_json::Value::as_str) == Some(id)
            || entry.get("name").and_then(serde_json::Value::as_str) == Some(name)
    })
}

// ---------------------------------------------------------------------
// Search / predicates
// ---------------------------------------------------------------------

fn database_entry_matches(entry: &DatabaseEntrySummary, q: &str) -> bool {
    entry.name.to_ascii_lowercase().contains(q)
        || entry.category.to_ascii_lowercase().contains(q)
        || entry.kind.to_ascii_lowercase().contains(q)
        || entry.id.to_ascii_lowercase().contains(q)
        || entry
            .description
            .as_deref()
            .unwrap_or_default()
            .to_ascii_lowercase()
            .contains(q)
        || entry
            .tags
            .iter()
            .any(|tag| tag.to_ascii_lowercase().contains(q))
}

fn is_inspectable_base(base: &poc2_engine::BaseType) -> bool {
    matches!(
        base.release_state,
        poc2_engine::base::ReleaseState::Released
    ) && is_inspectable_base_class(base.item_class.as_str())
        && !is_known_noncraft_base(base)
}

fn is_known_noncraft_base(base: &poc2_engine::BaseType) -> bool {
    // RePoE currently carries some unique placeholders, PoE1 carryovers,
    // and deprecated bases that still have valid item classes. Keep them
    // out until the refreshed local item DB has authoritative craftability.
    matches!(
        base.name.as_str(),
        "Golden Hoop" | "Ring" | "Abyssal Signet" | "Timeless Jewel" | "Diamond"
    ) || matches!(
        base.id.as_str(),
        "Metadata/Items/Rings/Ring" | "Metadata/Items/Jewels/TimelessJewel"
    )
}

fn is_inspectable_base_class(class: &str) -> bool {
    matches!(
        class,
        "OneHandSword"
            | "TwoHandSword"
            | "OneHandAxe"
            | "TwoHandAxe"
            | "OneHandMace"
            | "TwoHandMace"
            | "Bow"
            | "Crossbow"
            | "Spear"
            | "Flail"
            | "Staff"
            | "Warstaff"
            | "Quarterstaff"
            | "Sceptre"
            | "Wand"
            | "Dagger"
            | "Claw"
            | "Shield"
            | "Focus"
            | "Helmet"
            | "Boots"
            | "Gloves"
            | "Belt"
            | "Ring"
            | "Amulet"
            | "BodyArmour"
            | "Jewel"
    )
}

fn is_inspectable_material(kind: &str, id: &str, name: &str) -> bool {
    let haystack = format!("{} {} {}", kind, id, name).to_ascii_lowercase();
    if [
        "skillgem",
        "flask",
        "charm",
        "key",
        "contract",
        "blueprint",
        "logbook",
        "treasure",
        "incubator",
        "sanctum",
    ]
    .iter()
    .any(|blocked| haystack.contains(blocked))
    {
        return false;
    }
    matches!(
        kind,
        "currency" | "omen" | "essence" | "catalyst" | "bone" | "soul_core" | "rune"
    )
}

fn material_category(kind: &str) -> String {
    match kind {
        "omen" => "Omen",
        "essence" => "Essence",
        "bone" => "Abyssal Bone",
        "catalyst" => "Catalyst",
        "currency" => "Currency",
        "soul_core" => "Soul Core",
        "rune" => "Rune",
        other => other,
    }
    .to_string()
}

fn material_description_from_json(entry: &serde_json::Value) -> Option<String> {
    ["tooltip", "description", "effect", "text"]
        .iter()
        .find_map(|key| entry.get(*key).and_then(serde_json::Value::as_str))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn tags_from_json(entry: &serde_json::Value) -> Vec<String> {
    entry
        .get("tags")
        .and_then(serde_json::Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|value| value.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn raw_json_fields(entry: &serde_json::Value) -> Vec<DatabaseStatLine> {
    let Some(obj) = entry.as_object() else {
        return Vec::new();
    };
    obj.iter()
        .filter(|(key, value)| {
            matches!(
                key.as_str(),
                "id" | "name" | "corrupt" | "tags" | "tooltip" | "description"
            ) && !value.is_null()
        })
        .map(|(key, value)| DatabaseStatLine {
            label: key.clone(),
            value: value
                .as_str()
                .map(str::to_string)
                .unwrap_or_else(|| value.to_string()),
            help: None,
        })
        .collect()
}

fn known_material_description(id: &str, name: &str) -> String {
    match id {
        "OrbOfTransmutation" | "GreaterOrbOfTransmutation" | "PerfectOrbOfTransmutation" => {
            format!("{name} upgrades a normal item into a magic item.")
        }
        "OrbOfAugmentation" | "GreaterOrbOfAugmentation" | "PerfectOrbOfAugmentation" => {
            format!("{name} adds a modifier to a magic item with an open affix slot.")
        }
        "RegalOrb" | "GreaterRegalOrb" | "PerfectRegalOrb" => {
            format!("{name} upgrades a magic item into a rare item.")
        }
        "OrbOfAlchemy" => "Orb of Alchemy upgrades a normal item into a rare item.".into(),
        "ExaltedOrb" | "GreaterExaltedOrb" | "PerfectExaltedOrb" => {
            format!("{name} adds a modifier to a rare item with an open affix slot.")
        }
        "OrbOfAnnulment" => "Orb of Annulment removes a random modifier from an item.".into(),
        "ChaosOrb" | "GreaterChaosOrb" | "PerfectChaosOrb" => {
            format!("{name} reforges modifiers on a rare item.")
        }
        "DivineOrb" => "Divine Orb rerolls modifier values within their existing tiers.".into(),
        "VaalOrb" => "Vaal Orb corrupts an item, causing an unpredictable crafting outcome.".into(),
        "HinekorasLock" => {
            "Hinekora's Lock previews the next crafting outcome before committing it.".into()
        }
        "FracturingOrb" => "Fracturing Orb locks one modifier in place by fracturing it.".into(),
        _ if id.starts_with("Refined") && id.ends_with("Catalyst") => {
            format!("{name} adds tagged quality that enhances matching modifiers on a jewel.")
        }
        _ if id.ends_with("Catalyst") => {
            format!(
                "{name} adds tagged quality that enhances matching modifiers on a ring or amulet."
            )
        }
        _ => format!("{name} is a crafting material used by the advisor."),
    }
}

fn material_applies_to(id: &str, kind: &str) -> Vec<String> {
    // Catalysts arrive via the bundle section ("catalyst" kind) or the
    // known-asset list ("currency" kind); gate by id either way. PoE2 0.5:
    // base catalysts apply to a ring or amulet only, Refined variants to
    // a jewel only — belts take no catalyst (poe2db catalysts.html).
    if kind == "catalyst" || id.ends_with("Catalyst") {
        return if id.starts_with("Refined") {
            vec!["Jewel".into()]
        } else {
            vec!["Ring".into(), "Amulet".into()]
        };
    }
    match kind {
        "bone" => vec![
            "Armour".into(),
            "Weapons".into(),
            "Jewellery".into(),
            "Jewel".into(),
        ],
        "essence" => vec!["Base items".into()],
        "omen" => vec!["Currency actions".into()],
        "currency" if id == "HinekorasLock" => vec!["Next craft action".into()],
        "currency" => vec!["Craftable items".into()],
        _ => Vec::new(),
    }
}

// ---------------------------------------------------------------------
// Derived base stats (heuristics — no external item DB yet)
// ---------------------------------------------------------------------

fn derived_base_stats(base: &poc2_engine::BaseType) -> Vec<DatabaseStatLine> {
    let class = base.item_class.as_str();
    let pool = format!("{:?}", base.attribute_pool).to_ascii_lowercase();
    let mut out = Vec::new();
    if class == "Sceptre" {
        out.push(DatabaseStatLine {
            label: "Spirit".into(),
            value: "100".into(),
            help: Some(glossary_help("Spirit")),
        });
        return out;
    }
    if matches!(
        class,
        "BodyArmour" | "Helmet" | "Boots" | "Gloves" | "Shield"
    ) {
        if pool.contains("str") {
            out.push(helped_stat("Armour", "base defensive stat"));
        }
        if pool.contains("dex") {
            out.push(helped_stat("Evasion", "base defensive stat"));
        }
        if pool.contains("int") {
            out.push(helped_stat("Energy Shield", "base defensive stat"));
        }
    }
    if class == "BodyArmour" {
        out.push(DatabaseStatLine {
            label: "Base Movement Speed".into(),
            value: "varies by base".into(),
            help: Some(
                "Exact local base movement speed will come from the refreshed item database."
                    .into(),
            ),
        });
    }
    out
}

fn derived_granted_effects(base: &poc2_engine::BaseType) -> Vec<DatabaseStatLine> {
    if base.item_class.as_str() != "Sceptre" {
        return Vec::new();
    }
    if base.implicits.is_empty() {
        return vec![DatabaseStatLine {
            label: "Grants Skill".into(),
            value: "varies by base".into(),
            help: Some(
                "Exact granted skill names will come from the refreshed local item database."
                    .into(),
            ),
        }];
    }
    base.implicits
        .iter()
        .map(|implicit| DatabaseStatLine {
            label: "Implicit".into(),
            value: implicit.as_str().to_string(),
            help: Some(
                "Sceptre implicits represent the granted skill/effect carried by that base.".into(),
            ),
        })
        .collect()
}

fn derived_class_notes(base: &poc2_engine::BaseType) -> Vec<String> {
    if base.item_class.as_str() != "Sceptre" {
        return Vec::new();
    }
    vec![
        "Sceptres are one-handed weapons that require Strength and Intelligence to equip.".into(),
        "They can be equipped in your main hand or off hand, but you cannot dual wield two Sceptres.".into(),
        "Sceptres cannot be used to Attack and do not grant bonuses to Spellcasting. Instead, they grant Spirit and can provide bonuses to allies.".into(),
    ]
}

fn helped_stat(label: &str, value: &str) -> DatabaseStatLine {
    DatabaseStatLine {
        label: label.into(),
        value: value.into(),
        help: Some(glossary_help(label)),
    }
}

fn derived_requirements(base: &poc2_engine::BaseType) -> Vec<String> {
    let mut out = vec![format!("Level {}", base.drop_level)];
    let pool = format!("{:?}", base.attribute_pool).to_ascii_lowercase();
    if pool.contains("str") {
        out.push("Strength requirement varies by base".into());
    }
    if pool.contains("dex") {
        out.push("Dexterity requirement varies by base".into());
    }
    if pool.contains("int") {
        out.push("Intelligence requirement varies by base".into());
    }
    out
}

fn glossary_help(label: &str) -> String {
    match label {
        "Energy Shield" => "Energy Shield protects Life by taking damage first and rapidly recharges after avoiding damage.".into(),
        "Armour" => "Armour mitigates physical hit damage. Larger hits require more Armour to mitigate effectively.".into(),
        "Evasion" => "Evasion gives a chance to avoid attacks before they hit.".into(),
        "Spirit" => "Spirit reserves persistent skills, minions, and buffs. Sceptres grant a fixed Spirit baseline.".into(),
        _ => "Local glossary entry.".into(),
    }
}

fn human_class_name(raw: &str) -> String {
    match raw {
        "BodyArmour" => "Body Armour".into(),
        "OneHandSword" => "One Hand Sword".into(),
        "TwoHandSword" => "Two Hand Sword".into(),
        "OneHandAxe" => "One Hand Axe".into(),
        "TwoHandAxe" => "Two Hand Axe".into(),
        "OneHandMace" => "One Hand Mace".into(),
        "TwoHandMace" => "Two Hand Mace".into(),
        "Sceptre" => "Sceptres".into(),
        other => {
            let mut out = String::new();
            for (i, c) in other.chars().enumerate() {
                if i > 0 && c.is_ascii_uppercase() {
                    out.push(' ');
                }
                out.push(c);
            }
            out
        }
    }
}

fn slug_name(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c
            } else if c.is_whitespace() || c == '-' || c == '_' {
                '_'
            } else {
                '\0'
            }
        })
        .filter(|c| *c != '\0')
        .collect::<String>()
        .split('_')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("_")
}

fn known_currency_assets() -> Vec<(&'static str, &'static str)> {
    vec![
        ("OrbOfTransmutation", "Orb of Transmutation"),
        ("GreaterOrbOfTransmutation", "Greater Orb of Transmutation"),
        ("PerfectOrbOfTransmutation", "Perfect Orb of Transmutation"),
        ("OrbOfAugmentation", "Orb of Augmentation"),
        ("GreaterOrbOfAugmentation", "Greater Orb of Augmentation"),
        ("PerfectOrbOfAugmentation", "Perfect Orb of Augmentation"),
        ("RegalOrb", "Regal Orb"),
        ("GreaterRegalOrb", "Greater Regal Orb"),
        ("PerfectRegalOrb", "Perfect Regal Orb"),
        ("OrbOfAlchemy", "Orb of Alchemy"),
        ("ExaltedOrb", "Exalted Orb"),
        ("GreaterExaltedOrb", "Greater Exalted Orb"),
        ("PerfectExaltedOrb", "Perfect Exalted Orb"),
        ("OrbOfAnnulment", "Orb of Annulment"),
        ("ChaosOrb", "Chaos Orb"),
        ("GreaterChaosOrb", "Greater Chaos Orb"),
        ("PerfectChaosOrb", "Perfect Chaos Orb"),
        ("DivineOrb", "Divine Orb"),
        ("VaalOrb", "Vaal Orb"),
        ("HinekorasLock", "Hinekora's Lock"),
        ("FracturingOrb", "Fracturing Orb"),
        // PoE2 0.5 catalysts (poe2db catalysts.html): 12 base kinds for
        // rings/amulets + 12 Refined variants for jewels. The PoE1 names
        // "Intrinsic Catalyst" / "Unstable Catalyst" do not exist in PoE2.
        ("FleshCatalyst", "Flesh Catalyst"),
        ("NeuralCatalyst", "Neural Catalyst"),
        ("CarapaceCatalyst", "Carapace Catalyst"),
        ("UulNetolsCatalyst", "Uul-Netol's Catalyst"),
        ("XophsCatalyst", "Xoph's Catalyst"),
        ("TulsCatalyst", "Tul's Catalyst"),
        ("EshsCatalyst", "Esh's Catalyst"),
        ("ChayulasCatalyst", "Chayula's Catalyst"),
        ("ReaverCatalyst", "Reaver Catalyst"),
        ("SibilantCatalyst", "Sibilant Catalyst"),
        ("SkitteringCatalyst", "Skittering Catalyst"),
        ("AdaptiveCatalyst", "Adaptive Catalyst"),
        ("RefinedFleshCatalyst", "Refined Flesh Catalyst"),
        ("RefinedNeuralCatalyst", "Refined Neural Catalyst"),
        ("RefinedCarapaceCatalyst", "Refined Carapace Catalyst"),
        ("RefinedUulNetolsCatalyst", "Refined Uul-Netol's Catalyst"),
        ("RefinedXophsCatalyst", "Refined Xoph's Catalyst"),
        ("RefinedTulsCatalyst", "Refined Tul's Catalyst"),
        ("RefinedEshsCatalyst", "Refined Esh's Catalyst"),
        ("RefinedChayulasCatalyst", "Refined Chayula's Catalyst"),
        ("RefinedReaverCatalyst", "Refined Reaver Catalyst"),
        ("RefinedSibilantCatalyst", "Refined Sibilant Catalyst"),
        ("RefinedSkitteringCatalyst", "Refined Skittering Catalyst"),
        ("RefinedAdaptiveCatalyst", "Refined Adaptive Catalyst"),
    ]
}
