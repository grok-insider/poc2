//! Phase-1 text parser.
//!
//! Produces a [`ParsedItem`] from the raw clipboard string. No mod-id
//! resolution happens here; the consumer can render the parsed item to
//! the user even when no data bundle is loaded.

use poc2_engine::item::Rarity;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// What the parser produced from the clipboard text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParsedItem {
    /// Raw item-class string (e.g., `"Body Armours"`, `"One Hand Maces"`).
    pub item_class: String,
    pub rarity: Rarity,
    /// Display name for Magic / Rare / Unique items. `None` for Normal.
    pub name: Option<String>,
    /// Base type string (e.g., `"Expert Wyrmscale Coat"`).
    pub base: String,
    /// Item Level. 0 if absent.
    pub ilvl: u32,
    /// Quality (0..=20 typical). 0 if absent.
    pub quality: u8,
    /// Requirements block.
    pub requirements: Requirements,
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

/// One mod-text line, with optional trailing-tag flags.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModLine {
    /// The literal mod text the game printed, with any trailing tags
    /// stripped. E.g., `"+25 to Maximum Energy Shield"`.
    pub text: String,
    /// True if the game tagged this mod with `(fractured)`.
    pub fractured: bool,
    /// True if the game tagged this mod with `(crafted)`.
    pub crafted: bool,
    /// True if the game tagged this mod with `(implicit)`. Implicit
    /// mods are normally found in the implicit section but PoE2's
    /// runic-implicit mods appear in the explicit section with this tag.
    pub implicit_tag: bool,
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
        if section.iter().any(|l| l.starts_with("Quality:")) {
            parse_quality_section(section, &mut out);
        } else if section.first().map(|s| s.trim()) == Some("Requirements:") {
            parse_requirements_section(section, &mut out);
        } else if section.iter().any(|l| l.starts_with("Item Level:")) {
            parse_ilvl_section(section, &mut out);
        } else {
            // Either an implicit, explicit, or trailer-flag section.
            // Heuristic: if any line is a single-word trailing flag, it's the trailer.
            // If lines look like mod text, append as explicits.
            // (A real impl would know section ordering exactly; this is
            // robust enough for PoE2's actual formats.)
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
        } else {
            other.push(line);
        }
    }
    out.item_class = item_class.ok_or(ParseError::NoItemClass)?;
    out.rarity = rarity.ok_or_else(|| ParseError::NoRarity(lines.join("\n")))?;

    // `other` is the name + base block. Conventions:
    // - Normal: 1 line = base
    // - Magic:  1 line = generated name + base merged ("Doom Vest of the Hawk")
    //   we store the whole line as `base` since separating prefixes/suffixes
    //   from the base needs the bundle's BaseTypeRegistry.
    // - Rare:   2 lines = name on first, base on second
    // - Unique: 2 lines = name on first, base on second
    match (out.rarity, other.len()) {
        (Rarity::Normal, 1) => out.base = other[0].to_string(),
        (Rarity::Magic, 1) => {
            // Magic items have a generated name like "Sturdy Vest of the Hawk".
            // Treat the whole line as the base+affixes string for v1.
            out.base = other[0].to_string();
        }
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
        // (Other lines like Armour:, Energy Shield:, Spirit: are
        // base stats — ignored at the parser level; the engine's
        // base registry computes them. We could capture them later.)
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

fn parse_ilvl_section(lines: &[&str], out: &mut ParsedItem) {
    for line in lines {
        if let Some(rest) = line.strip_prefix("Item Level:") {
            if let Some(n) = parse_first_u32(rest) {
                out.ilvl = n;
            }
        }
    }
}

fn classify_mod_or_trailer(lines: &[&str], out: &mut ParsedItem) {
    // Trailer flags: each line is exactly one of these tokens.
    let all_trailer_flags = lines.iter().all(|l| is_trailer_flag(l.trim()));
    if all_trailer_flags {
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
    // Otherwise it's a mod section. Whether implicit or explicit is
    // determined positionally by the format: implicits come BEFORE
    // explicits, and each is its own section. We conservatively
    // append everything to `explicits` and rely on the
    // `(implicit)` tag (PoE prints these on each implicit mod) to
    // route them. If the producer didn't tag, we fall back to: "the
    // first mod section after Item Level is the explicit/implicit
    // mix" and split heuristically based on tags.
    for line in lines {
        let mod_line = parse_mod_line(line);
        if mod_line.implicit_tag {
            out.implicits.push(mod_line);
        } else {
            out.explicits.push(mod_line);
        }
    }
}

fn is_trailer_flag(s: &str) -> bool {
    matches!(s, "Corrupted" | "Mirrored" | "Sanctified" | "Note: ~price")
}

/// Strip trailing tags like `(fractured)` `(crafted)` `(implicit)`
/// from a mod line and capture them as flags.
fn parse_mod_line(line: &str) -> ModLine {
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
            // Rune-source augment: drop the tag, keep the line as explicit.
            text = stripped.trim_end().to_string();
        } else if let Some(stripped) = lower.strip_suffix("(enchant)") {
            text = stripped.trim_end().to_string();
        } else {
            break;
        }
    }
    ModLine {
        text: text.trim().to_string(),
        fractured,
        crafted,
        implicit_tag,
    }
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
        assert_eq!(p.explicits.len(), 4);
        assert_eq!(p.explicits[0].text, "+25 to Maximum Energy Shield");
        assert!(p.explicits[3].fractured);
        assert!(!p.corrupted);
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
}
