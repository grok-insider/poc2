//! Per-base art ingestion script (Phase 8).
//!
//! Reads the latest bundle from `~/.config/poc2/bundles/poc2.bundle.json.gz`
//! (or `$POC2_BUNDLE`), iterates released base items, scrapes the
//! corresponding poe2db detail page for the `Art/2DItems/.../Basetypes/<File>.webp`
//! URL, downloads each image into
//! `apps/desktop/public/base-icons/<class-pascal>/<file>.webp`, and writes
//! a `manifest.json` mapping every base id to its local path.
//!
//! The script must not fail (`exit 0` always). Bases that can't be
//! resolved end up in the `missing` list of the manifest with a reason.

#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::too_many_lines)]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use clap::Parser;
use poc2_data::Bundle;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::{info, warn};

#[derive(Parser, Debug)]
#[command(
    about = "Download poe2db base-item icons into apps/desktop/public/base-icons/.",
    long_about = "Reads the loaded bundle, iterates released base items, scrapes \
                  poe2db.tw detail pages for the canonical Art/2DItems/.../Basetypes URL, \
                  downloads each image, and writes a manifest under apps/desktop/public/base-icons/."
)]
struct Cli {
    /// Path to the bundle (.bundle.json or .bundle.json.gz). When omitted,
    /// the script uses `$POC2_BUNDLE`, then `~/.config/poc2/bundles/poc2.bundle.json.gz`.
    #[arg(long)]
    bundle: Option<PathBuf>,
    /// Output directory. Defaults to `apps/desktop/public/base-icons` relative to CWD.
    #[arg(long, default_value = "apps/desktop/public/base-icons")]
    out: PathBuf,
    /// Re-download every base, even ones already present locally.
    #[arg(long, default_value_t = false)]
    refresh: bool,
    /// Maximum number of bases to process. 0 means all.
    #[arg(long, default_value_t = 0)]
    limit: usize,
    /// Delay between requests, in milliseconds.
    #[arg(long, default_value_t = 500)]
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
    manifest.version = 1;
    manifest.fetched_at = iso8601_now();

    let client = Client::builder()
        .user_agent(
            "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) \
             Chrome/127.0.0.0 Safari/537.36",
        )
        .timeout(Duration::from_secs(30))
        .build()?;

    let bases: Vec<_> = bundle
        .base_items
        .iter()
        .filter(|b| matches!(b.release_state, poc2_engine::base::ReleaseState::Released))
        .filter(|b| is_gear_class(b.item_class.as_str()))
        .collect();

    info!(total = bases.len(), "released gear bases");

    let mut ok = 0usize;
    let mut skipped = 0usize;
    let mut missing = 0usize;

    for (processed, base) in bases.into_iter().enumerate() {
        if cli.limit > 0 && processed >= cli.limit {
            break;
        }

        let class_pascal = pascal_class(base.item_class.as_str());
        let id = base.id.as_str().to_string();

        if !cli.refresh {
            if let Some(existing) = manifest.entries.get(&id) {
                let dest = cli.out.join(&existing.rel);
                if dest.is_file() {
                    skipped += 1;
                    continue;
                }
            }
        }

        let detail_url = poe2db_url(&base.name);

        // Drain any previous "missing" record for this id; we'll re-add if it fails.
        manifest.missing.retain(|m| m.name != base.name);

        match fetch_one(&client, &detail_url).await {
            Ok((source_url, file_name)) => {
                let rel = format!("{class_pascal}/{file_name}");
                let dest = cli.out.join(&rel);
                if let Some(parent) = dest.parent() {
                    fs::create_dir_all(parent)?;
                }
                match download_to(&client, &source_url, &dest, &detail_url).await {
                    Ok(()) => {
                        manifest.entries.insert(
                            id.clone(),
                            ManifestEntry {
                                name: base.name.clone(),
                                class_pascal: class_pascal.clone(),
                                rel,
                                source_url,
                                drop_level: base.drop_level,
                                attribute_pool: format!("{:?}", base.attribute_pool)
                                    .to_ascii_lowercase(),
                            },
                        );
                        ok += 1;
                        info!(
                            base = %base.name,
                            class = %class_pascal,
                            file = %dest.display(),
                            "ok"
                        );
                    }
                    Err(e) => {
                        manifest.missing.push(MissingEntry {
                            name: base.name.clone(),
                            class_pascal: class_pascal.clone(),
                            reason: format!("download: {e}"),
                            detail_url: detail_url.clone(),
                        });
                        missing += 1;
                        warn!(base = %base.name, error = %e, "download failed");
                    }
                }
            }
            Err(e) => {
                manifest.missing.push(MissingEntry {
                    name: base.name.clone(),
                    class_pascal: class_pascal.clone(),
                    reason: format!("scrape: {e}"),
                    detail_url: detail_url.clone(),
                });
                missing += 1;
                warn!(base = %base.name, error = %e, "scrape failed");
            }
        }

        write_manifest(&manifest_path, &manifest)?;
        tokio::time::sleep(Duration::from_millis(cli.delay_ms)).await;
    }

    write_manifest(&manifest_path, &manifest)?;
    info!(ok, skipped, missing, total = ok + skipped + missing, "done");
    Ok(())
}

fn resolve_bundle_path(arg: Option<&Path>) -> anyhow::Result<PathBuf> {
    if let Some(p) = arg {
        return Ok(p.to_path_buf());
    }
    if let Ok(env) = std::env::var("POC2_BUNDLE") {
        return Ok(PathBuf::from(env));
    }
    let xdg = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| Path::new(&h).join(".config")))
        .ok_or_else(|| anyhow::anyhow!("no $XDG_CONFIG_HOME or $HOME"))?;
    let candidate = xdg.join("poc2/bundles/poc2.bundle.json.gz");
    if candidate.is_file() {
        return Ok(candidate);
    }
    Err(anyhow::anyhow!(
        "no bundle found; pass --bundle or set $POC2_BUNDLE"
    ))
}

fn load_manifest(path: &Path) -> Manifest {
    fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str::<Manifest>(&s).ok())
        .unwrap_or_default()
}

fn write_manifest(path: &Path, manifest: &Manifest) -> anyhow::Result<()> {
    let serialized = serde_json::to_string_pretty(manifest)?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, serialized)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

/// Translate the bundle's `item_class` (often display-cased like `"Body Armour"`)
/// into PascalCase used by the engine and frontend (`"BodyArmour"`).
fn pascal_class(raw: &str) -> String {
    let mut s = String::with_capacity(raw.len());
    let mut up = true;
    for c in raw.chars() {
        if c.is_whitespace() || c == '-' || c == '_' {
            up = true;
        } else if up {
            for u in c.to_uppercase() {
                s.push(u);
            }
            up = false;
        } else {
            s.push(c);
        }
    }
    s
}

fn is_gear_class(raw: &str) -> bool {
    matches!(
        raw,
        "Body Armour"
            | "Helmet"
            | "Gloves"
            | "Boots"
            | "Shield"
            | "Buckler"
            | "Quiver"
            | "Focus"
            | "Belt"
            | "Amulet"
            | "Ring"
            | "Bow"
            | "Crossbow"
            | "Wand"
            | "Sceptre"
            | "Staff"
            | "Warstaff"
            | "Quarterstaff"
            | "Spear"
            | "Flail"
            | "Claw"
            | "Dagger"
            | "One Hand Sword"
            | "One Hand Axe"
            | "One Hand Mace"
            | "Two Hand Sword"
            | "Two Hand Axe"
            | "Two Hand Mace"
    )
}

fn poe2db_url(name: &str) -> String {
    let slug: String = name
        .chars()
        .map(|c| if c == ' ' { '_' } else { c })
        .collect();
    format!("https://poe2db.tw/us/{slug}")
}

async fn fetch_one(client: &Client, detail_url: &str) -> anyhow::Result<(String, String)> {
    let html = with_retry(|| async {
        let resp = client.get(detail_url).send().await?.error_for_status()?;
        let text = resp.text().await?;
        Ok::<_, anyhow::Error>(text)
    })
    .await?;
    let url = pick_base_image_url(&html)
        .ok_or_else(|| anyhow::anyhow!("no Art/2DItems Basetypes URL on page"))?;
    let file_name = url
        .rsplit('/')
        .next()
        .map(str::to_string)
        .unwrap_or_else(|| "icon.webp".into());
    Ok((url, file_name))
}

/// Search the rendered HTML for a poe2db CDN URL matching
/// `Art/2DItems/.../Basetypes/<File>.webp`. Falls back to the page's
/// `og:image` if the inline match isn't found.
fn pick_base_image_url(html: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let needle = "art/2ditems/";
    let mut from = 0;
    while let Some(rel) = lower[from..].find(needle) {
        let i = from + rel;
        let start = html[..i].rfind("https://").unwrap_or(0);
        let end = html[i..].find(".webp")?;
        let candidate = &html[start..(i + end + ".webp".len())];
        if candidate.to_ascii_lowercase().contains("/basetypes/") {
            return Some(candidate.to_string());
        }
        from = i + needle.len();
    }
    extract_og_image(html)
}

fn extract_og_image(html: &str) -> Option<String> {
    for marker in [
        "property=\"og:image\"",
        "property='og:image'",
        "name=\"og:image\"",
        "name='og:image'",
    ] {
        let Some(pos) = html.find(marker) else {
            continue;
        };
        let tail = &html[pos..html.len().min(pos + 600)];
        if let Some(content_pos) = tail.find("content=") {
            let after = &tail[content_pos + "content=".len()..];
            let quote = after.chars().next()?;
            if quote != '"' && quote != '\'' {
                continue;
            }
            let rest = &after[quote.len_utf8()..];
            let end = rest.find(quote)?;
            return Some(rest[..end].to_string());
        }
    }
    None
}

async fn download_to(client: &Client, url: &str, dest: &Path, referer: &str) -> anyhow::Result<()> {
    let bytes = with_retry(|| async {
        let resp = client
            .get(url)
            .header(reqwest::header::REFERER, referer)
            .header(reqwest::header::ACCEPT, "image/webp,image/*,*/*;q=0.8")
            .send()
            .await?
            .error_for_status()?;
        let b = resp.bytes().await?;
        Ok::<_, anyhow::Error>(b)
    })
    .await?;
    let tmp = dest.with_extension("webp.tmp");
    fs::write(&tmp, &bytes)?;
    fs::rename(&tmp, dest)?;
    Ok(())
}

async fn with_retry<T, F, Fut>(f: F) -> anyhow::Result<T>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<T, anyhow::Error>>,
{
    let mut delay = Duration::from_millis(500);
    let mut last_err = None;
    for attempt in 0..3 {
        match f().await {
            Ok(v) => return Ok(v),
            Err(e) => {
                last_err = Some(e);
                if attempt < 2 {
                    tokio::time::sleep(delay).await;
                    delay *= 2;
                }
            }
        }
    }
    Err(last_err.unwrap_or_else(|| anyhow::anyhow!("retry exhausted")))
}

fn iso8601_now() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    secs.to_string()
}

// Avoid pulling in the full Value parser on the hot path; we only need it
// for ad-hoc debug logs if we ever inspect a partial JSON response.
#[allow(dead_code)]
fn debug_value(s: &str) -> Option<Value> {
    serde_json::from_str(s).ok()
}
