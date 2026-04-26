//! Phase 2 — lower a [`ParsedItem`] into an engine [`Item`].
//!
//! Mod text → mod id resolution requires the [`ModRegistry`]. Each
//! parsed mod line is matched against `ModDefinition::text_template`
//! (case-insensitive substring match for now; M5 polish replaces this
//! with proper `{0}` placeholder template parsing once the data
//! pipeline emits real templates).
//!
//! Mods that fail to resolve are logged and dropped; the engine still
//! gets a valid [`Item`] so the advisor can run, just with reduced mod
//! fidelity.
//!
//! Affix split (prefix vs suffix) is taken from each resolved
//! [`ModDefinition::affix_type`].

use poc2_engine::ids::{BaseTypeId, ItemClassId, ModId};
use poc2_engine::item::{AffixType, Item, ModRoll, QualityKind};
use poc2_engine::mods::{ModDefinition, ModKind};
use poc2_engine::registry::ModRegistry;
use smallvec::SmallVec;
use thiserror::Error;

use crate::text::{ModLine, ParsedItem};

#[derive(Debug, Error)]
pub enum LowerError {
    #[error("parsed item has no item class")]
    NoItemClass,
}

/// Lower a [`ParsedItem`] into an engine [`Item`].
///
/// Returns the [`Item`] plus a list of mod text strings that did NOT
/// resolve to any registered mod — useful for surfacing parser
/// coverage gaps in the UI.
///
/// Returns a `Result` for forward-compat: future versions will validate
/// the parsed base against a base registry and may reject items whose
/// base is unknown.
#[allow(clippy::unnecessary_wraps)]
pub fn lower_to_item(
    parsed: &ParsedItem,
    registry: &ModRegistry,
) -> Result<(Item, Vec<String>), LowerError> {
    let class = item_class_id_from_text(&parsed.item_class);
    let mut prefixes: SmallVec<[ModRoll; 3]> = SmallVec::new();
    let mut suffixes: SmallVec<[ModRoll; 3]> = SmallVec::new();
    let mut implicits: SmallVec<[ModRoll; 2]> = SmallVec::new();
    let mut unresolved: Vec<String> = Vec::new();

    for line in &parsed.implicits {
        if let Some(roll) = resolve_mod_line(line, registry, &class, ModKind::Implicit) {
            implicits.push(roll);
        } else {
            unresolved.push(line.text.clone());
        }
    }

    for line in &parsed.explicits {
        // Crafted mods are kind=Explicit but the engine doesn't yet have
        // a "crafted" flag; preserve as Explicit. Fractured maps to is_fractured.
        let kind = if line.implicit_tag {
            ModKind::Implicit
        } else {
            ModKind::Explicit
        };
        let Some(mut roll) = resolve_mod_line(line, registry, &class, kind) else {
            unresolved.push(line.text.clone());
            continue;
        };
        if line.fractured {
            roll.is_fractured = true;
        }
        match roll.affix_type {
            AffixType::Prefix => prefixes.push(roll),
            AffixType::Suffix => suffixes.push(roll),
            AffixType::Implicit => implicits.push(roll),
            AffixType::Enchantment => {}
        }
    }

    let item = Item {
        base: BaseTypeId::from(parsed.base.as_str()),
        ilvl: parsed.ilvl,
        rarity: parsed.rarity,
        corrupted: parsed.corrupted,
        sanctified: parsed.sanctified,
        mirrored: parsed.mirrored,
        quality: parsed.quality,
        quality_kind: QualityKind::Untagged,
        implicits,
        prefixes,
        suffixes,
        enchantments: SmallVec::new(),
        hidden_desecrated: None,
        sockets: SmallVec::new(),
        hinekora_lock: None,
    };
    Ok((item, unresolved))
}

/// Convert PoE2 item-class text (e.g., `"Body Armours"`) to our
/// internal [`ItemClassId`] (e.g., `"BodyArmour"`).
///
/// Naive normalization: strip plural `s` and remove spaces. Real
/// production would use a static lookup table built from the bundle.
#[must_use]
pub fn item_class_id_from_text(class: &str) -> ItemClassId {
    let normalized: String = class
        .split_whitespace()
        .map(|w| {
            let stripped = w.strip_suffix('s').unwrap_or(w);
            // Capitalize first letter just in case the input is lowercase.
            let mut chars = stripped.chars();
            match chars.next() {
                Some(c) => c.to_ascii_uppercase().to_string() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect();
    ItemClassId::from(normalized.as_str())
}

/// Resolve a mod line by case-insensitive substring match against
/// every mod's `text_template` (when present) or `name` field.
fn resolve_mod_line(
    line: &ModLine,
    registry: &ModRegistry,
    class: &ItemClassId,
    expected_kind: ModKind,
) -> Option<ModRoll> {
    let needle = line.text.to_lowercase();
    let best: Option<&ModDefinition> = registry
        .iter()
        .filter(|m| m.allowed_item_classes.iter().any(|c| c == class))
        .filter(|m| m.kind == expected_kind || expected_kind == ModKind::Explicit)
        .find(|m| matches_template(m, &needle));
    let def = best?;
    Some(ModRoll {
        mod_id: ModId::from(def.id.as_str()),
        affix_type: def.affix_type,
        kind: def.kind,
        values: SmallVec::new(),
        is_fractured: false,
    })
}

fn matches_template(def: &ModDefinition, needle_lower: &str) -> bool {
    if let Some(template) = &def.text_template {
        if templates_match(template, needle_lower) {
            return true;
        }
    }
    if let Some(name) = &def.name {
        if needle_lower.contains(&name.to_lowercase()) {
            return true;
        }
    }
    false
}

/// Match a template against a lower-cased mod line.
///
/// Splits the template at `{N}` placeholders and verifies each literal
/// segment appears in the line in order. Each segment is matched with
/// non-empty character runs only — leading/trailing whitespace is
/// trimmed since placeholders typically substitute numeric tokens that
/// don't carry whitespace of their own.
fn templates_match(template: &str, line_lower: &str) -> bool {
    let template_lower = template.to_lowercase();
    let segments = split_template_by_placeholders(&template_lower);
    let mut cursor = 0_usize;
    for seg in &segments {
        let seg_trimmed = seg.trim();
        if seg_trimmed.is_empty() {
            continue;
        }
        match line_lower[cursor..].find(seg_trimmed) {
            Some(found) => cursor += found + seg_trimmed.len(),
            None => return false,
        }
    }
    true
}

/// Split `+{0} to Maximum Energy Shield` into
/// `["+", " to Maximum Energy Shield"]`. Placeholder spans like `{0}`
/// `{1}` `{2:+d}` etc. are removed.
fn split_template_by_placeholders(template: &str) -> Vec<String> {
    let mut segments: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut chars = template.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '{' {
            // Consume until matching `}`. Anything inside is a placeholder.
            for inner in chars.by_ref() {
                if inner == '}' {
                    break;
                }
            }
            // Push the segment up to here, then start a new one.
            segments.push(std::mem::take(&mut current));
        } else {
            current.push(c);
        }
    }
    segments.push(current);
    segments
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::parse_clipboard_text;
    use poc2_engine::ids::{ConceptId, ModGroupId, StatId, TagId};
    use poc2_engine::mods::{ModDomain, ModFlags, ModGroup, ModStat, SpawnWeight};
    use poc2_engine::patch::PatchRange;
    use smallvec::smallvec;

    fn mk_mod(
        id: &str,
        text_template: &str,
        kind: ModKind,
        affix: AffixType,
        class: &str,
    ) -> ModDefinition {
        ModDefinition {
            id: ModId::from(id),
            name: None,
            mod_group: ModGroup(ModGroupId::from(format!("g-{id}"))),
            affix_type: affix,
            kind,
            domain: ModDomain::Item,
            tags: smallvec![],
            concept_set: smallvec![ConceptId::from("EnergyShield")],
            spawn_weights: smallvec![SpawnWeight {
                tag: TagId::from(class),
                weight: 1
            }],
            stats: smallvec![ModStat {
                stat_id: StatId::from("local_energy_shield"),
                min: 0.0,
                max: 100.0
            }],
            required_level: 1,
            allowed_item_classes: smallvec![ItemClassId::from(class)],
            patch_range: PatchRange::ALL,
            flags: ModFlags::empty(),
            text_template: Some(text_template.to_string()),
        }
    }

    #[test]
    fn item_class_normalization_is_naive_strip_s() {
        assert_eq!(
            item_class_id_from_text("Body Armours").as_str(),
            "BodyArmour"
        );
        assert_eq!(item_class_id_from_text("Helmets").as_str(), "Helmet");
        assert_eq!(
            item_class_id_from_text("One Hand Maces").as_str(),
            "OneHandMace"
        );
    }

    #[test]
    fn lower_resolves_mod_text_via_template_match() {
        let registry = ModRegistry::from_mods(vec![
            mk_mod(
                "EsPrefix1",
                "+{0} to Maximum Energy Shield",
                ModKind::Explicit,
                AffixType::Prefix,
                "BodyArmour",
            ),
            mk_mod(
                "ColdResSuffix1",
                "+{0}% to Cold Resistance",
                ModKind::Explicit,
                AffixType::Suffix,
                "BodyArmour",
            ),
        ]);

        let parsed = parse_clipboard_text(
            "Item Class: Body Armours\n\
             Rarity: Rare\n\
             Doom Greaves\n\
             Wyrmscale Coat\n\
             --------\n\
             Item Level: 82\n\
             --------\n\
             +25 to Maximum Energy Shield\n\
             +18% to Cold Resistance\n",
        )
        .unwrap();
        let (item, unresolved) = lower_to_item(&parsed, &registry).unwrap();
        assert_eq!(item.prefixes.len(), 1);
        assert_eq!(item.suffixes.len(), 1);
        assert!(unresolved.is_empty());
    }

    #[test]
    fn unresolved_lines_surface_in_output() {
        let registry = ModRegistry::from_mods(vec![]);
        let parsed = parse_clipboard_text(
            "Item Class: Body Armours\n\
             Rarity: Rare\n\
             Doom Greaves\n\
             Wyrmscale Coat\n\
             --------\n\
             Item Level: 82\n\
             --------\n\
             +25 to Maximum Energy Shield\n",
        )
        .unwrap();
        let (item, unresolved) = lower_to_item(&parsed, &registry).unwrap();
        assert!(item.prefixes.is_empty());
        assert_eq!(unresolved.len(), 1);
    }

    #[test]
    fn fractured_flag_propagates_to_mod_roll() {
        let registry = ModRegistry::from_mods(vec![mk_mod(
            "EsPrefix1",
            "+{0} to Maximum Energy Shield",
            ModKind::Explicit,
            AffixType::Prefix,
            "BodyArmour",
        )]);
        let parsed = parse_clipboard_text(
            "Item Class: Body Armours\n\
             Rarity: Rare\n\
             Doom Greaves\n\
             Wyrmscale Coat\n\
             --------\n\
             Item Level: 82\n\
             --------\n\
             +25 to Maximum Energy Shield (fractured)\n",
        )
        .unwrap();
        let (item, _) = lower_to_item(&parsed, &registry).unwrap();
        assert_eq!(item.prefixes.len(), 1);
        assert!(item.prefixes[0].is_fractured);
    }
}
