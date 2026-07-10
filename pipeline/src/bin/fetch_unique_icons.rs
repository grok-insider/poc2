//! Fetch unique-item art from poe2db into `apps/web/public/unique-icons/`.
//!
//! Scrapes the single listing page `https://poe2db.tw/us/Unique_item`, which
//! embeds every unique's CDN `.webp` next to its display name. Soft-fails
//! (exit 0); art is regenerable/gitignored (GGG assets, not committed).
//!
//! ```text
//! cargo run -p poc2-pipeline --bin fetch-unique-icons -- \
//!   --out apps/web/public/unique-icons
//! ```

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use clap::Parser;
use regex::Regex;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

#[derive(Parser, Debug)]
#[command(name = "fetch-unique-icons")]
#[command(about = "Download poe2db unique-item icons into apps/web/public/unique-icons/.")]
struct Cli {
    /// Output directory for webp files + manifest.json.
    #[arg(long, default_value = "apps/web/public/unique-icons")]
    out: PathBuf,
    /// Re-download every image even when present.
    #[arg(long, default_value_t = false)]
    refresh: bool,
    /// Max images to download (0 = all).
    #[arg(long, default_value_t = 0)]
    limit: usize,
    /// Delay between image downloads, ms.
    #[arg(long, default_value_t = 150)]
    delay_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ManifestEntry {
    name: String,
    rel: String,
    source_url: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Manifest {
    version: u32,
    fetched_at: String,
    /// Keys are lowercased display names.
    entries: BTreeMap<String, ManifestEntry>,
}

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
        warn!(error = %e, "fetch-unique-icons errored at top level");
    }
}

async fn run(cli: Cli) -> anyhow::Result<()> {
    fs::create_dir_all(&cli.out)?;
    let manifest_path = cli.out.join("manifest.json");
    let mut manifest = load_manifest(&manifest_path);
    manifest.version = 1;
    manifest.fetched_at = iso8601_now();

    let client = Client::builder()
        .user_agent(
            "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) \
             Chrome/127.0.0.0 Safari/537.36",
        )
        .timeout(Duration::from_secs(45))
        .build()?;

    let listing = "https://poe2db.tw/us/Unique_item";
    info!(%listing, "fetching unique listing");
    let html = client
        .get(listing)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;

    // <img ... src="https://cdn.../Uniques/Facebreaker.webp" alt="Facebreaker"
    // Also accept alt before src.
    let re = Regex::new(
        r#"src="(https://cdn\.poe2db\.tw/image/Art/2DItems/[^"]+/(?:Uniques?)/[^"]+\.webp)"[^>]*alt="([^"]+)""#,
    )?;
    let re_alt_first = Regex::new(
        r#"alt="([^"]+)"[^>]*src="(https://cdn\.poe2db\.tw/image/Art/2DItems/[^"]+/(?:Uniques?)/[^"]+\.webp)""#,
    )?;

    let mut found: BTreeMap<String, (String, String)> = BTreeMap::new(); // lower name → (display, url)
    for cap in re.captures_iter(&html) {
        let url = cap[1].to_string();
        let name = cap[2].trim().to_string();
        if name.is_empty() {
            continue;
        }
        found.entry(name.to_lowercase()).or_insert((name, url));
    }
    for cap in re_alt_first.captures_iter(&html) {
        let name = cap[1].trim().to_string();
        let url = cap[2].to_string();
        if name.is_empty() {
            continue;
        }
        found.entry(name.to_lowercase()).or_insert((name, url));
    }
    info!(count = found.len(), "unique art rows parsed from listing");

    let mut downloaded = 0usize;
    let mut skipped = 0usize;
    let mut failed = 0usize;

    for (key, (name, url)) in &found {
        let file = url.rsplit('/').next().unwrap_or("icon.webp");
        let dest = cli.out.join(file);
        if dest.exists() && !cli.refresh {
            skipped += 1;
            manifest.entries.insert(
                key.clone(),
                ManifestEntry {
                    name: name.clone(),
                    rel: file.to_string(),
                    source_url: url.clone(),
                },
            );
            continue;
        }
        if cli.limit > 0 && downloaded >= cli.limit {
            skipped += 1;
            continue;
        }
        match client.get(url).send().await {
            Ok(resp) if resp.status().is_success() => match resp.bytes().await {
                Ok(bytes) if bytes.len() > 200 => {
                    fs::write(&dest, &bytes)?;
                    downloaded += 1;
                    manifest.entries.insert(
                        key.clone(),
                        ManifestEntry {
                            name: name.clone(),
                            rel: file.to_string(),
                            source_url: url.clone(),
                        },
                    );
                    tokio::time::sleep(Duration::from_millis(cli.delay_ms)).await;
                }
                _ => {
                    failed += 1;
                    warn!(%url, "empty or unreadable unique art");
                }
            },
            Ok(resp) => {
                failed += 1;
                warn!(%url, status = %resp.status(), "unique art HTTP error");
            }
            Err(e) => {
                failed += 1;
                warn!(%url, error = %e, "unique art fetch failed");
            }
        }
    }

    write_manifest(&manifest_path, &manifest)?;
    info!(
        downloaded,
        skipped,
        failed,
        entries = manifest.entries.len(),
        out = %cli.out.display(),
        "unique icon fetch complete"
    );
    Ok(())
}

fn load_manifest(path: &Path) -> Manifest {
    fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn write_manifest(path: &Path, manifest: &Manifest) -> anyhow::Result<()> {
    let json = serde_json::to_string_pretty(manifest)?;
    fs::write(path, json)?;
    Ok(())
}

fn iso8601_now() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("unix:{secs}")
}
