//! Phase-1 text parser.
//!
//! Produces a [`ParsedItem`] from the raw clipboard string. No mod-id
//! resolution happens here; the consumer can render the parsed item to
//! the user even when no data bundle is loaded.
//!
//! Two clipboard variants are supported and auto-detected:
//! - **Basic** — each explicit mod is a bare stat line.
//! - **Advanced Mod Descriptions** (PoE2 option) — each mod is a
//!   `{ <Affix> Modifier "<name>" (Tier: N) — tags }` header followed by
//!   one or more stat lines whose numbers carry `value(min-max)` ranges.

use poc2_engine::item::Rarity;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// What the parser produced from the clipboard text.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParsedItem {
    /// Raw item-class string (e.g., `"Body Armours"`, `"Foci"`).
    pub item_class: String,
    pub rarity: Rarity,
    /// Display name for Magic / Rare / Unique items. `None` for Normal.
    pub name: Option<String>,
    /// Base type string. For Magic items this is the merged name+affix line
    /// (e.g. `"Indomitable Tasalian Focus of the Polar Bear"`); the bundle-aware
    /// caller strips the affix words to recover the true base.
    pub base: String,
    /// Item Level. 0 if absent.
    pub ilvl: u32,
    /// Quality (0..=20 typical). 0 if absent.
    pub quality: u8,
    /// Requirements block.
    pub requirements: Requirements,
    /// Raw `Sockets:` line value (e.g. `"S"`, `"S S"`). `None` if absent.
    pub sockets: Option<String>,
    /// True when the input carried `{ ... Modifier ... }` annotations
    /// (the Advanced Mod Descriptions clipboard format).
    pub advanced: bool,
    /// Implicit mod text lines (raw).
    pub implicits: Vec<ModLine>,
    /// Explicit mod text lines (raw). Both prefixes and suffixes are
    /// here together — the engine's affix split needs the bundle for
    /// per-mod metadata.
    pub explicits: Vec<ModLine>,
    /// Trailing flags.
    pub corrupted: bool,
    pub mirrored: bool,
    pub sanctified: bool,
}

/// Mod requirements block.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Requirements {
    /// Level requirement, if present.
    pub level: Option<u32>,
    /// Strength requirement, if present.
    pub str_req: Option<u32>,
    /// Dexterity requirement, if present.
    pub dex_req: Option<u32>,
    /// Intelligence requirement, if present.
    pub int_req: Option<u32>,
}

/// One mod-text line, with optional trailing-tag flags and (for the
/// Advanced format) the parsed `{ ... Modifier ... }` annotation + rolls.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModLine {
    /// The literal mod text the game printed, with trailing tags stripped
    /// and `value(min-max)` ranges normalised to just the value
    /// (e.g. `"90% increased Energy Shield"`). Hybrid mods join their stat
    /// lines with `\n`.
    pub text: String,
    /// True if the game tagged this mod with `(fractured)` (or a
    /// `Fractured` header qualifier).
    pub fractured: bool,
    /// True if the game tagged this mod with `(crafted)`.
    pub crafted: bool,
    /// True if the game tagged this mod with `(implicit)` or the advanced
    /// header is an `Implicit Modifier`.
    pub implicit_tag: bool,
    /// True for a `Desecrated` advanced header qualifier.
    #[serde(default)]
    pub desecrated: bool,
    /// Advanced-format annotation (`{ ... Modifier ... }` header), if present.
    #[serde(default)]
    pub annotation: Option<ModAnnotation>,
    /// Per-stat rolled value + range parsed from `value(min-max)` tokens,
    /// left-to-right. Basic-format lines carry no range, so each roll's
    /// `min`/`max` collapse to the value itself.
    #[serde(default)]
    pub rolls: Vec<StatRoll>,
}

impl ModLine {
    fn basic(text: String, fractured: bool, crafted: bool, implicit_tag: bool) -> Self {
        let rolls = extract_basic_rolls(&text);
        ModLine {
            text,
            fractured,
            crafted,
            implicit_tag,
            desecrated: false,
            annotation: None,
            rolls,
        }
    }
}

/// Affix kind named by an advanced `{ ... Modifier ... }` header.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnnotationAffix {
    Prefix,
    Suffix,
    Implicit,
}

/// Parsed `{ <Affix> Modifier "<name>" (Tier: N) — tags }` header.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModAnnotation {
    pub affix: AnnotationAffix,
    /// Affix name, e.g. `"Indomitable"`, `"of the Polar Bear"`. Empty if the
    /// header carried no quoted name.
    pub name: String,
    /// Tier ordinal from `(Tier: N)`. `None` if absent.
    pub tier: Option<u16>,
    /// Tags after the em-dash, e.g. `["Elemental","Cold","Resistance"]`.
    pub tags: Vec<String>,
}

/// A single rolled stat value with the tier's `[min, max]` range, parsed
/// from a `value(min-max)` token like `90(80-91)`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct StatRoll {
    pub value: f64,
    pub min: f64,
    pub max: f64,
}

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("input is empty")]
    Empty,
    #[error("could not detect rarity line; got: {0}")]
    NoRarity(String),
    #[error("could not detect item class line")]
    NoItemClass,
    #[error("invalid rarity: {0}")]
    BadRarity(String),
}

const SEPARATOR: &str = "--------";

/// Parse PoE2 clipboard text into a [`ParsedItem`].
pub fn parse_clipboard_text(input: &str) -> Result<ParsedItem, ParseError> {
    if input.trim().is_empty() {
        return Err(ParseError::Empty);
    }

    // Strip BOM and trailing whitespace; normalize CRLF.
    let cleaned = input.trim_start_matches('\u{feff}').replace("\r\n", "\n");

    // Advanced format carries `{ ... Modifier ... }` header lines.
    let advanced = cleaned.lines().any(|l| is_annotation_header(l.trim()));

    // Split into sections by `--------` lines.
    let sections: Vec<Vec<&str>> = split_sections(&cleaned);
    if sections.is_empty() {
        return Err(ParseError::Empty);
    }

    let mut out = ParsedItem {
        item_class: String::new(),
        rarity: Rarity::Normal,
        name: None,
        base: String::new(),
        ilvl: 0,
        quality: 0,
        requirements: Requirements::default(),
        sockets: None,
        advanced,
        implicits: Vec::new(),
        explicits: Vec::new(),
        corrupted: false,
        mirrored: false,
        sanctified: false,
    };

    // Header section (first one). Must contain Item Class + Rarity + Name/Base.
    parse_header_section(&sections[0], &mut out)?;

    // Body sections.
    for section in sections.iter().skip(1) {
        if section.iter().any(|l| is_annotation_header(l.trim())) {
            // Advanced mod block: group `{header}` + following stat lines.
            classify_mod_or_trailer(section, &mut out);
        } else if section.iter().any(|l| l.starts_with("Quality:")) {
            parse_quality_section(section, &mut out);
        } else if section.first().map(|s| s.trim()) == Some("Requirements:") {
            parse_requirements_section(section, &mut out);
        } else if section.iter().any(|l| l.starts_with("Requires:")) {
            parse_requires_inline(section, &mut out);
        } else if section.iter().any(|l| l.starts_with("Item Level:")) {
            parse_ilvl_section(section, &mut out);
        } else if section.iter().any(|l| l.starts_with("Sockets:")) {
            parse_sockets_section(section, &mut out);
        } else if section.iter().all(|l| is_item_property_line(l)) {
            // A base-stat block (Energy Shield:, Armour:, etc.). Skip — the
            // engine recomputes base stats; these are not modifiers.
        } else {
            classify_mod_or_trailer(section, &mut out);
        }
    }

    Ok(out)
}

/// Split into sections separated by `--------` lines. Empty lines and
/// the separator lines themselves are stripped from the section bodies.
fn split_sections(input: &str) -> Vec<Vec<&str>> {
    let mut out: Vec<Vec<&str>> = Vec::new();
    let mut current: Vec<&str> = Vec::new();
    for line in input.lines() {
        let trimmed = line.trim_end();
        if trimmed == SEPARATOR {
            if !current.is_empty() {
                out.push(std::mem::take(&mut current));
            }
        } else if !trimmed.is_empty() {
            current.push(trimmed);
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

fn parse_header_section(lines: &[&str], out: &mut ParsedItem) -> Result<(), ParseError> {
    let mut item_class: Option<String> = None;
    let mut rarity: Option<Rarity> = None;
    let mut other: Vec<&str> = Vec::new();

    for line in lines {
        if let Some(rest) = line.strip_prefix("Item Class:") {
            item_class = Some(rest.trim().to_string());
        } else if let Some(rest) = line.strip_prefix("Rarity:") {
            rarity = Some(parse_rarity(rest.trim())?);
        } else if line.starts_with("Requires:") {
            // Some clients fold the one-line Requires into the header.
            parse_requires_line(line, out);
        } else {
            other.push(line);
        }
    }
    out.item_class = item_class.ok_or(ParseError::NoItemClass)?;
    out.rarity = rarity.ok_or_else(|| ParseError::NoRarity(lines.join("\n")))?;

    // `other` is the name + base block. Conventions:
    // - Normal: 1 line = base
    // - Magic:  1 line = generated name + base merged ("Doom Vest of the Hawk")
    //   we store the whole line as `base`; the bundle-aware caller strips affixes.
    // - Rare:   2 lines = name on first, base on second
    // - Unique: 2 lines = name on first, base on second
    match (out.rarity, other.len()) {
        // Normal: bare base. Magic: merged name+affix line (caller strips affixes).
        (Rarity::Normal | Rarity::Magic, 1) => out.base = other[0].to_string(),
        (Rarity::Rare | Rarity::Unique, 2) => {
            out.name = Some(other[0].to_string());
            out.base = other[1].to_string();
        }
        // Fallback: take the last non-empty line as base.
        _ => {
            if let Some(last) = other.last() {
                out.base = (*last).to_string();
            }
            if other.len() >= 2 {
                out.name = Some(other[0].to_string());
            }
        }
    }
    Ok(())
}

fn parse_rarity(s: &str) -> Result<Rarity, ParseError> {
    match s {
        "Normal" => Ok(Rarity::Normal),
        "Magic" => Ok(Rarity::Magic),
        "Rare" => Ok(Rarity::Rare),
        "Unique" => Ok(Rarity::Unique),
        other => Err(ParseError::BadRarity(other.to_string())),
    }
}

fn parse_quality_section(lines: &[&str], out: &mut ParsedItem) {
    for line in lines {
        if let Some(rest) = line.strip_prefix("Quality:") {
            // "+20% (augmented)" or "+0%"
            let digits: String = rest
                .chars()
                .skip_while(|c| !c.is_ascii_digit())
                .take_while(char::is_ascii_digit)
                .collect();
            if let Ok(q) = digits.parse::<u8>() {
                out.quality = q;
            }
        }
        // Other lines (Armour:, Energy Shield:, Spirit:) are base stats —
        // ignored at the parser level; the engine's base registry computes them.
    }
}

fn parse_requirements_section(lines: &[&str], out: &mut ParsedItem) {
    for line in lines {
        if let Some(rest) = line.strip_prefix("Level:") {
            out.requirements.level = parse_first_u32(rest);
        } else if let Some(rest) = line.strip_prefix("Str:") {
            out.requirements.str_req = parse_first_u32(rest);
        } else if let Some(rest) = line.strip_prefix("Dex:") {
            out.requirements.dex_req = parse_first_u32(rest);
        } else if let Some(rest) = line.strip_prefix("Int:") {
            out.requirements.int_req = parse_first_u32(rest);
        }
    }
}

fn parse_requires_inline(lines: &[&str], out: &mut ParsedItem) {
    for line in lines {
        if line.starts_with("Requires:") {
            parse_requires_line(line, out);
        }
    }
}

/// Parse a one-line `Requires: Level 80, 115 Int` (Advanced/short format).
/// Each comma token is classified by its trailing keyword.
fn parse_requires_line(line: &str, out: &mut ParsedItem) {
    let Some(rest) = line.strip_prefix("Requires:") else {
        return;
    };
    for token in rest.split(',') {
        let t = token.trim();
        let lower = t.to_ascii_lowercase();
        let n = parse_first_u32(t);
        if lower.starts_with("level") {
            out.requirements.level = n;
        } else if lower.ends_with("str") || lower.contains("strength") {
            out.requirements.str_req = n;
        } else if lower.ends_with("dex") || lower.contains("dexterity") {
            out.requirements.dex_req = n;
        } else if lower.ends_with("int") || lower.contains("intelligence") {
            out.requirements.int_req = n;
        }
    }
}

fn parse_ilvl_section(lines: &[&str], out: &mut ParsedItem) {
    for line in lines {
        if let Some(rest) = line.strip_prefix("Item Level:") {
            if let Some(n) = parse_first_u32(rest) {
                out.ilvl = n;
            }
        }
    }
}

fn parse_sockets_section(lines: &[&str], out: &mut ParsedItem) {
    for line in lines {
        if let Some(rest) = line.strip_prefix("Sockets:") {
            out.sockets = Some(rest.trim().to_string());
        }
    }
}

fn classify_mod_or_trailer(lines: &[&str], out: &mut ParsedItem) {
    // Trailer flags: each line is exactly one of these tokens.
    if lines.iter().all(|l| is_trailer_flag(l.trim())) {
        for line in lines {
            match line.trim() {
                "Corrupted" => out.corrupted = true,
                "Mirrored" => out.mirrored = true,
                "Sanctified" => out.sanctified = true,
                _ => {}
            }
        }
        return;
    }

    // Advanced block: `{ ... Modifier ... }` header(s) + following stat lines.
    if lines.iter().any(|l| is_annotation_header(l.trim())) {
        for ml in group_advanced_mod_lines(lines) {
            if ml.implicit_tag {
                out.implicits.push(ml);
            } else {
                out.explicits.push(ml);
            }
        }
        return;
    }

    // Basic format: one stat line per mod. Skip stray property lines.
    for line in lines {
        if is_item_property_line(line) {
            continue;
        }
        let mod_line = parse_mod_line(line);
        if mod_line.implicit_tag {
            out.implicits.push(mod_line);
        } else {
            out.explicits.push(mod_line);
        }
    }
}

/// Group an Advanced-format mod section: each `{header}` plus its following
/// stat line(s) becomes one [`ModLine`] (a hybrid mod = one header + N stat
/// lines → one `ModLine` with N rolls).
fn group_advanced_mod_lines(lines: &[&str]) -> Vec<ModLine> {
    let mut out: Vec<ModLine> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i].trim();
        if let Some(header) = parse_annotation_header(line) {
            let mut texts: Vec<String> = Vec::new();
            let mut rolls: Vec<StatRoll> = Vec::new();
            let mut fractured = header.fractured;
            let mut crafted = header.crafted;
            i += 1;
            while i < lines.len() && !is_annotation_header(lines[i].trim()) {
                let stat = lines[i].trim();
                if !stat.is_empty() {
                    let (text, f, c, _impl) = strip_trailing_tags(stat);
                    fractured |= f;
                    crafted |= c;
                    let (norm, line_rolls) = extract_stat_rolls(&text);
                    texts.push(norm);
                    rolls.extend(line_rolls);
                }
                i += 1;
            }
            let implicit_tag = header.annotation.affix == AnnotationAffix::Implicit;
            out.push(ModLine {
                text: texts.join("\n"),
                fractured,
                crafted,
                implicit_tag,
                desecrated: header.desecrated,
                annotation: Some(header.annotation),
                rolls,
            });
        } else {
            // Orphan stat line with no header (defensive). Treat as basic.
            if !line.is_empty() && !is_item_property_line(line) {
                out.push(parse_mod_line(line));
            }
            i += 1;
        }
    }
    out
}

struct ParsedHeader {
    annotation: ModAnnotation,
    fractured: bool,
    crafted: bool,
    desecrated: bool,
}

fn is_annotation_header(line: &str) -> bool {
    line.starts_with('{') && line.contains("Modifier")
}

/// Parse `{ <quals> <Affix> Modifier "<name>" (Tier: N) ... — tag, tag }`.
fn parse_annotation_header(line: &str) -> Option<ParsedHeader> {
    let line = line.trim();
    if !line.starts_with('{') {
        return None;
    }
    let inner = line.trim_start_matches('{').trim_end_matches('}').trim();

    // Split tags off the right of the em-dash (accept en-dash and " - ").
    let (left, tags) = split_header_tags(inner);

    // Locate the affix keyword and any leading qualifiers in `left`.
    let lower = left.to_ascii_lowercase();
    if !lower.contains("modifier") {
        return None;
    }
    let fractured = lower.contains("fractured");
    let crafted = lower.contains("crafted");
    let desecrated = lower.contains("desecrated");
    let affix = if lower.contains("prefix") {
        AnnotationAffix::Prefix
    } else if lower.contains("suffix") {
        AnnotationAffix::Suffix
    } else if lower.contains("implicit") {
        AnnotationAffix::Implicit
    } else {
        return None;
    };

    // Quoted name (optional).
    let name = left
        .split_once('"')
        .and_then(|(_, rest)| rest.split_once('"').map(|(n, _)| n.trim().to_string()))
        .unwrap_or_default();

    // (Tier: N)
    let tier = left
        .split_once("(Tier:")
        .and_then(|(_, rest)| parse_first_u32(rest))
        .and_then(|n| u16::try_from(n).ok());

    Some(ParsedHeader {
        annotation: ModAnnotation {
            affix,
            name,
            tier,
            tags,
        },
        fractured,
        crafted,
        desecrated,
    })
}

/// Split the header's left part from its tag list on the first em-/en-dash
/// (or ` - ` fallback). Returns `(left, tags)`.
fn split_header_tags(inner: &str) -> (String, Vec<String>) {
    let idx = inner
        .find('\u{2014}') // em dash —
        .or_else(|| inner.find('\u{2013}')) // en dash –
        .or_else(|| inner.find(" - "));
    if let Some(i) = idx {
        let left = inner[..i].trim().to_string();
        // Skip the dash char/sequence.
        let after = &inner[i..];
        let after = after
            .trim_start_matches('\u{2014}')
            .trim_start_matches('\u{2013}')
            .trim_start_matches(" - ")
            .trim_start_matches('-')
            .trim();
        let tags = after
            .split(',')
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .collect();
        (left, tags)
    } else {
        (inner.trim().to_string(), Vec::new())
    }
}

/// Replace each `value(min-max)` token with just `value`, returning the
/// normalised text and the parsed rolls in order.
fn extract_stat_rolls(line: &str) -> (String, Vec<StatRoll>) {
    let chars: Vec<char> = line.chars().collect();
    let mut out = String::with_capacity(line.len());
    let mut rolls: Vec<StatRoll> = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if let Some((roll, display, end)) = try_match_range(&chars, i) {
            out.push_str(&display);
            rolls.push(roll);
            i = end;
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    (out, rolls)
}

/// Extract the bare numeric tokens of a basic-format stat line, left-to-right.
/// The basic clipboard prints no `(min-max)` range, so each roll's `min`/`max`
/// collapse to the rolled value.
fn extract_basic_rolls(line: &str) -> Vec<StatRoll> {
    let chars: Vec<char> = line.chars().collect();
    let mut rolls: Vec<StatRoll> = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if let Some((value, len)) = parse_number(&chars, i) {
            rolls.push(StatRoll {
                value,
                min: value,
                max: value,
            });
            i += len;
        } else {
            i += 1;
        }
    }
    rolls
}

/// Try to match `NUMBER ( NUMBER - NUMBER )` starting at `chars[i]`.
/// Returns `(roll, value-display, end-index)` on success.
fn try_match_range(chars: &[char], i: usize) -> Option<(StatRoll, String, usize)> {
    let (value, vlen) = parse_number(chars, i)?;
    let mut j = i + vlen;
    if chars.get(j) != Some(&'(') {
        return None;
    }
    j += 1;
    let (min, mlen) = parse_number(chars, j)?;
    j += mlen;
    if chars.get(j) != Some(&'-') {
        return None;
    }
    j += 1;
    let (max, xlen) = parse_number(chars, j)?;
    j += xlen;
    if chars.get(j) != Some(&')') {
        return None;
    }
    j += 1;
    let display: String = chars[i..i + vlen].iter().collect();
    Some((StatRoll { value, min, max }, display, j))
}

/// Parse an optionally-signed decimal number starting at `chars[i]`.
/// Returns `(value, char-length-consumed)`.
fn parse_number(chars: &[char], i: usize) -> Option<(f64, usize)> {
    let mut j = i;
    if matches!(chars.get(j), Some('+' | '-')) {
        j += 1;
    }
    let start_digits = j;
    while matches!(chars.get(j), Some(c) if c.is_ascii_digit()) {
        j += 1;
    }
    if matches!(chars.get(j), Some('.')) {
        j += 1;
        while matches!(chars.get(j), Some(c) if c.is_ascii_digit()) {
            j += 1;
        }
    }
    if j == start_digits {
        return None; // no digits
    }
    let s: String = chars[i..j].iter().collect();
    s.parse::<f64>().ok().map(|v| (v, j - i))
}

fn is_trailer_flag(s: &str) -> bool {
    matches!(s, "Corrupted" | "Mirrored" | "Sanctified" | "Note: ~price")
}

/// Known item-property labels that prefix a `Label: value` base-stat line.
/// Used to skip base-stat blocks so they aren't misparsed as mods.
fn is_item_property_line(line: &str) -> bool {
    const LABELS: &[&str] = &[
        "Quality",
        "Armour",
        "Evasion",
        "Evasion Rating",
        "Energy Shield",
        "Ward",
        "Spirit",
        "Block",
        "Block chance",
        "Physical Damage",
        "Elemental Damage",
        "Chaos Damage",
        "Critical Hit Chance",
        "Critical Strike Chance",
        "Attacks per Second",
        "Weapon Range",
        "Item Level",
        "Requires",
        "Requirements",
        "Sockets",
        "Rune Sockets",
        "Stack Size",
        "Level",
        "Experience",
        "Limited to",
        "Radius",
        "Reload Time",
        // Granted-effect properties (shields' "Grants Skill: Raise Shield",
        // sceptres' "Grants Skill: Level 20 Malice", flask "Grants:" lines).
        // The colon check below keeps real mod lines ("Grants 30 Life per
        // Enemy Hit") out of this bucket.
        "Grants Skill",
        "Grants",
        "Charm Slots",
        "Duration",
        "Charges",
    ];
    LABELS
        .iter()
        .any(|lab| line.starts_with(lab) && line[lab.len()..].starts_with(':'))
}

/// Strip trailing tags like `(fractured)` `(crafted)` `(implicit)` from a
/// stat line and report which were present.
fn strip_trailing_tags(line: &str) -> (String, bool, bool, bool) {
    let mut text = line.to_string();
    let mut fractured = false;
    let mut crafted = false;
    let mut implicit_tag = false;
    loop {
        let lower = text.trim_end();
        if let Some(stripped) = lower.strip_suffix("(fractured)") {
            fractured = true;
            text = stripped.trim_end().to_string();
        } else if let Some(stripped) = lower.strip_suffix("(crafted)") {
            crafted = true;
            text = stripped.trim_end().to_string();
        } else if let Some(stripped) = lower.strip_suffix("(implicit)") {
            implicit_tag = true;
            text = stripped.trim_end().to_string();
        } else if let Some(stripped) = lower.strip_suffix("(rune)") {
            text = stripped.trim_end().to_string();
        } else if let Some(stripped) = lower.strip_suffix("(enchant)") {
            text = stripped.trim_end().to_string();
        } else {
            break;
        }
    }
    (text.trim().to_string(), fractured, crafted, implicit_tag)
}

/// Parse a basic-format mod line (no annotation, no ranges).
fn parse_mod_line(line: &str) -> ModLine {
    let (text, fractured, crafted, implicit_tag) = strip_trailing_tags(line);
    ModLine::basic(text, fractured, crafted, implicit_tag)
}

fn parse_first_u32(s: &str) -> Option<u32> {
    let digits: String = s
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse::<u32>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grants_skill_lines_are_properties_not_mods() {
        // Effigial Tower Shield regression: "Grants Skill: Raise Shield"
        // must never surface as an unresolved mod line.
        const SHIELD: &str = "\
Item Class: Shields
Rarity: Normal
Effigial Tower Shield
--------
Block chance: 26%
Armour: 76
--------
Requires: Level 21 (unmet), 32 Str
--------
Item Level: 21
--------
Grants Skill: Raise Shield
";
        let parsed = parse_clipboard_text(SHIELD).expect("parses");
        assert_eq!(parsed.ilvl, 21);
        assert!(
            parsed.explicits.is_empty(),
            "Grants Skill must not be classified as a mod; got {:?}",
            parsed.explicits
        );
        assert!(parsed.implicits.is_empty());
    }

    const RARE_BODY_ARMOUR: &str = "\
Item Class: Body Armours
Rarity: Rare
Doom Greaves
Expert Wyrmscale Coat
--------
Quality: +20% (augmented)
Armour: 521 (augmented)
Energy Shield: 102 (augmented)
--------
Requirements:
Level: 65
Str: 96
Int: 96
--------
Item Level: 82
--------
+25 to Maximum Energy Shield
+18% to Cold Resistance
+40 to Maximum Life
54% increased Energy Shield (fractured)
";

    const NORMAL_BASE: &str = "\
Item Class: Body Armours
Rarity: Normal
Wyrmscale Coat
--------
Item Level: 82
";

    const MAGIC_ITEM: &str = "\
Item Class: Body Armours
Rarity: Magic
Sturdy Wyrmscale Coat of the Hawk
--------
Quality: +0%
Armour: 521
--------
Requirements:
Level: 65
--------
Item Level: 82
--------
+15% increased Armour
+20 to Dexterity
";

    const CORRUPTED_RARE: &str = "\
Item Class: Body Armours
Rarity: Rare
Doom Greaves
Wyrmscale Coat
--------
Item Level: 82
--------
+50 to Maximum Life
--------
Corrupted
";

    /// The user's exact Advanced Mod Descriptions example.
    const ADVANCED_FOCUS: &str = "\
Item Class: Foci
Rarity: Magic
Indomitable Tasalian Focus of the Polar Bear
--------
Energy Shield: 173 (augmented)
--------
Requires: Level 80, 115 Int
--------
Sockets: S
--------
Item Level: 80
--------
{ Prefix Modifier \"Indomitable\" (Tier: 2) — Energy Shield }
90(80-91)% increased Energy Shield
{ Suffix Modifier \"of the Polar Bear\" (Tier: 3) — Elemental, Cold, Resistance }
+32(31-35)% to Cold Resistance
";

    #[test]
    fn parse_rare_full_format() {
        let p = parse_clipboard_text(RARE_BODY_ARMOUR).unwrap();
        assert_eq!(p.item_class, "Body Armours");
        assert_eq!(p.rarity, Rarity::Rare);
        assert_eq!(p.name.as_deref(), Some("Doom Greaves"));
        assert_eq!(p.base, "Expert Wyrmscale Coat");
        assert_eq!(p.ilvl, 82);
        assert_eq!(p.quality, 20);
        assert_eq!(p.requirements.level, Some(65));
        assert_eq!(p.requirements.str_req, Some(96));
        assert_eq!(p.requirements.int_req, Some(96));
        assert!(!p.advanced);
        assert_eq!(p.explicits.len(), 4);
        assert_eq!(p.explicits[0].text, "+25 to Maximum Energy Shield");
        assert!(p.explicits[3].fractured);
        assert!(!p.corrupted);
    }

    #[test]
    fn basic_format_lines_carry_value_rolls() {
        let p = parse_clipboard_text(RARE_BODY_ARMOUR).unwrap();
        let values: Vec<Vec<f64>> = p
            .explicits
            .iter()
            .map(|m| m.rolls.iter().map(|r| r.value).collect())
            .collect();
        assert_eq!(values, vec![vec![25.0], vec![18.0], vec![40.0], vec![54.0]]);
        // No range in the basic format: min/max collapse to the value.
        assert_eq!(
            p.explicits[0].rolls,
            vec![StatRoll {
                value: 25.0,
                min: 25.0,
                max: 25.0
            }]
        );
    }

    #[test]
    fn basic_format_multi_number_line_carries_all_rolls() {
        let rolls = extract_basic_rolls("Adds 5 to 12 Physical Damage");
        assert_eq!(
            rolls.iter().map(|r| r.value).collect::<Vec<_>>(),
            vec![5.0, 12.0]
        );
    }

    #[test]
    fn parse_normal_with_no_explicits() {
        let p = parse_clipboard_text(NORMAL_BASE).unwrap();
        assert_eq!(p.rarity, Rarity::Normal);
        assert_eq!(p.name, None);
        assert_eq!(p.base, "Wyrmscale Coat");
        assert_eq!(p.ilvl, 82);
        assert!(p.explicits.is_empty());
    }

    #[test]
    fn parse_magic_format() {
        let p = parse_clipboard_text(MAGIC_ITEM).unwrap();
        assert_eq!(p.rarity, Rarity::Magic);
        assert_eq!(p.name, None);
        assert_eq!(p.base, "Sturdy Wyrmscale Coat of the Hawk");
        assert_eq!(p.explicits.len(), 2);
    }

    #[test]
    fn parse_detects_corrupted_trailer() {
        let p = parse_clipboard_text(CORRUPTED_RARE).unwrap();
        assert!(p.corrupted);
        assert!(!p.mirrored);
        assert!(!p.sanctified);
    }

    #[test]
    fn parse_empty_input_errors() {
        assert!(matches!(parse_clipboard_text(""), Err(ParseError::Empty)));
        assert!(matches!(
            parse_clipboard_text("   \n  "),
            Err(ParseError::Empty)
        ));
    }

    #[test]
    fn parse_handles_crlf_line_endings() {
        let crlf = RARE_BODY_ARMOUR.replace('\n', "\r\n");
        let p = parse_clipboard_text(&crlf).unwrap();
        assert_eq!(p.ilvl, 82);
    }

    #[test]
    fn parse_strips_implicit_tag_routes_to_implicits() {
        let s = "\
Item Class: Body Armours
Rarity: Rare
Doom Greaves
Wyrmscale Coat
--------
Item Level: 82
--------
+15 to Maximum Life (implicit)
+25 to Maximum Energy Shield
";
        let p = parse_clipboard_text(s).unwrap();
        assert_eq!(p.implicits.len(), 1);
        assert_eq!(p.explicits.len(), 1);
        assert_eq!(p.implicits[0].text, "+15 to Maximum Life");
        assert!(p.implicits[0].implicit_tag);
    }

    #[test]
    fn parse_strips_crafted_tag() {
        let s = "\
Item Class: Body Armours
Rarity: Rare
Doom Greaves
Wyrmscale Coat
--------
Item Level: 82
--------
+30% to Cold Resistance (crafted)
";
        let p = parse_clipboard_text(s).unwrap();
        assert_eq!(p.explicits[0].text, "+30% to Cold Resistance");
        assert!(p.explicits[0].crafted);
    }

    #[test]
    fn parse_unknown_rarity_errors() {
        let s = "\
Item Class: Body Armours
Rarity: Mythical
Foo
";
        let r = parse_clipboard_text(s);
        assert!(matches!(r, Err(ParseError::BadRarity(_))));
    }

    // ---- Advanced Mod Descriptions format -----------------------------

    #[test]
    fn parse_advanced_focus_example() {
        let p = parse_clipboard_text(ADVANCED_FOCUS).unwrap();
        assert!(p.advanced);
        assert_eq!(p.item_class, "Foci");
        assert_eq!(p.rarity, Rarity::Magic);
        assert_eq!(p.base, "Indomitable Tasalian Focus of the Polar Bear");
        assert_eq!(p.ilvl, 80);
        assert_eq!(p.sockets.as_deref(), Some("S"));
        assert_eq!(p.requirements.level, Some(80));
        assert_eq!(p.requirements.int_req, Some(115));

        assert_eq!(p.explicits.len(), 2);

        let pre = &p.explicits[0];
        assert_eq!(pre.text, "90% increased Energy Shield");
        assert_eq!(
            pre.rolls,
            vec![StatRoll {
                value: 90.0,
                min: 80.0,
                max: 91.0
            }]
        );
        let a = pre.annotation.as_ref().unwrap();
        assert_eq!(a.affix, AnnotationAffix::Prefix);
        assert_eq!(a.name, "Indomitable");
        assert_eq!(a.tier, Some(2));
        assert_eq!(a.tags, vec!["Energy Shield"]);

        let suf = &p.explicits[1];
        assert_eq!(suf.text, "+32% to Cold Resistance");
        assert_eq!(
            suf.rolls,
            vec![StatRoll {
                value: 32.0,
                min: 31.0,
                max: 35.0
            }]
        );
        let b = suf.annotation.as_ref().unwrap();
        assert_eq!(b.affix, AnnotationAffix::Suffix);
        assert_eq!(b.name, "of the Polar Bear");
        assert_eq!(b.tier, Some(3));
        assert_eq!(b.tags, vec!["Elemental", "Cold", "Resistance"]);
    }

    #[test]
    fn parse_advanced_hybrid_one_header_two_stats() {
        let s = "\
Item Class: Foci
Rarity: Magic
Whorl Focus
--------
Item Level: 80
--------
{ Prefix Modifier \"Sapphire\" (Tier: 1) — Energy Shield, Mana }
12(10-13)% increased Energy Shield
+8(7-10) to maximum Mana
";
        let p = parse_clipboard_text(s).unwrap();
        assert_eq!(p.explicits.len(), 1);
        let m = &p.explicits[0];
        assert_eq!(m.text, "12% increased Energy Shield\n+8 to maximum Mana");
        assert_eq!(m.rolls.len(), 2);
        assert_eq!(
            m.rolls[0],
            StatRoll {
                value: 12.0,
                min: 10.0,
                max: 13.0
            }
        );
        assert_eq!(
            m.rolls[1],
            StatRoll {
                value: 8.0,
                min: 7.0,
                max: 10.0
            }
        );
    }

    #[test]
    fn parse_requires_inline_format() {
        let mut out = ParsedItem {
            item_class: String::new(),
            rarity: Rarity::Normal,
            name: None,
            base: String::new(),
            ilvl: 0,
            quality: 0,
            requirements: Requirements::default(),
            sockets: None,
            advanced: false,
            implicits: Vec::new(),
            explicits: Vec::new(),
            corrupted: false,
            mirrored: false,
            sanctified: false,
        };
        parse_requires_line("Requires: Level 80, 115 Int", &mut out);
        assert_eq!(out.requirements.level, Some(80));
        assert_eq!(out.requirements.int_req, Some(115));
        assert_eq!(out.requirements.str_req, None);
    }

    #[test]
    fn parse_sockets_line_not_a_mod() {
        let p = parse_clipboard_text(ADVANCED_FOCUS).unwrap();
        assert!(p.explicits.iter().all(|m| !m.text.contains("Sockets")));
        assert!(p
            .explicits
            .iter()
            .all(|m| !m.text.contains("Energy Shield: 173")));
    }

    #[test]
    fn extract_multiple_ranges_in_one_line() {
        let (text, rolls) = extract_stat_rolls("Adds 5(4-6) to 12(10-14) Physical Damage");
        assert_eq!(text, "Adds 5 to 12 Physical Damage");
        assert_eq!(rolls.len(), 2);
        assert_eq!(
            rolls[0],
            StatRoll {
                value: 5.0,
                min: 4.0,
                max: 6.0
            }
        );
        assert_eq!(
            rolls[1],
            StatRoll {
                value: 12.0,
                min: 10.0,
                max: 14.0
            }
        );
    }
}
