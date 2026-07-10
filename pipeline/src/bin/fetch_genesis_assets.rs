//! Fetch Genesis Tree art assets (node icons + Wombgift item art) into
//! `apps/web/public/genesis-icons/` with a small manifest.
//!
//! Sources (see docs/83 + the Genesis research notes):
//! - Node icon classes (`Keepers*` + `MasteryBlank`): poe2db CDN
//!   `https://cdn.poe2db.tw/image/Art/2DArt/SkillIcons/passives/<Name>.webp`
//! - Wombgift art (`BreachFruit1..5`): poe2db CDN
//!   `https://cdn.poe2db.tw/image/Art/2DItems/Currency/Breach/<Name>.webp`,
//!   falling back to RePoE-fork PNGs
//!   `https://repoe-fork.github.io/poe2/Art/2DItems/Currency/Breach/<Name>.png`.
//!
//! Like `fetch_base_icons`, this is an operator tool: the output directory is
//! a regenerable, gitignored artifact (game art belongs to GGG; we mirror the
//! community CDNs at build time rather than committing art). Soft-fails per
//! asset and always exits 0 with a manifest of what landed.
//!
//! Usage:
//!   cargo run --release -p poc2-pipeline --bin fetch-genesis-assets -- \
//!     [--out apps/web/public/genesis-icons] [--delay-ms 300]

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use clap::Parser;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

#[derive(Parser, Debug)]
#[command(name = "fetch-genesis-assets")]
struct Cli {
    /// Output directory for the icons + manifest.
    #[arg(long, default_value = "apps/web/public/genesis-icons")]
    out: PathBuf,
    /// Delay between requests in milliseconds (politeness).
    #[arg(long, default_value_t = 300)]
    delay_ms: u64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Manifest {
    version: u32,
    fetched_at: String,
    /// asset key → relative file name.
    entries: BTreeMap<String, String>,
    missing: Vec<String>,
}

/// Passive node icon classes used by the Brequel tree.
const NODE_ICONS: &[&str] = &[
    "KeepersCurrencyNode",
    "KeepersCurrencyNotable",
    "KeepersEquipmentNode",
    "KeepersEquipmentNotable",
    "KeepersUniqueNode",
    "KeepersUniqueNotable",
    "KeepersMiscellaneousNode",
    "KeepersMiscellaneousNotable",
    "MasteryBlank",
];

/// Wombgift item art (BreachFruit1=Banded, 2=Lavish, 3=Signet, 4=Revelatory,
/// 5=Ornate).
const GIFT_ART: &[&str] = &[
    "BreachFruit1",
    "BreachFruit2",
    "BreachFruit3",
    "BreachFruit4",
    "BreachFruit5",
];

/// PoE2 item-popup UI sprites (poe2db `popup2` set — the in-game tooltip
/// header caps/middles and separators). Mirrored into `ui/` for the
/// 1:1 PoE2-style tooltips in the Genesis panel.
const POPUP_SPRITES: &[(&str, &str)] = &[
    // (cdn path, local file)
    ("item/popup2/header-normal-left.webp", "header-normal-left.webp"),
    ("item/popup2/header-normal-middle.webp", "header-normal-middle.webp"),
    ("item/popup2/header-normal-right.webp", "header-normal-right.webp"),
    ("item/popup2/header-currency-left.webp", "header-currency-left.webp"),
    ("item/popup2/header-currency-middle.webp", "header-currency-middle.webp"),
    ("item/popup2/header-currency-right.webp", "header-currency-right.webp"),
    ("item/popup2/header-magic-left.webp", "header-magic-left.webp"),
    ("item/popup2/header-magic-middle.webp", "header-magic-middle.webp"),
    ("item/popup2/header-magic-right.webp", "header-magic-right.webp"),
    // Single-line rare still used as fallback; live rares prefer double-rare.
    ("item/popup2/header-rare-left.webp", "header-rare-left.webp"),
    ("item/popup2/header-rare-middle.webp", "header-rare-middle.webp"),
    ("item/popup2/header-rare-right.webp", "header-rare-right.webp"),
    (
        "item/popup2/header-double-rare-left.webp",
        "header-double-rare-left.webp",
    ),
    (
        "item/popup2/header-double-rare-middle.webp",
        "header-double-rare-middle.webp",
    ),
    (
        "item/popup2/header-double-rare-right.webp",
        "header-double-rare-right.webp",
    ),
    (
        "item/popup2/header-double-unique-left.webp",
        "header-double-unique-left.webp",
    ),
    (
        "item/popup2/header-double-unique-middle.webp",
        "header-double-unique-middle.webp",
    ),
    (
        "item/popup2/header-double-unique-right.webp",
        "header-double-unique-right.webp",
    ),
    ("item/popup2/header-gem-left.webp", "header-gem-left.webp"),
    ("item/popup2/header-gem-middle.webp", "header-gem-middle.webp"),
    ("item/popup2/header-gem-right.webp", "header-gem-right.webp"),
    // Full-width gem title strip (poe2db GemPopup doubleLine).
    (
        "art/textures/interface/2d/2dart/uiimages/ingame/smarthover/gemhovertitle.webp",
        "header-gem-title.webp",
    ),
    ("item/popup/seperator-normal.webp", "seperator-normal.webp"),
    ("item/popup/seperator-currency.webp", "seperator-currency.webp"),
    ("item/popup/seperator-magic.webp", "seperator-magic.webp"),
    ("item/popup/seperator-rare.webp", "seperator-rare.webp"),
    ("item/popup/seperator-unique.webp", "seperator-unique.webp"),
    ("item/popup/seperator-gem.webp", "seperator-gem.webp"),
    // The REAL in-game Genesis ("Brequel") tree node frames — referenced by
    // BrequelTree.json's `art` section. Small/notable × normal/can-allocate/
    // active, plus the womb inventory slot and the node glow.
    (
        "Art/2DArt/UIImages/InGame/BreachLeague/BreachTreePassiveSkillScreenPassiveFrameNormal.webp",
        "frame-small-normal.webp",
    ),
    (
        "Art/2DArt/UIImages/InGame/BreachLeague/BreachTreePassiveSkillScreenPassiveFrameCanAllocate.webp",
        "frame-small-canallocate.webp",
    ),
    (
        "Art/2DArt/UIImages/InGame/BreachLeague/BreachTreePassiveSkillScreenPassiveFrameActive.webp",
        "frame-small-active.webp",
    ),
    (
        "Art/2DArt/UIImages/InGame/BreachLeague/BreachBasicPassiveSkillScreenPassiveFrameNormal.webp",
        "frame-notable-normal.webp",
    ),
    (
        "Art/2DArt/UIImages/InGame/BreachLeague/BreachBasicPassiveSkillScreenPassiveFrameCanAllocate.webp",
        "frame-notable-canallocate.webp",
    ),
    (
        "Art/2DArt/UIImages/InGame/BreachLeague/BreachBasicPassiveSkillScreenPassiveFrameActive.webp",
        "frame-notable-active.webp",
    ),
    (
        "Art/2DArt/UIImages/InGame/BreachLeague/BreachTreeInventorySlot1x1.webp",
        "frame-womb-slot.webp",
    ),
    (
        "Art/2DArt/UIImages/InGame/BreachLeague/BreachTreeInventorySlot1x1Active.webp",
        "frame-womb-slot-active.webp",
    ),
    (
        "Art/2DArt/UIImages/InGame/GlowDesaturated.webp",
        "node-glow.webp",
    ),
];

/// GGG-hosted webfonts used by the in-game tooltips (also served by
/// pathofexile.com itself; fetched into `apps/web/public/fonts/`).
const FONTS: &[(&str, &str)] = &[
    (
        "https://web.poecdn.com/font/fontin-smallcaps-webfont.woff",
        "fontin-smallcaps-webfont.woff",
    ),
    (
        "https://web.poecdn.com/font/fontin-regular-webfont.woff",
        "fontin-regular-webfont.woff",
    ),
];

fn iso8601_now() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // Coarse ISO-8601 (UTC, no chrono dependency).
    format!("unix:{now}")
}

async fn fetch_to(
    client: &Client,
    urls: &[String],
    dest: &std::path::Path,
) -> anyhow::Result<bool> {
    for url in urls {
        match client
            .get(url)
            .header("Referer", "https://poe2db.tw/")
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => {
                let bytes = resp.bytes().await?;
                if bytes.len() < 200 {
                    warn!(%url, len = bytes.len(), "suspiciously small asset; trying next source");
                    continue;
                }
                fs::write(dest, &bytes)?;
                info!(%url, dest = %dest.display(), bytes = bytes.len(), "fetched");
                return Ok(true);
            }
            Ok(resp) => {
                warn!(%url, status = %resp.status(), "asset fetch failed; trying next source");
            }
            Err(e) => {
                warn!(%url, error = %e, "asset fetch errored; trying next source");
            }
        }
    }
    Ok(false)
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
        warn!(error = %e, "fetch-genesis-assets errored at top level");
    }
}

async fn run(cli: Cli) -> anyhow::Result<()> {
    fs::create_dir_all(&cli.out)?;
    let client = Client::builder()
        .user_agent(
            "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) \
             Chrome/127.0.0.0 Safari/537.36",
        )
        .timeout(Duration::from_secs(30))
        .build()?;

    let mut manifest = Manifest {
        version: 1,
        fetched_at: iso8601_now(),
        ..Default::default()
    };

    for key in NODE_ICONS {
        let file = format!("{key}.webp");
        let dest = cli.out.join(&file);
        let urls = vec![format!(
            "https://cdn.poe2db.tw/image/Art/2DArt/SkillIcons/passives/{key}.webp"
        )];
        if dest.exists() || fetch_to(&client, &urls, &dest).await? {
            manifest.entries.insert((*key).to_string(), file);
        } else {
            manifest.missing.push((*key).to_string());
        }
        tokio::time::sleep(Duration::from_millis(cli.delay_ms)).await;
    }

    for key in GIFT_ART {
        // Prefer webp (poe2db), fall back to RePoE PNG.
        let webp = cli.out.join(format!("{key}.webp"));
        let png = cli.out.join(format!("{key}.png"));
        if webp.exists() {
            manifest
                .entries
                .insert((*key).to_string(), format!("{key}.webp"));
            continue;
        }
        if png.exists() {
            manifest
                .entries
                .insert((*key).to_string(), format!("{key}.png"));
            continue;
        }
        let webp_urls = vec![format!(
            "https://cdn.poe2db.tw/image/Art/2DItems/Currency/Breach/{key}.webp"
        )];
        if fetch_to(&client, &webp_urls, &webp).await? {
            manifest
                .entries
                .insert((*key).to_string(), format!("{key}.webp"));
        } else {
            let png_urls = vec![format!(
                "https://repoe-fork.github.io/poe2/Art/2DItems/Currency/Breach/{key}.png"
            )];
            if fetch_to(&client, &png_urls, &png).await? {
                manifest
                    .entries
                    .insert((*key).to_string(), format!("{key}.png"));
            } else {
                manifest.missing.push((*key).to_string());
            }
        }
        tokio::time::sleep(Duration::from_millis(cli.delay_ms)).await;
    }

    // ---- PoE2 tooltip UI sprites (header caps + separators) -------------
    let ui_dir = cli.out.join("ui");
    fs::create_dir_all(&ui_dir)?;
    for (cdn_path, file) in POPUP_SPRITES {
        let dest = ui_dir.join(file);
        if dest.exists() {
            manifest
                .entries
                .insert(format!("ui/{file}"), format!("ui/{file}"));
            continue;
        }
        let urls = vec![format!("https://cdn.poe2db.tw/image/{cdn_path}")];
        if fetch_to(&client, &urls, &dest).await? {
            manifest
                .entries
                .insert(format!("ui/{file}"), format!("ui/{file}"));
        } else {
            manifest.missing.push(format!("ui/{file}"));
        }
        tokio::time::sleep(Duration::from_millis(cli.delay_ms)).await;
    }

    // ---- Fontin webfonts (in-game tooltip typography) --------------------
    let fonts_dir = cli
        .out
        .parent()
        .map(|p| p.join("fonts"))
        .unwrap_or_else(|| PathBuf::from("apps/web/public/fonts"));
    fs::create_dir_all(&fonts_dir)?;
    for (url, file) in FONTS {
        let dest = fonts_dir.join(file);
        if dest.exists() {
            manifest
                .entries
                .insert(format!("font/{file}"), format!("../fonts/{file}"));
            continue;
        }
        if fetch_to(&client, &[(*url).to_string()], &dest).await? {
            manifest
                .entries
                .insert(format!("font/{file}"), format!("../fonts/{file}"));
        } else {
            manifest.missing.push(format!("font/{file}"));
        }
        tokio::time::sleep(Duration::from_millis(cli.delay_ms)).await;
    }

    let manifest_path = cli.out.join("manifest.json");
    fs::write(&manifest_path, serde_json::to_vec_pretty(&manifest)?)?;
    info!(
        ok = manifest.entries.len(),
        missing = manifest.missing.len(),
        manifest = %manifest_path.display(),
        "genesis asset fetch complete"
    );
    Ok(())
}
