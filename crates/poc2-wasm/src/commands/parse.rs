//! `parse` command — turn raw PoE2 clipboard item text into an engine `Item`.
//!
//! Three phases:
//! 1. **text** — pure string parsing ([`parse_clipboard_text`]), handling both
//!    the basic and the "Advanced Mod Descriptions" clipboard formats.
//! 2. **resolve** — map the printed item-class (e.g. `"Foci"`) to the canonical
//!    [`ItemClassId`] and the (de-affixed) base name to the real bundle
//!    [`BaseTypeId`], so the engine applies the right attribute-variant
//!    (str/dex/int) modifier pool.
//! 3. **lower** — resolve each mod line to a registered mod id.
//!
//! The class/base lookup tables are built once from the bundle and cached on
//! the engine state (see [`build_class_index`] / [`build_base_index`]).

use std::collections::HashMap;

use poc2_engine::base::{BaseType, ReleaseState};
use poc2_engine::ids::{BaseTypeId, ItemClassId};
use poc2_engine::item::{Item, Rarity};
use poc2_engine::ModRegistry;
use poc2_parser::{
    item_class_id_from_text, lower_to_item, parse_clipboard_text, AnnotationAffix, ParsedItem,
};
use serde::Serialize;

/// Lookup tables + registry needed to parse a clipboard item against a bundle.
pub struct ParseContext<'a> {
    pub registry: &'a ModRegistry,
    /// Lowercased class-display string → canonical [`ItemClassId`].
    pub class_by_display: &'a HashMap<String, ItemClassId>,
    /// `(class, lowercased base name)` → real bundle [`BaseTypeId`].
    pub base_by_class_name: &'a HashMap<(ItemClassId, String), BaseTypeId>,
}

/// Response of the `parse` command. The first three fields preserve the
/// original contract; the rest are additive (base/class resolution + warnings).
#[derive(Debug, Serialize)]
pub struct ParseClipboardResponse {
    /// Phase-1 parse output (text fields, incl. advanced annotations/rolls).
    pub parsed: ParsedItem,
    /// Phase-3 lower output (engine `Item`, with the resolved base id).
    pub item: Item,
    /// Mod text lines that did not resolve to any registered mod.
    pub unresolved: Vec<String>,
    /// Resolved base display name (Magic items have affix words stripped).
    pub base_display_name: Option<String>,
    /// Canonical item class id (e.g. `"Focus"`).
    pub item_class_id: Option<String>,
    /// `true` when the base resolved to a real bundle base (correct mod pool);
    /// `false` means we fell back to the class id (approximate pool).
    pub base_resolved: bool,
    /// Non-fatal notices (unresolved base, ambiguity, …).
    pub warnings: Vec<String>,
}

/// Parse PoE2 clipboard item text into an engine [`Item`] with a resolved base.
pub fn parse_item(ctx: &ParseContext, text: &str) -> Result<ParseClipboardResponse, String> {
    let parsed = parse_clipboard_text(text).map_err(|e| e.to_string())?;

    let class = resolve_item_class(ctx, &parsed.item_class);

    // Recover the true base name. For Magic items the display line merges the
    // base with affix words; the advanced format names the affixes explicitly.
    let base_name = if parsed.rarity == Rarity::Magic && parsed.advanced {
        strip_affix_words(&parsed.base, &parsed)
    } else {
        parsed.base.clone()
    };

    let mut warnings: Vec<String> = Vec::new();
    let resolved = resolve_base(ctx, &class, &base_name).or_else(|| {
        // Basic-format Magic: no annotation names, fall back to the longest
        // base name contained in the merged display string.
        if parsed.rarity == Rarity::Magic && !parsed.advanced {
            resolve_base_contained(ctx, &class, &parsed.base)
        } else {
            None
        }
    });

    let base_resolved = resolved.is_some();
    if !base_resolved {
        warnings.push(format!(
            "base '{base_name}' not found for class '{}' — modifier pool is approximate",
            class.as_str()
        ));
    }

    let (item, unresolved) =
        lower_to_item(&parsed, ctx.registry, &class, resolved).map_err(|e| e.to_string())?;

    Ok(ParseClipboardResponse {
        base_display_name: (!base_name.is_empty()).then(|| base_name.clone()),
        item_class_id: Some(class.as_str().to_string()),
        base_resolved,
        warnings,
        parsed,
        item,
        unresolved,
    })
}

// ---- class resolution -------------------------------------------------------

/// Resolve the printed `Item Class:` string (display plural, e.g. `"Foci"`,
/// `"Body Armours"`) to the canonical [`ItemClassId`] (`"Focus"`,
/// `"BodyArmour"`). Falls back to naive normalization when unknown.
fn resolve_item_class(ctx: &ParseContext, display: &str) -> ItemClassId {
    let key = display.trim().to_lowercase();
    if let Some(canonical) = irregular_class(&key) {
        return ItemClassId::from(canonical);
    }
    if let Some(c) = ctx.class_by_display.get(&key) {
        return c.clone();
    }
    item_class_id_from_text(display)
}

/// Game plurals that simple de-pluralization can't recover.
fn irregular_class(key: &str) -> Option<&'static str> {
    Some(match key {
        "foci" => "Focus",
        "staves" => "Staff",
        "warstaves" => "Warstaff",
        _ => return None,
    })
}

/// Build the display-string → canonical class index from the bundle's bases.
#[must_use]
pub fn build_class_index(bundle: &poc2_data::Bundle) -> HashMap<String, ItemClassId> {
    let mut map: HashMap<String, ItemClassId> = HashMap::new();
    for b in &bundle.base_items {
        for key in class_display_keys(b.item_class.as_str()) {
            map.entry(key).or_insert_with(|| b.item_class.clone());
        }
    }
    map
}

/// Lowercased display variants the game might print for a canonical class id.
fn class_display_keys(canonical: &str) -> Vec<String> {
    let split = pascal_split(canonical).to_lowercase();
    let lower = canonical.to_lowercase();
    let mut keys = vec![
        lower.clone(),
        split.clone(),
        split.replace(' ', ""),
        format!("{split}s"),
        format!("{lower}s"),
    ];
    keys.sort();
    keys.dedup();
    keys
}

/// Insert a space before each interior uppercase letter (`BodyArmour` →
/// `Body Armour`).
fn pascal_split(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && c.is_ascii_uppercase() {
            out.push(' ');
        }
        out.push(c);
    }
    out
}

// ---- base resolution --------------------------------------------------------

/// Strip the Advanced-format affix words from a Magic item's merged display
/// line to recover the base name ("Indomitable Tasalian Focus of the Polar
/// Bear" → "Tasalian Focus").
fn strip_affix_words(display: &str, parsed: &ParsedItem) -> String {
    let mut s = display.trim().to_string();
    for line in &parsed.explicits {
        let Some(ann) = &line.annotation else {
            continue;
        };
        let name = ann.name.trim();
        if name.is_empty() {
            continue;
        }
        match ann.affix {
            AnnotationAffix::Prefix => s = strip_prefix_word(&s, name),
            AnnotationAffix::Suffix => s = strip_suffix_phrase(&s, name),
            AnnotationAffix::Implicit => {}
        }
    }
    s.trim().to_string()
}

fn strip_prefix_word(s: &str, name: &str) -> String {
    if let Some(rest) = s.get(name.len()..) {
        if s[..name.len()].eq_ignore_ascii_case(name) {
            return rest.trim_start().to_string();
        }
    }
    s.to_string()
}

fn strip_suffix_phrase(s: &str, name: &str) -> String {
    if name.len() > s.len() {
        return s.to_string();
    }
    let split = s.len() - name.len();
    if let Some(head) = s.get(..split) {
        if s[split..].eq_ignore_ascii_case(name) {
            return head.trim_end().to_string();
        }
    }
    s.to_string()
}

fn resolve_base(ctx: &ParseContext, class: &ItemClassId, name: &str) -> Option<BaseTypeId> {
    let key = (class.clone(), name.trim().to_lowercase());
    ctx.base_by_class_name.get(&key).cloned()
}

/// Fallback for basic-format Magic items: pick the longest class base name
/// that appears in the merged display string.
fn resolve_base_contained(
    ctx: &ParseContext,
    class: &ItemClassId,
    full: &str,
) -> Option<BaseTypeId> {
    let hay = full.to_lowercase();
    let mut best: Option<(usize, &BaseTypeId)> = None;
    for ((c, name), id) in ctx.base_by_class_name {
        if c != class || !hay.contains(name.as_str()) {
            continue;
        }
        if best.is_none_or(|(len, _)| name.len() > len) {
            best = Some((name.len(), id));
        }
    }
    best.map(|(_, id)| id.clone())
}

/// Build the `(class, lowercased name)` → base-id index from the bundle,
/// preferring `Released` bases (deterministic tiebreak by smallest id).
#[must_use]
pub fn build_base_index(bundle: &poc2_data::Bundle) -> HashMap<(ItemClassId, String), BaseTypeId> {
    let mut best: HashMap<(ItemClassId, String), &BaseType> = HashMap::new();
    for b in &bundle.base_items {
        let key = (b.item_class.clone(), b.name.to_lowercase());
        match best.get(&key) {
            Some(cur) if !base_better(b, cur) => {}
            _ => {
                best.insert(key, b);
            }
        }
    }
    best.into_iter().map(|(k, b)| (k, b.id.clone())).collect()
}

/// Prefer Released bases; among equals, the lexicographically smaller id.
fn base_better(a: &BaseType, cur: &BaseType) -> bool {
    let ra = matches!(a.release_state, ReleaseState::Released);
    let rc = matches!(cur.release_state, ReleaseState::Released);
    if ra != rc {
        return ra;
    }
    a.id.as_str() < cur.id.as_str()
}
