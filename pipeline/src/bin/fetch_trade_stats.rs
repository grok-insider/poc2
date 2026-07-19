//! Mod-text → trade-API stat-id table generator.
//!
//! The web price-check feature needs to turn an item's mod lines into
//! official trade-site stat filters. That mapping is maintained first-party
//! nowhere in this repo, so this script derives it from the vendored
//! Exiled-Exchange-2 dataParser output (MIT licensed):
//! `example-repos/Exiled-Exchange-2/dataParser/output/en/stats.ndjson`.
//!
//! Default mode reads the vendored NDJSON, flattens each entry's
//! `trade.ids` buckets into `ids` (dropping entries that carry no trade
//! ids), and writes `apps/web/public/trade-stats.json` — a regenerable,
//! gitignored artifact, like the base-icon/genesis-icon manifests.
//!
//! `--live` additionally GETs `https://www.pathofexile.com/api/trade2/data/stats`
//! and merges it on top: official ids win, vendored matchers fill the gaps,
//! and official-only refs land with a text-derived matcher. On any fetch
//! failure it warns and falls back to vendored ids only. The script must
//! not fail (`exit 0` always).

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use clap::Parser;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

#[derive(Parser, Debug)]
#[command(
    about = "Build apps/web/public/trade-stats.json from the vendored EE2 stats NDJSON.",
    long_about = "Reads the vendored Exiled-Exchange-2 dataParser stats NDJSON, flattens \
                  each entry's trade.ids into the web price-check contract, and writes \
                  apps/web/public/trade-stats.json. With --live, the official \
                  /api/trade2/data/stats endpoint is merged on top (official ids win)."
)]
struct Cli {
    /// Path to the vendored EE2 stats NDJSON.
    #[arg(
        long,
        default_value = "example-repos/Exiled-Exchange-2/dataParser/output/en/stats.ndjson"
    )]
    source: PathBuf,
    /// Output path. Defaults to `apps/web/public/trade-stats.json` relative to CWD.
    #[arg(long, default_value = "apps/web/public/trade-stats.json")]
    out: PathBuf,
    /// Fetch the official trade API stat list and merge it (official ids win,
    /// vendored matchers fill gaps). Falls back to vendored-only on failure.
    #[arg(long, default_value_t = false)]
    live: bool,
}

const TRADE_STATS_URL: &str = "https://www.pathofexile.com/api/trade2/data/stats";
const SOURCE_NOTE: &str =
    "Exiled-Exchange-2 dataParser (MIT) + pathofexile.com /api/trade2/data/stats";

/// One human-text matcher. The web contract is exactly
/// `string`/`value`/`negate`; other vendored matcher fields (`advanced`,
/// `oils`, …) are dropped on deserialization.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Matcher {
    string: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    negate: Option<bool>,
}

/// One vendored NDJSON line. The internal EE2 `id` (and `dp`,
/// `fromAreaMods`, `trade.option`, `trade.inverted`, …) are not part of the
/// web contract and are ignored.
#[derive(Debug, Deserialize)]
struct VendoredStat {
    #[serde(rename = "ref")]
    ref_text: String,
    better: i64,
    matchers: Vec<Matcher>,
    #[serde(default)]
    trade: Option<VendoredTrade>,
}

#[derive(Debug, Deserialize)]
struct VendoredTrade {
    // The logbook-faction entries carry an explicit `"ids": null`.
    #[serde(default)]
    ids: Option<BTreeMap<String, Vec<String>>>,
}

/// One output stat: `trade.ids` flattened to `ids`, buckets kept verbatim
/// (explicit/implicit/fractured/enchant/rune/pseudo/desecrated/sanctum/skill).
#[derive(Debug, Serialize)]
struct StatEntry {
    #[serde(rename = "ref")]
    ref_text: String,
    better: i64,
    matchers: Vec<Matcher>,
    ids: BTreeMap<String, Vec<String>>,
}

/// The on-disk contract consumed by the web price-check lane.
#[derive(Debug, Serialize)]
struct TradeStatsDoc {
    version: u32,
    source: &'static str,
    generated: String,
    stats: Vec<StatEntry>,
}

// Official `/api/trade2/data/stats` shape:
// `{"result":[{"id":"explicit","label":"Explicit","entries":[{"id":"explicit.stat_123","text":"#% increased ...","type":"explicit"}]}]}`.
#[derive(Debug, Deserialize)]
struct OfficialStats {
    result: Vec<OfficialGroup>,
}

#[derive(Debug, Deserialize)]
struct OfficialGroup {
    entries: Vec<OfficialEntry>,
}

#[derive(Debug, Deserialize)]
struct OfficialEntry {
    id: String,
    text: String,
    #[serde(rename = "type")]
    kind: String,
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
        warn!(error = %e, "fetch-trade-stats errored at top level; no table written");
    }
}

async fn run(cli: Cli) -> anyhow::Result<()> {
    info!(source = %cli.source.display(), "reading vendored stats NDJSON");
    let ndjson = fs::read_to_string(&cli.source)?;
    let (mut stats, dropped) = transform_ndjson(&ndjson);
    info!(stats = stats.len(), dropped, "vendored NDJSON transformed");

    if cli.live {
        let client = Client::builder()
            .user_agent(
                "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) \
                 Chrome/127.0.0.0 Safari/537.36",
            )
            .timeout(Duration::from_secs(30))
            .build()?;
        match fetch_official(&client).await {
            Ok(official) => {
                let (replaced, added) = merge_official(&mut stats, official);
                info!(
                    replaced,
                    added, "official trade stats merged (official ids win)"
                );
            }
            Err(e) => {
                warn!(error = %e, "live fetch failed; falling back to vendored ids only");
            }
        }
    }

    let doc = TradeStatsDoc {
        version: 1,
        source: SOURCE_NOTE,
        generated: unix_now(),
        stats,
    };
    if let Some(parent) = cli.out.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&cli.out, serde_json::to_vec(&doc)?)?;
    info!(
        stats = doc.stats.len(),
        out = %cli.out.display(),
        "trade-stats table written"
    );
    Ok(())
}

/// NDJSON → output stats. Entries without trade ids (EE2-internal stats the
/// trade site can't filter on) are dropped; the dropped count is returned
/// alongside. Matcher fields and bucket names are kept verbatim.
fn transform_ndjson(ndjson: &str) -> (Vec<StatEntry>, usize) {
    let mut stats = Vec::new();
    let mut dropped = 0usize;
    for line in ndjson.lines().filter(|l| !l.trim().is_empty()) {
        let entry: VendoredStat = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(e) => {
                warn!(error = %e, "skipping malformed NDJSON line");
                dropped += 1;
                continue;
            }
        };
        let mut ids = entry.trade.and_then(|t| t.ids).unwrap_or_default();
        ids.retain(|_, v| !v.is_empty());
        if ids.is_empty() {
            dropped += 1;
            continue;
        }
        stats.push(StatEntry {
            ref_text: entry.ref_text,
            better: entry.better,
            matchers: entry.matchers,
            ids,
        });
    }
    (stats, dropped)
}

/// Merge the official stat list on top of the vendored table. Where a ref
/// exists in both, the official ids replace the vendored ones (the vendored
/// matchers stay); official-only refs are appended with a text-derived
/// matcher. Returns (replaced, added).
fn merge_official(stats: &mut Vec<StatEntry>, official: OfficialStats) -> (usize, usize) {
    // Group official entries by display text bucketed by stat type. Mirrors
    // EE2's valueless path (`stats_combined_df`): value-variant ids
    // (`...|N`) belong to its value converter, not this table.
    let mut by_ref: BTreeMap<String, BTreeMap<String, Vec<String>>> = BTreeMap::new();
    for entry in official.result.into_iter().flat_map(|g| g.entries) {
        if entry.id.contains('|') {
            continue;
        }
        let bucket = by_ref
            .entry(official_ref(&entry.text))
            .or_default()
            .entry(entry.kind)
            .or_default();
        if !bucket.contains(&entry.id) {
            bucket.push(entry.id);
        }
    }

    let mut replaced = 0usize;
    for stat in stats.iter_mut() {
        if let Some(ids) = by_ref.remove(&stat.ref_text) {
            stat.ids = ids;
            replaced += 1;
        }
    }

    let added = by_ref.len();
    for (ref_text, ids) in by_ref {
        stats.push(StatEntry {
            ref_text: ref_text.clone(),
            better: 1,
            matchers: vec![Matcher {
                string: ref_text,
                value: None,
                negate: None,
            }],
            ids,
        });
    }
    stats.sort_by(|a, b| a.ref_text.cmp(&b.ref_text));
    (replaced, added)
}

/// Official text → vendored `ref`. Mirrors EE2's normalisation
/// (`text.replace("+#%", "#%")` then `re.sub(r" \(.*\)", "", ref)` —
/// greedy, so the cut runs from the first ` (` to the last `)`).
fn official_ref(text: &str) -> String {
    let text = text.replace("+#%", "#%");
    match (text.find(" ("), text.rfind(')')) {
        (Some(start), Some(end)) if end > start => {
            format!("{}{}", &text[..start], &text[end + 1..])
        }
        _ => text,
    }
}

async fn fetch_official(client: &Client) -> anyhow::Result<OfficialStats> {
    for attempt in 0..3u32 {
        match client.get(TRADE_STATS_URL).send().await {
            Ok(resp) if resp.status().is_success() => {
                return Ok(resp.json::<OfficialStats>().await?);
            }
            Ok(resp) => {
                warn!(status = %resp.status(), attempt, "trade stats fetch non-200");
            }
            Err(e) => warn!(error = %e, attempt, "trade stats fetch error"),
        }
        tokio::time::sleep(Duration::from_millis(800 * u64::from(attempt + 1))).await;
    }
    anyhow::bail!("failed after retries: {TRADE_STATS_URL}")
}

fn unix_now() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("unix:{secs}")
}

#[cfg(test)]
mod tests {
    use super::*;

    // Four representative vendored lines: a multi-bucket stat with a valued
    // matcher, an EE2-internal stat with no trade key, a negated-matcher
    // stat, and a logbook-faction-style stat with an explicit null ids.
    const FIXTURE: &str = r##"{"ref": "# Charm Slot", "better": 1, "matchers": [{"string": "# Charm Slots"}, {"string": "# Charm Slot", "value": 1}], "trade": {"ids": {"explicit": ["explicit.stat_2582079000"], "rune": ["rune.stat_554899692"]}}, "id": "num_charm_slots"}
{"ref": "Internal Only", "better": 1, "matchers": [{"string": "Internal Only"}], "id": "ee2_internal"}
{"ref": "#% reduced Attribute Requirements", "better": 1, "matchers": [{"string": "#% increased Attribute Requirements", "negate": true}, {"string": "#% reduced Attribute Requirements"}], "trade": {"ids": {"explicit": ["explicit.stat_3639275092"], "desecrated": ["desecrated.stat_3639275092"]}}, "id": "local_attribute_requirements_+%"}
{"ref": "Has Logbook Faction: Knights of the Sun", "better": -1, "matchers": [{"string": "Has Logbook Faction: Knights of the Sun"}], "trade": {"ids": null}}"##;

    #[test]
    fn drops_entries_without_trade_ids() {
        let (stats, dropped) = transform_ndjson(FIXTURE);
        // "Internal Only" (no trade key) and the faction entry (null ids) drop.
        assert_eq!(dropped, 2);
        let refs: Vec<&str> = stats.iter().map(|s| s.ref_text.as_str()).collect();
        assert_eq!(refs, ["# Charm Slot", "#% reduced Attribute Requirements"]);

        // Buckets that exist but are empty count as no ids too.
        let empty_bucket = r##"{"ref": "Empty Bucket", "better": 1, "matchers": [{"string": "Empty Bucket"}], "trade": {"ids": {"explicit": []}}}"##;
        let (stats, dropped) = transform_ndjson(empty_bucket);
        assert!(stats.is_empty());
        assert_eq!(dropped, 1);
    }

    #[test]
    fn preserves_value_and_negate_matchers() {
        let (stats, _) = transform_ndjson(FIXTURE);
        assert_eq!(stats[0].matchers[1].value, Some(1));
        assert_eq!(stats[1].matchers[0].negate, Some(true));
        // Absent matcher fields stay absent in the serialized contract.
        let plain = serde_json::to_string(&stats[0].matchers[0]).unwrap();
        assert_eq!(plain, r##"{"string":"# Charm Slots"}"##);
        let valued = serde_json::to_string(&stats[0].matchers[1]).unwrap();
        assert_eq!(valued, r##"{"string":"# Charm Slot","value":1}"##);
    }

    #[test]
    fn flattens_trade_id_buckets() {
        let (stats, _) = transform_ndjson(FIXTURE);
        assert_eq!(stats[0].ids["explicit"], ["explicit.stat_2582079000"]);
        assert_eq!(stats[0].ids["rune"], ["rune.stat_554899692"]);
        assert_eq!(stats[0].better, 1);
        // The flattened entry serializes with `ids`, not `trade.ids`, and
        // without the internal EE2 `id`.
        let json = serde_json::to_string(&stats[0]).unwrap();
        assert!(json.contains(r#""ids":{"explicit":"#));
        assert!(!json.contains(r#""trade""#));
        assert!(!json.contains("num_charm_slots"));
    }

    #[test]
    fn merge_prefers_official_ids_and_fills_gaps() {
        let (mut stats, _) = transform_ndjson(FIXTURE);
        let official: OfficialStats = serde_json::from_value(serde_json::json!({
            "result": [{
                "id": "explicit",
                "label": "Explicit",
                "entries": [
                    // Replaces the vendored explicit id; the `+#%` and
                    // ` (...)` normalisations mirror EE2.
                    {"id": "explicit.stat_999", "text": "+#% reduced Attribute Requirements (Local)", "type": "explicit"},
                    {"id": "explicit.stat_777", "text": "# to Brand-New Stat", "type": "explicit"},
                    // Value variants are the value converter's job — skipped.
                    {"id": "explicit.stat_777|5", "text": "# to Brand-New Stat", "type": "explicit"}
                ]
            }]
        }))
        .unwrap();
        let (replaced, added) = merge_official(&mut stats, official);
        assert_eq!((replaced, added), (1, 1));

        let attr = stats
            .iter()
            .find(|s| s.ref_text == "#% reduced Attribute Requirements")
            .unwrap();
        assert_eq!(attr.ids["explicit"], ["explicit.stat_999"]);
        assert!(!attr.ids.contains_key("desecrated")); // official ids win outright
        assert_eq!(attr.matchers.len(), 2); // vendored matchers kept

        let new = stats
            .iter()
            .find(|s| s.ref_text == "# to Brand-New Stat")
            .unwrap();
        assert_eq!(new.ids["explicit"], ["explicit.stat_777"]);
        assert_eq!(new.matchers[0].string, "# to Brand-New Stat");
    }
}
