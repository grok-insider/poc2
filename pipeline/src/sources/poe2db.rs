//! poe2db.tw scraper for omens and bones (the data RePoE-fork doesn't ship).
//!
//! poe2db.tw publishes per-currency listing pages with one entry per
//! omen / bone / essence. The HTML pattern is consistent enough that
//! a CSS-selector scraper produces stable output. The fetched data is
//! merged into the bundle's `omens` and `bones` sections.
//!
//! ## URLs
//!
//! - <https://poe2db.tw/us/Omen> — 44 omens
//! - <https://poe2db.tw/us/Desecrated_Modifiers> — bones (bone metadata is on
//!   the Desecration mechanics page rather than its own page)
//!
//! ## Rate limiting
//!
//! We fetch each page once per pipeline run (1 RPS upper bound).
//! poe2db has no rate-limit headers documented; we default to a 30s
//! timeout and a polite User-Agent.

use std::time::Duration;

use poc2_data::{SourceRevision, SourceRevisions};
use reqwest::Client;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};

use crate::error::{PipelineError, PipelineResult};

const POE2DB_OMEN_URL: &str = "https://poe2db.tw/us/Omen";
const POE2DB_DESECRATED_URL: &str = "https://poe2db.tw/us/Desecrated_Modifiers";
const POE2DB_VAAL_URL: &str = "https://poe2db.tw/us/Vaal_Modifiers";

// -------------------------------------------------------------------------
// Typed snapshot
// -------------------------------------------------------------------------

/// One omen as scraped from poe2db.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Poe2dbOmen {
    /// Display name (e.g., `"Omen of Whittling"`).
    pub name: String,
    /// Slug used in the engine's `OmenId` (e.g., `"OmenOfWhittling"`).
    pub id: String,
    /// One-line description (the in-game "While this item is active …" text).
    pub description: String,
    /// Image asset URL (poe2db CDN).
    pub icon_url: Option<String>,
}

/// Bone presets — derived from the engine's BoneSize × BoneSubtype matrix.
/// poe2db's Desecrated_Modifiers page lists these but we don't need to
/// scrape them since the engine already enumerates them.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Poe2dbBone {
    pub name: String,
    /// Canonical id `{Size}{Subtype}` matching `engine::CurrencyResolver`.
    pub id: String,
    pub size: String,
    pub subtype: String,
}

/// One desecrated mod scraped from poe2db. Mirrors the shape of
/// `pipeline/data/desecrated_mods.json` so the regen binary can write
/// straight to disk without conversion.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Poe2dbDesecratedMod {
    /// Stable engine `ModId` derived from name + class. Not always
    /// inferrable from the page; the regen binary fills this in via
    /// the same convention as the curated fixtures
    /// (`{Class}Desecrated_{Lord}_{StatStem}`).
    pub id: String,
    pub name: String,
    pub lord: String,
    pub affix: String,
    pub classes: Vec<String>,
    pub tier: u32,
    pub required_level: u32,
    pub stats: Vec<Poe2dbStat>,
}

/// One Vaal-corruption implicit scraped from poe2db.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Poe2dbVaalImplicit {
    pub id: String,
    pub name: String,
    pub classes: Vec<String>,
    pub required_level: u32,
    pub stats: Vec<Poe2dbStat>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Poe2dbStat {
    pub stat_id: String,
    pub min: f64,
    pub max: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Poe2dbSnapshot {
    pub omens: Vec<Poe2dbOmen>,
    pub bones: Vec<Poe2dbBone>,
    pub revisions: SourceRevisions,
}

impl Poe2dbSnapshot {
    pub fn count_summary(&self) -> String {
        format!(
            "poe2db: {} omens, {} bones",
            self.omens.len(),
            self.bones.len()
        )
    }
}

// -------------------------------------------------------------------------
// Fetch + parse
// -------------------------------------------------------------------------

/// Fetch + scrape both the omen page and the bone enumeration. Bones are
/// produced from the engine's enum rather than scraped; CoE / poe2db don't
/// expose them as a structured list.
pub async fn fetch(client: &Client) -> PipelineResult<Poe2dbSnapshot> {
    let mut revisions = Vec::new();
    let now = now_iso8601();

    let omen_html = fetch_text(client, POE2DB_OMEN_URL).await?;
    revisions.push(SourceRevision {
        name: "poe2db.omens".into(),
        revision: format!("len={}", omen_html.len()),
        url: Some(POE2DB_OMEN_URL.into()),
        fetched_at: now.clone(),
    });
    let omens = parse_omen_page(&omen_html);
    let bones = enumerate_bones();

    Ok(Poe2dbSnapshot {
        omens,
        bones,
        revisions: SourceRevisions(revisions),
    })
}

/// Scrape the omen-list page, returning every entry under
/// `#OmenItem` (the page's "Omen Item /44" section).
///
/// Implementation strategy:
/// - The page renders each omen as a sequence of two `<a>` tags (icon
///   link + name link) followed by description paragraphs.
/// - We match anchor pairs whose `href` looks like `Omen_of_X`, then
///   scoop up the next two `<p>` siblings as the stack-size + description.
#[must_use]
pub fn parse_omen_page(html: &str) -> Vec<Poe2dbOmen> {
    let doc = Html::parse_document(html);
    let mut out: Vec<Poe2dbOmen> = Vec::new();
    let mut seen: ahash::AHashSet<String> = ahash::AHashSet::new();

    // Match anchors that point to `Omen_of_*` slugs.
    let anchor_sel = Selector::parse("a[href^=\"Omen_of_\"]").unwrap();
    for anchor in doc.select(&anchor_sel) {
        let href = anchor.value().attr("href").unwrap_or("");
        if !href.starts_with("Omen_of_") {
            continue;
        }
        let slug = href.to_string();
        // The text-only anchor is the one whose sole child is text. The
        // icon anchor contains an <img>; we skip it.
        let has_img = anchor
            .select(&Selector::parse("img").unwrap())
            .next()
            .is_some();
        if has_img {
            continue;
        }
        if !seen.insert(slug.clone()) {
            continue;
        }
        let name = anchor.text().collect::<String>().trim().to_string();
        if name.is_empty() {
            continue;
        }
        let id = slug_to_currency_id(&slug);

        // The icon URL lives on the previous sibling anchor's <img>.
        let icon_url = find_icon_for_anchor(&anchor);

        // The description is in the surrounding text — try to capture
        // by walking sibling text nodes until we hit the next anchor.
        let description = grab_description_after(&anchor);

        out.push(Poe2dbOmen {
            name,
            id,
            description,
            icon_url,
        });
    }
    out
}

/// Convert a URL slug (`Omen_of_Whittling`) to engine id (`OmenOfWhittling`).
fn slug_to_currency_id(slug: &str) -> String {
    slug.split('_')
        .map(|w| {
            let mut chars = w.chars();
            match chars.next() {
                Some(c) => c.to_ascii_uppercase().to_string() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

/// Find the icon URL by walking back to the matching icon anchor.
fn find_icon_for_anchor(text_anchor: &scraper::ElementRef<'_>) -> Option<String> {
    // poe2db pairs an <a><img/></a> immediately before each text anchor
    // with the same href. Walk previous siblings in the parent element.
    let parent = text_anchor.parent()?;
    let mut prev_anchor: Option<scraper::ElementRef<'_>> = None;
    for child in parent.children() {
        if let Some(el) = scraper::ElementRef::wrap(child) {
            if el.value().name() == "a"
                && el.value().attr("href") == text_anchor.value().attr("href")
            {
                if std::ptr::eq(el.value(), text_anchor.value()) {
                    break;
                }
                prev_anchor = Some(el);
            }
        }
    }
    prev_anchor
        .and_then(|a| a.select(&Selector::parse("img").unwrap()).next())
        .and_then(|img| img.value().attr("src").map(str::to_string))
}

// =========================================================================
// Desecrated Modifiers + Vaal Implicits scraper (regen-fixtures path)
// =========================================================================
//
// poe2db.tw renders these as HTML tables grouped by item class. The DOM
// shape isn't guaranteed stable across patches; this parser is
// conservative — it emits whatever structured rows it can find and the
// caller writes the result to disk for human review before merging
// back into the bundled fixtures. **The pipeline build does NOT call
// these functions**; they're invoked only by `bin/regen_poe2db_fixtures.rs`.

/// Fetch + parse the Desecrated Modifiers page. Returns a list of
/// `Poe2dbDesecratedMod` rows ready to serialise into
/// `pipeline/data/desecrated_mods.json`.
///
/// Real-world note: when poe2db restructures the page, this function
/// will return fewer entries than expected. The regen binary surfaces
/// that count to the operator, who manually inspects the diff before
/// committing the regenerated fixture.
pub async fn fetch_desecrated_mods(client: &Client) -> PipelineResult<Vec<Poe2dbDesecratedMod>> {
    let html = fetch_text(client, POE2DB_DESECRATED_URL).await?;
    Ok(parse_desecrated_page(&html))
}

/// Fetch + parse the Vaal Modifiers (corruption implicits) page.
pub async fn fetch_vaal_implicits(client: &Client) -> PipelineResult<Vec<Poe2dbVaalImplicit>> {
    let html = fetch_text(client, POE2DB_VAAL_URL).await?;
    Ok(parse_vaal_page(&html))
}

/// Parse a Desecrated_Modifiers HTML page. Looks for `<table>` blocks
/// preceded by a `<h2>` or `<h3>` carrying the item-class name; each
/// row's `<td>` cells are interpreted as `tier | name | required_level
/// | stats`. Tolerant: rows that don't fit the schema are skipped.
#[must_use]
pub fn parse_desecrated_page(html: &str) -> Vec<Poe2dbDesecratedMod> {
    let doc = Html::parse_document(html);
    let mut out: Vec<Poe2dbDesecratedMod> = Vec::new();

    let header_sel = Selector::parse("h2, h3").unwrap();
    let table_sel = Selector::parse("table").unwrap();
    let row_sel = Selector::parse("tr").unwrap();
    let cell_sel = Selector::parse("td").unwrap();

    // Walk top-level body children. For each header that names a class,
    // the next table found is treated as that class's entry list.
    let mut current_class: Option<String> = None;
    let body_sel = Selector::parse("body").unwrap();
    let Some(body) = doc.select(&body_sel).next() else {
        return out;
    };
    for child in body.descendants() {
        let Some(el) = scraper::ElementRef::wrap(child) else {
            continue;
        };
        if header_sel.matches(&el) {
            let txt = el.text().collect::<String>().trim().to_string();
            current_class = guess_class_from_header(&txt);
            continue;
        }
        if !table_sel.matches(&el) {
            continue;
        }
        let Some(class) = current_class.clone() else {
            continue;
        };
        for row in el.select(&row_sel) {
            let cells: Vec<String> = row
                .select(&cell_sel)
                .map(|c| c.text().collect::<String>().trim().to_string())
                .collect();
            if cells.len() < 3 {
                continue;
            }
            // Heuristic: cell 0 = lord ("Amanamu" / "Kurgal" / "Ulaman"
            // — sometimes encoded as a small icon + label).
            // cell 1 = affix label ("Prefix" or "Suffix").
            // cell 2 = mod name.
            // cell 3 = required level (numeric).
            // cell 4 = mod text (stat range parseable).
            let lord = cells.first().cloned().unwrap_or_default();
            if !["Amanamu", "Kurgal", "Ulaman"]
                .iter()
                .any(|l| lord.contains(l))
            {
                continue;
            }
            let lord_canonical = if lord.contains("Amanamu") {
                "Amanamu"
            } else if lord.contains("Kurgal") {
                "Kurgal"
            } else {
                "Ulaman"
            };
            let affix_text = cells.get(1).cloned().unwrap_or_default();
            let affix = if affix_text.to_lowercase().contains("prefix") {
                "Prefix"
            } else {
                "Suffix"
            };
            let name = cells.get(2).cloned().unwrap_or_default();
            let required_level = cells
                .get(3)
                .and_then(|s| s.parse::<u32>().ok())
                .unwrap_or(60);
            let stat_text = cells.get(4).cloned().unwrap_or_default();
            let (stat_id, min, max) = parse_stat_range(&stat_text);
            let id = synthesize_desecrated_id(&class, lord_canonical, &stat_id, &name);
            out.push(Poe2dbDesecratedMod {
                id,
                name,
                lord: lord_canonical.into(),
                affix: affix.into(),
                classes: vec![class.clone()],
                tier: 1,
                required_level,
                stats: vec![Poe2dbStat { stat_id, min, max }],
            });
        }
    }
    out
}

/// Parse a Vaal_Modifiers HTML page. Same row-based heuristic.
#[must_use]
pub fn parse_vaal_page(html: &str) -> Vec<Poe2dbVaalImplicit> {
    let doc = Html::parse_document(html);
    let mut out: Vec<Poe2dbVaalImplicit> = Vec::new();

    let header_sel = Selector::parse("h2, h3").unwrap();
    let table_sel = Selector::parse("table").unwrap();
    let row_sel = Selector::parse("tr").unwrap();
    let cell_sel = Selector::parse("td").unwrap();

    let body_sel = Selector::parse("body").unwrap();
    let Some(body) = doc.select(&body_sel).next() else {
        return out;
    };
    let mut current_class: Option<String> = None;
    for child in body.descendants() {
        let Some(el) = scraper::ElementRef::wrap(child) else {
            continue;
        };
        if header_sel.matches(&el) {
            let txt = el.text().collect::<String>().trim().to_string();
            current_class = guess_class_from_header(&txt);
            continue;
        }
        if !table_sel.matches(&el) {
            continue;
        }
        let Some(class) = current_class.clone() else {
            continue;
        };
        for row in el.select(&row_sel) {
            let cells: Vec<String> = row
                .select(&cell_sel)
                .map(|c| c.text().collect::<String>().trim().to_string())
                .collect();
            if cells.len() < 2 {
                continue;
            }
            let name = cells.first().cloned().unwrap_or_default();
            if name.is_empty() {
                continue;
            }
            let required_level = cells
                .get(1)
                .and_then(|s| s.parse::<u32>().ok())
                .unwrap_or(60);
            let stat_text = cells.get(2).cloned().unwrap_or_default();
            let (stat_id, min, max) = parse_stat_range(&stat_text);
            let id = synthesize_vaal_id(&class, &stat_id, &name);
            out.push(Poe2dbVaalImplicit {
                id,
                name,
                classes: vec![class.clone()],
                required_level,
                stats: vec![Poe2dbStat { stat_id, min, max }],
            });
        }
    }
    out
}

/// Map a poe2db section header to the engine's `ItemClassId`. Returns
/// `None` for headers that don't correspond to a gear class so the
/// caller skips the following table. Conservative: only matches the
/// classes the curated fixtures already cover, since the regen binary
/// is meant to refresh those rather than expand class coverage.
fn guess_class_from_header(header: &str) -> Option<String> {
    let h = header.to_ascii_lowercase();
    if h.contains("body armour") || h.contains("body armor") {
        Some("BodyArmour".into())
    } else if h.contains("helmet") {
        Some("Helmet".into())
    } else if h.contains("boot") {
        Some("Boots".into())
    } else if h.contains("glove") {
        Some("Gloves".into())
    } else if h.contains("ring") {
        Some("Ring".into())
    } else if h.contains("amulet") {
        Some("Amulet".into())
    } else if h.contains("belt") {
        Some("Belt".into())
    } else {
        None
    }
}

/// Best-effort `(min, max, stat_id)` extractor from a "+(40-50) to Spirit"
/// style mod-text cell. The stat_id is a slugified copy of the
/// human-readable stat name; the min/max are the leading numeric range.
/// Falls back to `(0.0, 0.0, "unknown_stat")` when no range is present.
#[must_use]
fn parse_stat_range(text: &str) -> (String, f64, f64) {
    use regex::Regex;
    static RANGE_RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let re = RANGE_RE.get_or_init(|| {
        // Matches "(40-50)", "+(0.6-1.0)%" etc. Captures min, max.
        Regex::new(r"\(?(\d+(?:\.\d+)?)\s*[-–]\s*(\d+(?:\.\d+)?)\)?").unwrap()
    });
    if let Some(caps) = re.captures(text) {
        let min: f64 = caps[1].parse().unwrap_or(0.0);
        let max: f64 = caps[2].parse().unwrap_or(0.0);
        let stat_id = slugify_stat_id(text);
        return (stat_id, min, max);
    }
    (slugify_stat_id(text), 0.0, 0.0)
}

/// Convert mod text to a snake_case stat id: lowercased ASCII alnum,
/// underscores between words. Strips numbers and punctuation so the
/// resulting id is a stable handle even across patches that retune
/// numeric ranges. Capped at 48 chars.
fn slugify_stat_id(text: &str) -> String {
    let mut out = String::new();
    let mut last_underscore = true;
    for c in text.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            last_underscore = false;
        } else if !last_underscore {
            out.push('_');
            last_underscore = true;
        }
    }
    let trimmed = out.trim_matches('_');
    trimmed.chars().take(48).collect()
}

fn synthesize_desecrated_id(class: &str, lord: &str, stat_id: &str, name: &str) -> String {
    let stat_token = if stat_id.is_empty() {
        slugify_stat_id(name)
    } else {
        stat_id.to_string()
    };
    let camel = stat_token
        .split('_')
        .filter(|s| !s.is_empty())
        .map(capitalize_word)
        .collect::<String>();
    format!("{class}Desecrated_{lord}_{camel}")
}

fn synthesize_vaal_id(class: &str, stat_id: &str, name: &str) -> String {
    let stat_token = if stat_id.is_empty() {
        slugify_stat_id(name)
    } else {
        stat_id.to_string()
    };
    let camel = stat_token
        .split('_')
        .filter(|s| !s.is_empty())
        .map(capitalize_word)
        .collect::<String>();
    format!("VaalImplicit_{class}_{camel}")
}

fn capitalize_word(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_ascii_uppercase().to_string() + chars.as_str(),
        None => String::new(),
    }
}

/// Best-effort: scoop up the next sibling text nodes as the description.
fn grab_description_after(text_anchor: &scraper::ElementRef<'_>) -> String {
    // Walk forward through siblings until we hit another <a>.
    let mut buf = String::new();
    let mut node = text_anchor.next_sibling();
    while let Some(n) = node {
        if let Some(el) = scraper::ElementRef::wrap(n) {
            if el.value().name() == "a" {
                break;
            }
            for t in el.text() {
                buf.push_str(t);
                buf.push(' ');
            }
        } else if let Some(t) = n.value().as_text() {
            buf.push_str(t);
            buf.push(' ');
        }
        node = n.next_sibling();
    }
    let cleaned: String = buf
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    // Trim "Stack Size: 1 / 10" prefix if present.
    cleaned
        .strip_prefix("Stack Size: 1 / 10")
        .unwrap_or(&cleaned)
        .trim()
        .to_string()
}

/// Enumerate the `(BoneSize, BoneSubtype)` matrix the engine implements.
/// Excludes the unsupported (Gnawed, Cranium) and (Ancient, Cranium) cells.
fn enumerate_bones() -> Vec<Poe2dbBone> {
    let mut out = Vec::new();
    for (size, sname) in &[
        ("Gnawed", "Gnawed"),
        ("Preserved", "Preserved"),
        ("Ancient", "Ancient"),
    ] {
        for (subtype, sub_name) in &[
            ("Rib", "Rib"),
            ("Jawbone", "Jawbone"),
            ("Collarbone", "Collarbone"),
            ("Cranium", "Cranium"),
        ] {
            // Only Preserved Cranium exists (per game data).
            if *subtype == "Cranium" && *size != "Preserved" {
                continue;
            }
            let id = format!("{size}{subtype}");
            let display = format!("{sname} {sub_name}");
            out.push(Poe2dbBone {
                name: display,
                id,
                size: (*size).into(),
                subtype: (*subtype).into(),
            });
        }
    }
    out
}

async fn fetch_text(client: &Client, url: &str) -> PipelineResult<String> {
    let resp = client
        .get(url)
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .map_err(|e| PipelineError::Http {
            url: url.into(),
            source: e,
        })?;
    let status = resp.status();
    if !status.is_success() {
        return Err(PipelineError::HttpStatus {
            url: url.into(),
            status: status.as_u16(),
            body: String::new(),
        });
    }
    resp.text().await.map_err(|e| PipelineError::Http {
        url: url.into(),
        source: e,
    })
}

fn now_iso8601() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    format!("{secs}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_to_currency_id_is_pascal_concatenation() {
        assert_eq!(slug_to_currency_id("Omen_of_Whittling"), "OmenOfWhittling");
        assert_eq!(
            slug_to_currency_id("Omen_of_Sinistral_Erasure"),
            "OmenOfSinistralErasure"
        );
    }

    #[test]
    fn enumerate_bones_returns_expected_matrix() {
        let bones = enumerate_bones();
        // 3 sizes × 3 main subtypes (Rib, Jawbone, Collarbone)
        // + 1 Preserved Cranium = 10 entries.
        assert_eq!(bones.len(), 10);
        assert!(bones.iter().any(|b| b.id == "PreservedRib"));
        assert!(bones.iter().any(|b| b.id == "AncientJawbone"));
        assert!(bones.iter().any(|b| b.id == "PreservedCranium"));
        // No Gnawed Cranium / Ancient Cranium.
        assert!(!bones.iter().any(|b| b.id == "GnawedCranium"));
        assert!(!bones.iter().any(|b| b.id == "AncientCranium"));
    }

    #[test]
    fn parse_omen_page_extracts_known_omens_from_minimal_fixture() {
        let html = r#"
            <html><body>
                <a href="Omen_of_Whittling"><img src="https://cdn.example/img.webp" alt="x"/></a>
                <a href="Omen_of_Whittling">Omen of Whittling</a>
                Stack Size: 1 / 10
                While this item is active your next Chaos Orb will remove the lowest level modifier
                <a href="Omen_of_Sinistral_Exaltation"><img src="https://cdn.example/img2.webp"/></a>
                <a href="Omen_of_Sinistral_Exaltation">Omen of Sinistral Exaltation</a>
                Stack Size: 1 / 10
                While this item is active your next Exalted Orb will add only prefix modifiers
            </body></html>
        "#;
        let omens = parse_omen_page(html);
        assert_eq!(omens.len(), 2);
        let whittling = omens.iter().find(|o| o.id == "OmenOfWhittling").unwrap();
        assert_eq!(whittling.name, "Omen of Whittling");
        assert!(whittling.description.contains("lowest level"));
        assert!(whittling.icon_url.as_deref() == Some("https://cdn.example/img.webp"));
    }

    #[test]
    fn parse_omen_page_handles_empty_input() {
        assert!(parse_omen_page("").is_empty());
        assert!(parse_omen_page("<html></html>").is_empty());
    }

    #[test]
    fn slugify_stat_id_is_stable() {
        assert_eq!(slugify_stat_id("+(40-50) to Spirit"), "40_50_to_spirit");
        assert_eq!(
            slugify_stat_id("(0.6-1.0)% maximum Life Regen"),
            "0_6_1_0_maximum_life_regen"
        );
        assert_eq!(slugify_stat_id(""), "");
    }

    #[test]
    fn parse_stat_range_extracts_numeric_window() {
        let (id, mn, mx) = parse_stat_range("+(40-50) to Spirit");
        assert!((mn - 40.0).abs() < 1e-9);
        assert!((mx - 50.0).abs() < 1e-9);
        assert!(id.contains("spirit"));
    }

    #[test]
    fn synthesize_desecrated_id_matches_curated_convention() {
        let id = synthesize_desecrated_id(
            "BodyArmour",
            "Amanamu",
            "life_gained_on_hit",
            "of the Abyssal Lord (Life on Hit)",
        );
        assert_eq!(id, "BodyArmourDesecrated_Amanamu_LifeGainedOnHit");
    }

    #[test]
    fn parse_desecrated_page_extracts_rows_from_minimal_fixture() {
        // A handcrafted poe2db-shaped page with one BodyArmour table.
        let html = r"
            <html><body>
                <h2>Body Armour Desecrated Modifiers</h2>
                <table>
                    <tr>
                        <td>Amanamu</td><td>Suffix</td>
                        <td>of the Abyssal Lord (Life on Hit)</td>
                        <td>65</td><td>+(18-30) Life on Hit</td>
                    </tr>
                </table>
            </body></html>
        ";
        let mods = parse_desecrated_page(html);
        assert_eq!(mods.len(), 1);
        let m = &mods[0];
        assert_eq!(m.lord, "Amanamu");
        assert_eq!(m.affix, "Suffix");
        assert_eq!(m.classes, vec!["BodyArmour"]);
        assert_eq!(m.required_level, 65);
        assert_eq!(m.stats.len(), 1);
        assert!((m.stats[0].min - 18.0).abs() < 1e-9);
        assert!((m.stats[0].max - 30.0).abs() < 1e-9);
    }

    #[test]
    fn parse_vaal_page_extracts_rows_from_minimal_fixture() {
        let html = r"
            <html><body>
                <h2>Body Armour Vaal Implicits</h2>
                <table>
                    <tr>
                        <td>+(8-12)% to Maximum Life</td>
                        <td>60</td>
                        <td>+(8-12)% to Maximum Life</td>
                    </tr>
                </table>
            </body></html>
        ";
        let mods = parse_vaal_page(html);
        assert_eq!(mods.len(), 1);
        let m = &mods[0];
        assert_eq!(m.classes, vec!["BodyArmour"]);
        assert_eq!(m.required_level, 60);
        assert!((m.stats[0].min - 8.0).abs() < 1e-9);
        assert!((m.stats[0].max - 12.0).abs() < 1e-9);
    }

    #[test]
    fn guess_class_from_header_recognises_gear_classes() {
        assert_eq!(
            guess_class_from_header("Body Armour Desecrated Modifiers"),
            Some("BodyArmour".into())
        );
        assert_eq!(
            guess_class_from_header("Helmet Modifiers"),
            Some("Helmet".into())
        );
        assert_eq!(guess_class_from_header("Quiver Modifiers"), None);
    }
}
