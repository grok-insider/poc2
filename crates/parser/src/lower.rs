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

use crate::text::{AnnotationAffix, ModLine, ParsedItem};

#[derive(Debug, Error)]
pub enum LowerError {
    #[error("parsed item has no item class")]
    NoItemClass,
}

/// Lower a [`ParsedItem`] into an engine [`Item`], using a caller-resolved
/// item `class` and (when known) the real bundle `resolved_base`.
///
/// Setting `resolved_base` to the bundle `BaseTypeId` lets the engine resolve
/// the base's attribute-variant **tags** (str/dex/int), so the eligible-mod
/// pool is correct. When `None`, the item's `base` falls back to the class id
/// (legacy behaviour: class-only pool, no attribute-variant gating).
///
/// Returns the [`Item`] plus a list of mod text strings that did NOT resolve.
#[allow(clippy::unnecessary_wraps)]
pub fn lower_to_item(
    parsed: &ParsedItem,
    registry: &ModRegistry,
    class: &ItemClassId,
    resolved_base: Option<BaseTypeId>,
) -> Result<(Item, Vec<String>), LowerError> {
    let mut prefixes: SmallVec<[ModRoll; 3]> = SmallVec::new();
    let mut suffixes: SmallVec<[ModRoll; 3]> = SmallVec::new();
    let mut implicits: SmallVec<[ModRoll; 2]> = SmallVec::new();
    let mut unresolved: Vec<String> = Vec::new();

    for line in &parsed.implicits {
        if let Some(roll) = resolve_line(line, registry, class, ModKind::Implicit) {
            implicits.push(roll);
        } else {
            unresolved.push(line.text.clone());
        }
    }

    for line in &parsed.explicits {
        let kind = if line.implicit_tag {
            ModKind::Implicit
        } else {
            ModKind::Explicit
        };
        let Some(mut roll) = resolve_line(line, registry, class, kind) else {
            unresolved.push(line.text.clone());
            continue;
        };
        if line.fractured {
            roll.is_fractured = true;
        }
        if line.crafted {
            // `(crafted)`-tagged lines are crafted modifiers (Alloy /
            // Emotion / Genesis outputs) — 0.5 limits items to one.
            roll.kind = ModKind::Crafted;
        }
        match roll.affix_type {
            AffixType::Prefix => prefixes.push(roll),
            AffixType::Suffix => suffixes.push(roll),
            AffixType::Implicit => implicits.push(roll),
            AffixType::Enchantment => {}
        }
    }

    let base = resolved_base.unwrap_or_else(|| BaseTypeId::from(class.as_str()));
    let item = Item {
        base,
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

/// Offline convenience: resolve the class from the parsed text (naive
/// normalization) with no base resolution. Used by callers without a bundle.
#[allow(clippy::unnecessary_wraps)]
pub fn lower_to_item_offline(
    parsed: &ParsedItem,
    registry: &ModRegistry,
) -> Result<(Item, Vec<String>), LowerError> {
    let class = item_class_id_from_text(&parsed.item_class);
    lower_to_item(parsed, registry, &class, None)
}

/// Resolve one mod line: prefer the Advanced annotation (name+tier+affix);
/// fall back to text-template matching. Populates `values` from parsed rolls.
fn resolve_line(
    line: &ModLine,
    registry: &ModRegistry,
    class: &ItemClassId,
    expected_kind: ModKind,
) -> Option<ModRoll> {
    if line.annotation.is_some() {
        if let Some(roll) = resolve_mod_line_advanced(line, registry, class) {
            return Some(roll);
        }
    }
    resolve_mod_line(line, registry, class, expected_kind)
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
///
/// All tiers of a mod group share a template, so a template match alone
/// is ambiguous. With parsed rolls the tier whose stat ranges contain
/// every rolled value wins; without rolls — or when no tier's ranges
/// contain them — the first template match is kept.
fn resolve_mod_line(
    line: &ModLine,
    registry: &ModRegistry,
    class: &ItemClassId,
    expected_kind: ModKind,
) -> Option<ModRoll> {
    let needle = line.text.to_lowercase();
    let mut matches = registry
        .iter()
        .filter(|m| m.allowed_item_classes.iter().any(|c| c == class))
        .filter(|m| m.kind == expected_kind || expected_kind == ModKind::Explicit)
        .filter(|m| matches_template(m, &needle));
    let first = matches.next()?;
    let best = if line.rolls.is_empty() {
        first
    } else {
        std::iter::once(first)
            .chain(matches)
            .find(|m| rolls_within_stat_ranges(line, m))
            .unwrap_or(first)
    };
    Some(mod_roll_from(best, line))
}

/// True when the line's rolls map one-to-one onto the definition's stats
/// and every rolled value lies inside its positional `[min, max]` range.
fn rolls_within_stat_ranges(line: &ModLine, def: &ModDefinition) -> bool {
    line.rolls.len() == def.stats.len()
        && line
            .rolls
            .iter()
            .zip(def.stats.iter())
            .all(|(r, s)| s.min <= r.value && r.value <= s.max)
}

/// Resolve a mod from its Advanced annotation: match `ModDefinition.name`
/// (case-insensitive) within the class+affix candidate set, disambiguated by
/// tier. Far more precise than fuzzy text matching.
fn resolve_mod_line_advanced(
    line: &ModLine,
    registry: &ModRegistry,
    class: &ItemClassId,
) -> Option<ModRoll> {
    let ann = line.annotation.as_ref()?;
    let affix = match ann.affix {
        AnnotationAffix::Prefix => AffixType::Prefix,
        AnnotationAffix::Suffix => AffixType::Suffix,
        // Implicits aren't indexed by `for_class_affix`; let the template
        // fallback handle them.
        AnnotationAffix::Implicit => return None,
    };
    if ann.name.is_empty() {
        return None;
    }
    let name_lower = ann.name.to_lowercase();
    let mut fallback: Option<&ModDefinition> = None;
    for &idx in registry.for_class_affix(class, affix) {
        let Some(m) = registry.at(idx) else { continue };
        let Some(mname) = &m.name else { continue };
        if mname.to_lowercase() != name_lower {
            continue;
        }
        // Exact tier match wins; otherwise remember the first name match.
        if ann.tier.is_some() && m.tier == ann.tier {
            return Some(mod_roll_from(m, line));
        }
        if fallback.is_none() {
            fallback = Some(m);
        }
    }
    fallback.map(|def| mod_roll_from(def, line))
}

/// Build a [`ModRoll`] from a resolved definition, populating `values` from
/// the line's parsed rolls (positional to `def.stats`; basic lines have none).
fn mod_roll_from(def: &ModDefinition, line: &ModLine) -> ModRoll {
    ModRoll {
        mod_id: ModId::from(def.id.as_str()),
        affix_type: def.affix_type,
        kind: def.kind,
        values: line.rolls.iter().map(|r| r.value).collect(),
        is_fractured: false,
    }
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
/// Bundle templates come in two dialects, both handled here:
/// - `{N}` placeholders (synthetic/legacy fixtures);
/// - the RePoE game-text form: numeric ranges like `(10-19)` / `(2—3)`
///   stand where the rolled value prints, and bracket markup
///   `[EnergyShield|Energy Shield]` / `[Fire]` wraps keywords (the game
///   prints the display form — the part after `|`, or the sole token).
///
/// The template is normalized (brackets resolved), split at placeholder
/// spans, and each literal segment must appear in the line in order.
fn templates_match(template: &str, line_lower: &str) -> bool {
    let template_lower = resolve_bracket_markup(&template.to_lowercase());
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

/// Resolve `[Reference|Display]` → `Display` and `[Keyword]` → `Keyword`.
/// The game prints the display form, so matching happens against it.
fn resolve_bracket_markup(template: &str) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(open) = rest.find('[') {
        let Some(close_rel) = rest[open..].find(']') else {
            break;
        };
        let close = open + close_rel;
        out.push_str(&rest[..open]);
        let inner = &rest[open + 1..close];
        out.push_str(inner.rsplit('|').next().unwrap_or(inner));
        rest = &rest[close + 1..];
    }
    out.push_str(rest);
    out
}

/// Split `+{0} to Maximum Energy Shield` into
/// `["+", " to Maximum Energy Shield"]`. Placeholder spans are removed:
/// `{0}` `{2:+d}` style braces, and parenthesized numeric ranges in the
/// game-text dialect — `(10-19)`, `(26—30)`, `(7.5-9)`.
fn split_template_by_placeholders(template: &str) -> Vec<String> {
    let mut segments: Vec<String> = Vec::new();
    let mut current = String::new();
    let bytes: Vec<char> = template.chars().collect();
    let mut i = 0_usize;
    while i < bytes.len() {
        let c = bytes[i];
        if c == '{' {
            while i < bytes.len() && bytes[i] != '}' {
                i += 1;
            }
            i += 1; // consume '}'
            segments.push(std::mem::take(&mut current));
            continue;
        }
        if c == '(' {
            if let Some(end) = numeric_range_end(&bytes, i) {
                i = end + 1; // consume ')'
                segments.push(std::mem::take(&mut current));
                continue;
            }
        }
        current.push(c);
        i += 1;
    }
    segments.push(current);
    segments
}

/// When `bytes[open]` starts a parenthesized numeric range like `(10-19)`
/// or `(2—3)`, return the index of the closing `)`; otherwise `None`.
fn numeric_range_end(bytes: &[char], open: usize) -> Option<usize> {
    let close = bytes[open..].iter().position(|&c| c == ')')? + open;
    let inner: String = bytes[open + 1..close].iter().collect();
    let mut saw_digit = false;
    let mut saw_sep = false;
    for c in inner.chars() {
        match c {
            '0'..='9' | '.' => saw_digit = true,
            '-' | '—' | '–' => saw_sep = true,
            _ => return None,
        }
    }
    (saw_digit && saw_sep).then_some(close)
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
            tier: None,
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
        let registry = ModRegistry::from_mods(
            vec![
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
            ],
            vec![],
        );

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
        let (item, unresolved) = lower_to_item_offline(&parsed, &registry).unwrap();
        assert_eq!(item.prefixes.len(), 1);
        assert_eq!(item.suffixes.len(), 1);
        assert!(unresolved.is_empty());
    }

    fn mk_life_tier(id: &str, tier: u16, min: f64, max: f64) -> ModDefinition {
        let mut def = mk_mod(
            id,
            "+{0} to maximum Life",
            ModKind::Explicit,
            AffixType::Prefix,
            "BodyArmour",
        );
        def.tier = Some(tier);
        def.stats = smallvec![ModStat {
            stat_id: StatId::from("base_maximum_life"),
            min,
            max
        }];
        def
    }

    fn life_tier_registry() -> ModRegistry {
        ModRegistry::from_mods(
            vec![
                mk_life_tier("LifeT1", 1, 100.0, 119.0),
                mk_life_tier("LifeT2", 2, 80.0, 99.0),
                mk_life_tier("LifeT3", 3, 60.0, 79.0),
            ],
            vec![],
        )
    }

    fn rare_with_life_line(line: &str) -> ParsedItem {
        parse_clipboard_text(&format!(
            "Item Class: Body Armours\n\
             Rarity: Rare\n\
             Doom Greaves\n\
             Wyrmscale Coat\n\
             --------\n\
             Item Level: 82\n\
             --------\n\
             {line}\n",
        ))
        .unwrap()
    }

    #[test]
    fn templates_match_repoe_range_dialect() {
        // Real bundle templates use parenthesized ranges, not {N}.
        assert!(templates_match(
            "+(10-19) to maximum Life",
            "+85 to maximum life"
        ));
        assert!(templates_match(
            "(15-25)% increased maximum Energy Shield",
            "23% increased maximum energy shield"
        ));
        // Em-dash separators appear in some exports.
        assert!(templates_match(
            "(2—3)% increased maximum Energy Shield",
            "2% increased maximum energy shield"
        ));
        // Fixed numeric values are NOT placeholders — different numbers
        // must not match.
        assert!(!templates_match(
            "Adds 60 to 100 Fire Damage if you've Blocked Recently",
            "adds 12 to 24 fire damage if you've blocked recently"
        ));
        // Non-numeric parens stay literal.
        assert!(!templates_match("(augmented) bonus", "plain bonus"));
    }

    #[test]
    fn templates_match_bracket_markup() {
        // Game-text markup: the game prints the display form.
        assert!(templates_match(
            "+(20-30) to maximum [EnergyShield|Energy Shield]",
            "+27 to maximum energy shield"
        ));
        assert!(templates_match(
            "+(30-45)% to [Resistances|Fire Resistance]",
            "+38% to fire resistance"
        ));
        assert!(templates_match(
            "(26-30)% of [Fire] Damage taken [Recoup|Recouped] as Life",
            "28% of fire damage taken recouped as life"
        ));
    }

    #[test]
    fn basic_rolls_pick_tier_by_value_range() {
        // Three tiers share a template; 85 falls only in T2's stat range.
        let registry = life_tier_registry();
        let parsed = rare_with_life_line("+85 to maximum Life");
        let (item, unresolved) = lower_to_item_offline(&parsed, &registry).unwrap();
        assert!(unresolved.is_empty());
        assert_eq!(item.prefixes.len(), 1);
        assert_eq!(item.prefixes[0].mod_id.as_str(), "LifeT2");
        assert_eq!(item.prefixes[0].values.as_slice(), &[85.0]);
    }

    #[test]
    fn basic_rolls_outside_all_ranges_keep_first_match() {
        // 999 is in no tier's range: keep the first template match but
        // still carry the parsed value.
        let registry = life_tier_registry();
        let parsed = rare_with_life_line("+999 to maximum Life");
        let (item, unresolved) = lower_to_item_offline(&parsed, &registry).unwrap();
        assert!(unresolved.is_empty());
        assert_eq!(item.prefixes.len(), 1);
        assert_eq!(item.prefixes[0].mod_id.as_str(), "LifeT1");
        assert_eq!(item.prefixes[0].values.as_slice(), &[999.0]);
    }

    #[test]
    fn unresolved_lines_surface_in_output() {
        let registry = ModRegistry::from_mods(vec![], vec![]);
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
        let (item, unresolved) = lower_to_item_offline(&parsed, &registry).unwrap();
        assert!(item.prefixes.is_empty());
        assert_eq!(unresolved.len(), 1);
    }

    #[test]
    fn fractured_flag_propagates_to_mod_roll() {
        let registry = ModRegistry::from_mods(
            vec![mk_mod(
                "EsPrefix1",
                "+{0} to Maximum Energy Shield",
                ModKind::Explicit,
                AffixType::Prefix,
                "BodyArmour",
            )],
            vec![],
        );
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
        let (item, _) = lower_to_item_offline(&parsed, &registry).unwrap();
        assert_eq!(item.prefixes.len(), 1);
        assert!(item.prefixes[0].is_fractured);
    }

    fn mk_named_mod(
        id: &str,
        name: &str,
        tier: u16,
        required_level: u32,
        affix: AffixType,
        class: &str,
    ) -> ModDefinition {
        ModDefinition {
            id: ModId::from(id),
            name: Some(name.to_string()),
            mod_group: ModGroup(ModGroupId::from("es-incr")),
            affix_type: affix,
            kind: ModKind::Explicit,
            domain: ModDomain::Item,
            tags: smallvec![],
            concept_set: smallvec![ConceptId::from("EnergyShield")],
            spawn_weights: smallvec![SpawnWeight {
                tag: TagId::from(class),
                weight: 1
            }],
            stats: smallvec![ModStat {
                stat_id: StatId::from("local_es_pct"),
                min: 0.0,
                max: 100.0
            }],
            required_level,
            tier: Some(tier),
            allowed_item_classes: smallvec![ItemClassId::from(class)],
            patch_range: PatchRange::ALL,
            flags: ModFlags::empty(),
            text_template: Some("{0}% increased Energy Shield".to_string()),
        }
    }

    #[test]
    fn lower_resolves_by_annotation_name_and_tier() {
        // Two tiers of the same named affix; the annotation tier picks T2.
        let registry = ModRegistry::from_mods(
            vec![
                mk_named_mod("EsT1", "Indomitable", 1, 84, AffixType::Prefix, "Focus"),
                mk_named_mod("EsT2", "Indomitable", 2, 73, AffixType::Prefix, "Focus"),
            ],
            vec![],
        );
        let parsed = parse_clipboard_text(
            "Item Class: Foci\n\
             Rarity: Magic\n\
             Indomitable Tasalian Focus\n\
             --------\n\
             Item Level: 80\n\
             --------\n\
             { Prefix Modifier \"Indomitable\" (Tier: 2) — Energy Shield }\n\
             90(80-91)% increased Energy Shield\n",
        )
        .unwrap();
        let (item, unresolved) =
            lower_to_item(&parsed, &registry, &ItemClassId::from("Focus"), None).unwrap();
        assert!(unresolved.is_empty());
        assert_eq!(item.prefixes.len(), 1);
        assert_eq!(item.prefixes[0].mod_id.as_str(), "EsT2");
        // Rolled value populated from `90(80-91)`.
        assert_eq!(item.prefixes[0].values.as_slice(), &[90.0]);
    }

    #[test]
    fn lower_sets_resolved_base_id() {
        let registry = ModRegistry::from_mods(vec![], vec![]);
        let parsed = parse_clipboard_text(
            "Item Class: Foci\n\
             Rarity: Normal\n\
             Tasalian Focus\n\
             --------\n\
             Item Level: 80\n",
        )
        .unwrap();
        let base = BaseTypeId::from("Metadata/Items/Armours/Focii/FourFocus10Endgame");
        let (item, _) = lower_to_item(
            &parsed,
            &registry,
            &ItemClassId::from("Focus"),
            Some(base.clone()),
        )
        .unwrap();
        assert_eq!(item.base, base);
    }
}
