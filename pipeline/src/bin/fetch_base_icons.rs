//! Per-base art ingestion script.
//!
//! v2 (all-bases edition): instead of scraping one poe2db detail page per
//! base (~3,800 requests, drop_level>50 only), this scrapes the ~30
//! poe2db **class listing pages** (e.g. `/us/Shields`, `/us/Boots`). Every
//! listing row carries the base's GGPK metadata id in its `data-hover`
//! attribute (`Data\BaseItemTypes\Metadata/Items/...`) plus the item-art
//! `<img>` URL — an **exact, deduplicated join key** onto the bundle's
//! `BaseTypeId`s. No name fuzzing, full coverage (leveling bases included).
//!
//! Duplicate revalidation: rows are deduped by metadata id; conflicting
//! image URLs for the same id are counted and reported (first wins), and
//! bases sharing one art file download it once per class directory.
//!
//! Output: `apps/web/public/base-icons/<ClassPascal>/<file>.webp` plus a
//! `manifest.json` mapping every base id to its local path. The script
//! must not fail (`exit 0` always); unmatched released bases land in the
//! manifest's `missing` list.

#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::too_many_lines)]

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use clap::Parser;
use poc2_data::Bundle;
use regex::Regex;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

#[derive(Parser, Debug)]
#[command(
    about = "Download poe2db base-item icons into apps/web/public/base-icons/.",
    long_about = "Reads the loaded bundle, scrapes the poe2db class listing pages \
                  (metadata-id join, all released bases), downloads each base's \
                  item art, and writes a manifest under apps/web/public/base-icons/."
)]
struct Cli {
    /// Path to the bundle (.bundle.json or .bundle.json.gz). When omitted,
    /// the script uses `$POC2_BUNDLE`, then `~/.config/poc2/bundles/poc2.bundle.json.gz`.
    #[arg(long)]
    bundle: Option<PathBuf>,
    /// Output directory. Defaults to `apps/web/public/base-icons` relative to CWD.
    #[arg(long, default_value = "apps/web/public/base-icons")]
    out: PathBuf,
    /// Re-download every image, even ones already present locally.
    #[arg(long, default_value_t = false)]
    refresh: bool,
    /// Maximum number of images to download. 0 means all.
    #[arg(long, default_value_t = 0)]
    limit: usize,
    /// Delay between requests, in milliseconds.
    #[arg(long, default_value_t = 300)]
    delay_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ManifestEntry {
    name: String,
    class_pascal: String,
    rel: String,
    source_url: String,
    drop_level: u32,
    attribute_pool: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MissingEntry {
    name: String,
    class_pascal: String,
    reason: String,
    detail_url: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Manifest {
    version: u32,
    fetched_at: String,
    entries: BTreeMap<String, ManifestEntry>,
    missing: Vec<MissingEntry>,
}

/// Engine item-class id → poe2db listing page slug.
const CLASS_PAGES: &[(&str, &str)] = &[
    ("BodyArmour", "Body_Armours"),
    ("Boots", "Boots"),
    ("Gloves", "Gloves"),
    ("Helmet", "Helmets"),
    ("Shield", "Shields"),
    ("Buckler", "Bucklers"),
    ("Focus", "Foci"),
    ("Quiver", "Quivers"),
    ("Ring", "Rings"),
    ("Amulet", "Amulets"),
    ("Belt", "Belts"),
    ("Talisman", "Talismans"),
    ("Wand", "Wands"),
    ("Staff", "Staves"),
    ("Sceptre", "Sceptres"),
    ("Bow", "Bows"),
    ("Crossbow", "Crossbows"),
    ("Spear", "Spears"),
    ("Flail", "Flails"),
    ("Dagger", "Daggers"),
    ("Claw", "Claws"),
    ("OneHandSword", "One_Hand_Swords"),
    ("TwoHandSword", "Two_Hand_Swords"),
    ("OneHandAxe", "One_Hand_Axes"),
    ("TwoHandAxe", "Two_Hand_Axes"),
    ("OneHandMace", "One_Hand_Maces"),
    ("TwoHandMace", "Two_Hand_Maces"),
    ("Warstaff", "Quarterstaves"),
    ("Jewel", "Jewels"),
];

#[tokio::main(flavor = "current_thread")]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();
    if let Err(e) = run(cli).await {
        warn!(error = %e, "fetch-base-icons errored at top level; writing partial manifest");
    }
}

async fn run(cli: Cli) -> anyhow::Result<()> {
    let bundle_path = resolve_bundle_path(cli.bundle.as_deref())?;
    info!(path = %bundle_path.display(), "loading bundle");
    let bundle: Bundle = poc2_data::io::read_bundle(&bundle_path)?;

    fs::create_dir_all(&cli.out)?;
    let manifest_path = cli.out.join("manifest.json");
    let mut manifest = load_manifest(&manifest_path);
    manifest.version = 2;
    manifest.fetched_at = iso8601_now();
    manifest.missing.clear();

    let client = Client::builder()
        .user_agent(
            "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) \
             Chrome/127.0.0.0 Safari/537.36",
        )
        .timeout(Duration::from_secs(30))
        .build()?;

    // Every released gear base, by class. (No drop-level filter — leveling
    // bases get icons too.)
    let gear_classes: BTreeSet<&str> = CLASS_PAGES.iter().map(|(c, _)| *c).collect();
    let mut bases_by_class: BTreeMap<&str, Vec<&poc2_engine::base::BaseType>> = BTreeMap::new();
    for b in bundle
        .base_items
        .iter()
        .filter(|b| matches!(b.release_state, poc2_engine::base::ReleaseState::Released))
        // Only craftable gear classes — gems/soul cores/omens etc. have no
        // base-icon use in the crafting advisor.
        .filter(|b| gear_classes.contains(b.item_class.as_str()))
    {
        bases_by_class
            .entry(b.item_class.as_str())
            .or_default()
            .push(b);
    }

    // Listing-row pattern: the `data-hover` carries the URL-encoded GGPK id,
    // the adjacent `<img>` carries the art URL.
    let row_re = Regex::new(
        r#"data-hover="\?s=Data%5CBaseItemTypes%2F([^"]+?)"[^>]*href="([^"]*)"\s*>\s*<img[^>]*src="(https://cdn\.poe2db\.tw/image/[^"]+?\.webp)""#,
    )?;

    let mut id_to_art: BTreeMap<String, (String, String)> = BTreeMap::new(); // id → (img, detail href)
    let mut conflicts = 0usize;

    for (class_id, page) in CLASS_PAGES {
        if !bases_by_class.contains_key(class_id) {
            continue;
        }
        let url = format!("https://poe2db.tw/us/{page}");
        info!(%url, "fetching class listing");
        let html = match fetch_text(&client, &url).await {
            Ok(h) => h,
            Err(e) => {
                warn!(%url, error = %e, "listing fetch failed; bases of this class go to missing");
                continue;
            }
        };
        for cap in row_re.captures_iter(&html) {
            let raw_id = url_decode(&cap[1]);
            // `Data\BaseItemTypes\Metadata/Items/...` → take from `Metadata`.
            let id = raw_id
                .find("Metadata")
                .map_or(raw_id.clone(), |i| raw_id[i..].replace('\\', "/"));
            let img = cap[3].to_string();
            let href = cap[2].to_string();
            match id_to_art.get(&id) {
                Some((existing, _)) if existing != &img => conflicts += 1, // first wins
                Some(_) => {}
                None => {
                    id_to_art.insert(id, (img, href));
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(cli.delay_ms)).await;
    }
    info!(
        joined = id_to_art.len(),
        conflicts, "listing rows joined by metadata id (duplicates revalidated)"
    );

    let mut ok = 0usize;
    let mut skipped = 0usize;
    let mut missing = 0usize;
    let mut downloaded = 0usize;

    for (class_id, bases) in &bases_by_class {
        let class_dir = cli.out.join(class_id);
        for base in bases {
            let Some((img_url, href)) = id_to_art.get(base.id.as_str()) else {
                // Classes without listing pages (or rows poe2db doesn't
                // render) are reported, not fatal.
                manifest.missing.push(MissingEntry {
                    name: base.name.clone(),
                    class_pascal: (*class_id).to_string(),
                    reason: "no listing row matched this metadata id".into(),
                    detail_url: format!("https://poe2db.tw/us/{}", base.name.replace(' ', "_")),
                });
                missing += 1;
                continue;
            };
            let file = img_url.rsplit('/').next().unwrap_or("icon.webp");
            let rel = format!("{class_id}/{file}");
            let dest = class_dir.join(file);
            if !dest.exists() || cli.refresh {
                if cli.limit > 0 && downloaded >= cli.limit {
                    skipped += 1;
                    continue;
                }
                fs::create_dir_all(&class_dir)?;
                match fetch_bytes(&client, img_url).await {
                    Ok(bytes) if bytes.len() > 200 => {
                        fs::write(&dest, &bytes)?;
                        downloaded += 1;
                        tokio::time::sleep(Duration::from_millis(cli.delay_ms)).await;
                    }
                    Ok(_) | Err(_) => {
                        manifest.missing.push(MissingEntry {
                            name: base.name.clone(),
                            class_pascal: (*class_id).to_string(),
                            reason: "image download failed".into(),
                            detail_url: img_url.clone(),
                        });
                        missing += 1;
                        continue;
                    }
                }
            } else {
                skipped += 1;
            }
            manifest.entries.insert(
                base.id.as_str().to_string(),
                ManifestEntry {
                    name: base.name.clone(),
                    class_pascal: (*class_id).to_string(),
                    rel,
                    source_url: format!("https://poe2db.tw/us/{href}"),
                    drop_level: base.drop_level,
                    attribute_pool: format!("{:?}", base.attribute_pool),
                },
            );
            ok += 1;
        }
    }

    fs::write(&manifest_path, serde_json::to_vec_pretty(&manifest)?)?;
    info!(
        ok,
        downloaded,
        skipped,
        missing,
        conflicts,
        manifest = %manifest_path.display(),
        "base-icon fetch complete"
    );
    Ok(())
}

fn resolve_bundle_path(cli: Option<&Path>) -> anyhow::Result<PathBuf> {
    if let Some(p) = cli {
        return Ok(p.to_path_buf());
    }
    if let Ok(env) = std::env::var("POC2_BUNDLE") {
        return Ok(PathBuf::from(env));
    }
    // Same config-root fallback chain as poc2-market's cache dir:
    // XDG, then $HOME/.config, then the Windows roots (%APPDATA% is
    // already per-user config; %USERPROFILE%\.config mirrors unix).
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .or_else(|| std::env::var_os("APPDATA").map(PathBuf::from))
        .or_else(|| std::env::var_os("USERPROFILE").map(|h| PathBuf::from(h).join(".config")))
        .ok_or_else(|| {
            anyhow::anyhow!("none of XDG_CONFIG_HOME, HOME, APPDATA, USERPROFILE are set")
        })?;
    Ok(base
        .join("poc2")
        .join("bundles")
        .join("poc2.bundle.json.gz"))
}

fn load_manifest(path: &Path) -> Manifest {
    fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn iso8601_now() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("unix:{secs}")
}

async fn fetch_text(client: &Client, url: &str) -> anyhow::Result<String> {
    for attempt in 0..3u32 {
        match client
            .get(url)
            .header("Referer", "https://poe2db.tw/")
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => return Ok(resp.text().await?),
            Ok(resp) => {
                warn!(%url, status = %resp.status(), attempt, "listing fetch non-200");
            }
            Err(e) => warn!(%url, error = %e, attempt, "listing fetch error"),
        }
        tokio::time::sleep(Duration::from_millis(800 * u64::from(attempt + 1))).await;
    }
    anyhow::bail!("failed after retries: {url}")
}

async fn fetch_bytes(client: &Client, url: &str) -> anyhow::Result<Vec<u8>> {
    for attempt in 0..3u32 {
        match client
            .get(url)
            .header("Referer", "https://poe2db.tw/")
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => {
                return Ok(resp.bytes().await?.to_vec());
            }
            Ok(resp) => warn!(%url, status = %resp.status(), attempt, "image fetch non-200"),
            Err(e) => warn!(%url, error = %e, attempt, "image fetch error"),
        }
        tokio::time::sleep(Duration::from_millis(600 * u64::from(attempt + 1))).await;
    }
    anyhow::bail!("failed after retries: {url}")
}

/// Minimal percent-decoder for the `data-hover` payloads (%5C, %2F, %20…).
fn url_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}
