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
}
