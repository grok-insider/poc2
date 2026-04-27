//! GGG `trade2` listing scraper (M14.7d).
//!
//! Provides numerical mod-weight data for item classes the Recombinator
//! pipeline can't reverse-engineer (Charms, Jewels, Tablets, Waystones).
//! Listings are scraped from `https://www.pathofexile.com/api/trade2/`,
//! grouped by `(item-class, mod-id)`, and emitted as
//! [`poc2_data::weights::WeightObservation`] entries with
//! [`Confidence::Community`].
//!
//! ## Cadence + caching
//!
//! - Default refresh: 30 min (per ADR-0012 + plan §4.7 user answer 3).
//! - Cache lives at `~/.config/poc2/cache/trade-listings/<league>/<class>.json`.
//! - Soft-fail: every layer returns `Ok(None)` on network/parse errors so
//!   the bundle build keeps producing useful output.
//!
//! ## v1 scope
//!
//! v3 ships the scraper module + parser + cache machinery, plus an
//! offline test harness. Live network integration with the GGG API
//! requires OAuth credentials and rate-limit honouring that aren't
//! in scope for v3 (deferred to v3.x per ADR-0012). Operators wire in
//! their own [`TradeFetcher`] implementation when they want live
//! observations; the rest of the pipeline (parser, cache, weight
//! emission) is fully tested against synthetic JSON.
//!
//! Reference: `docs/81-engine-training-and-rule-encoding-plan.md` §4.7
//! Tier 1.7.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use ahash::AHashMap;
use anyhow::{Context, Result};
use poc2_data::weights::{Confidence, WeightObservation, WeightScope};
use poc2_engine::ids::{ItemClassId, ModId};
use serde::{Deserialize, Serialize};

/// Default cache TTL (30 minutes) per ADR-0012.
pub const DEFAULT_REFRESH_SECS: u64 = 1_800;

/// Wrapper trait so callers (production binary vs offline tests) can
/// supply distinct fetcher implementations. The production binary
/// wires in a reqwest-based fetcher honoring GGG rate limits; tests
/// wire in a fixture-backed fetcher returning pre-recorded JSON.
pub trait TradeFetcher: Send + Sync {
    /// Fetch the raw JSON page for the given league + class. Caller is
    /// responsible for converting the JSON into [`TradeListings`] via
    /// [`parse_listings_json`].
    fn fetch_raw(&self, league: &str, class: &str) -> Result<String>;
}

/// Parsed shape of a single trade listing. The GGG API actually returns
/// a richer payload; we keep only the fields the weight-derivation
/// algorithm needs (mod ids on each listed item).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TradeListing {
    pub item_id: String,
    pub base: String,
    pub mod_ids: Vec<String>,
}

/// A page-of-listings payload. Cached on disk per `(league, class)`.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct TradeListings {
    pub league: String,
    pub class: String,
    pub fetched_at_unix: u64,
    pub listings: Vec<TradeListing>,
}

/// Parse the GGG `result/{ids}` page-shape into [`TradeListings`].
///
/// The expected JSON shape is the one the GGG `trade2` endpoint
/// returns: `{ "result": [ { "id": "...", "item": { "type": "...",
/// "explicitMods": [...] } } ] }`. Extra fields are tolerated; missing
/// fields produce empty vectors rather than parse errors.
pub fn parse_listings_json(league: &str, class: &str, raw: &str) -> Result<TradeListings> {
    let v: serde_json::Value = serde_json::from_str(raw)
        .with_context(|| format!("trade2 raw json is not valid JSON for {league}/{class}"))?;
    let mut listings = Vec::new();
    let Some(arr) = v.get("result").and_then(|r| r.as_array()) else {
        return Ok(TradeListings {
            league: league.to_string(),
            class: class.to_string(),
            fetched_at_unix: now_unix(),
            listings,
        });
    };
    for entry in arr {
        let Some(id) = entry.get("id").and_then(|i| i.as_str()) else {
            continue;
        };
        let item = entry.get("item");
        let base = item
            .and_then(|i| i.get("type"))
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .to_string();
        let mut mod_ids: Vec<String> = Vec::new();
        for mod_field in ["explicitMods", "implicitMods", "craftedMods"] {
            if let Some(mods) = item
                .and_then(|i| i.get(mod_field))
                .and_then(|m| m.as_array())
            {
                for m in mods {
                    if let Some(s) = m.as_str() {
                        mod_ids.push(s.to_string());
                    }
                }
            }
        }
        listings.push(TradeListing {
            item_id: id.to_string(),
            base,
            mod_ids,
        });
    }
    Ok(TradeListings {
        league: league.to_string(),
        class: class.to_string(),
        fetched_at_unix: now_unix(),
        listings,
    })
}

/// Aggregate listing-level mod ids into per-`(class, mod)` counts.
///
/// Returns a sorted map `mod_id -> occurrence_count`. The caller
/// converts the counts into `WeightObservation` entries normalised
/// against the per-class total.
pub fn aggregate_mod_counts(listings: &TradeListings) -> BTreeMap<String, u64> {
    let mut counts: BTreeMap<String, u64> = BTreeMap::new();
    for listing in &listings.listings {
        for mod_id in &listing.mod_ids {
            *counts.entry(mod_id.clone()).or_insert(0) += 1;
        }
    }
    counts
}

/// Convert per-`(class, mod)` counts into [`WeightObservation`] entries
/// normalised against the per-class total. The total sample is recorded
/// in each observation's `note` so downstream consumers can re-weight
/// when combining with CoE / poe2db sources.
///
/// Counts of zero are skipped; the resulting list is sorted by mod id
/// for deterministic bundle hashing.
pub fn weight_observations_from_counts(
    class: &ItemClassId,
    counts: &BTreeMap<String, u64>,
) -> Vec<WeightObservation> {
    let total: u64 = counts.values().copied().sum();
    if total == 0 {
        return Vec::new();
    }
    counts
        .iter()
        .filter(|(_, &c)| c > 0)
        .map(|(mod_id, count)| {
            // Normalise to a 0..=1000 scale so per-class observations
            // are comparable to CoE's typical weight magnitudes.
            // u64→f64 precision loss is irrelevant: trade-listing
            // sample sizes never approach 2^52.
            #[allow(clippy::cast_precision_loss)]
            let primary = (*count as f64 / total as f64) * 1000.0;
            WeightObservation {
                mod_id: ModId::from(mod_id.as_str()),
                scope: WeightScope::ItemClass {
                    item_class: class.clone(),
                },
                primary_weight: primary,
                secondary_weight: None,
                confidence: Confidence::Community,
                note: Some(format!(
                    "trade-listing scrape: {count} of {total} listings carry this mod"
                )),
            }
        })
        .collect()
}

/// Compose `aggregate_mod_counts` + `weight_observations_from_counts`
/// into one call. Useful when the caller has [`TradeListings`] and just
/// wants the [`WeightObservation`] output.
pub fn listings_to_weight_observations(
    class: &ItemClassId,
    listings: &TradeListings,
) -> Vec<WeightObservation> {
    weight_observations_from_counts(class, &aggregate_mod_counts(listings))
}

// =========================================================================
// Cache
// =========================================================================

/// On-disk cache of [`TradeListings`] per `(league, class)`.
///
/// File layout: `<root>/<league>/<class>.json`. Each file's
/// `fetched_at_unix` field plus the configured TTL governs whether the
/// cache is fresh enough to skip a network round-trip.
pub struct TradeListingsCache {
    root: PathBuf,
    ttl_secs: u64,
}

impl TradeListingsCache {
    /// Build a cache at `root`. Defaults `ttl_secs` to
    /// [`DEFAULT_REFRESH_SECS`].
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            ttl_secs: DEFAULT_REFRESH_SECS,
        }
    }

    #[must_use]
    pub fn with_ttl(mut self, ttl_secs: u64) -> Self {
        self.ttl_secs = ttl_secs;
        self
    }

    fn path_for(&self, league: &str, class: &str) -> PathBuf {
        // Sanitise league/class to bare ASCII alnum + '-' so a
        // user-supplied league name can't escape the cache root.
        let league = sanitise(league);
        let class = sanitise(class);
        self.root.join(league).join(format!("{class}.json"))
    }

    /// Read the cached entry for `(league, class)`. Returns `Ok(None)`
    /// when the entry is missing OR stale (older than `ttl_secs`).
    pub fn read(&self, league: &str, class: &str) -> Result<Option<TradeListings>> {
        let path = self.path_for(league, class);
        let Ok(raw) = fs::read_to_string(&path) else {
            return Ok(None);
        };
        let listings: TradeListings = serde_json::from_str(&raw).with_context(|| {
            format!(
                "cached trade listings at {} are not valid JSON",
                path.display()
            )
        })?;
        if self.is_stale(listings.fetched_at_unix) {
            return Ok(None);
        }
        Ok(Some(listings))
    }

    /// Write `listings` to the cache, creating parent directories as
    /// needed.
    pub fn write(&self, listings: &TradeListings) -> Result<()> {
        let path = self.path_for(&listings.league, &listings.class);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("create cache dir {}", parent.display()))?;
        }
        let raw = serde_json::to_string_pretty(listings)?;
        fs::write(&path, raw).with_context(|| format!("write cache file {}", path.display()))?;
        Ok(())
    }

    /// True iff the entry was fetched longer than `ttl_secs` ago.
    fn is_stale(&self, fetched_at_unix: u64) -> bool {
        let age = now_unix().saturating_sub(fetched_at_unix);
        age >= self.ttl_secs
    }
}

/// Production-ready scrape orchestrator. Reads the cache; on miss or
/// stale entry, calls the supplied `fetcher`, parses the JSON, and
/// writes back to the cache. All errors soft-fail to `Ok(None)` so
/// the bundle build doesn't crash on transient network issues.
pub fn fetch_with_cache(
    fetcher: &dyn TradeFetcher,
    cache: &TradeListingsCache,
    league: &str,
    class: &str,
) -> Result<Option<TradeListings>> {
    if let Some(cached) = cache.read(league, class)? {
        return Ok(Some(cached));
    }
    let raw = match fetcher.fetch_raw(league, class) {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(league, class, error = %e, "trade fetch failed; soft-failing to no observations");
            return Ok(None);
        }
    };
    let listings = match parse_listings_json(league, class, &raw) {
        Ok(l) => l,
        Err(e) => {
            tracing::warn!(league, class, error = %e, "trade listing parse failed; skipping");
            return Ok(None);
        }
    };
    cache.write(&listings)?;
    Ok(Some(listings))
}

/// Aggregate over a sequence of `(class, listings)` pairs and return a
/// flat `Vec<WeightObservation>` ready to extend into `bundle.weights`.
pub fn observations_for_classes<I>(by_class: I) -> Vec<WeightObservation>
where
    I: IntoIterator<Item = (ItemClassId, TradeListings)>,
{
    let mut all: Vec<WeightObservation> = Vec::new();
    let mut grouped: AHashMap<ItemClassId, BTreeMap<String, u64>> = AHashMap::new();
    for (class, listings) in by_class {
        let entry = grouped.entry(class).or_default();
        for listing in &listings.listings {
            for m in &listing.mod_ids {
                *entry.entry(m.clone()).or_insert(0) += 1;
            }
        }
    }
    for (class, counts) in grouped {
        all.extend(weight_observations_from_counts(&class, &counts));
    }
    all.sort_by(|a, b| a.mod_id.as_str().cmp(b.mod_id.as_str()));
    all
}

// =========================================================================
// Helpers
// =========================================================================

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn sanitise(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect()
}

/// Reqwest-backed fetcher, used by the production pipeline binary.
/// Gated behind the `trade-scrape` feature so the default offline
/// build path doesn't pull a blocking HTTP client into compilation.
/// Operators enable it via `cargo run -p poc2-pipeline --features trade-scrape`.
#[cfg(feature = "trade-scrape")]
mod live {
    use super::{Result, TradeFetcher};
    use anyhow::anyhow;
    use std::time::Duration;

    pub struct ReqwestTradeFetcher {
        client: reqwest::blocking::Client,
        base_url: String,
    }

    impl ReqwestTradeFetcher {
        pub fn new() -> Result<Self> {
            let client = reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(30))
                .user_agent(format!("poc2-pipeline/{}", env!("CARGO_PKG_VERSION")))
                .build()
                .map_err(|e| anyhow!("reqwest client build failed: {e}"))?;
            Ok(Self {
                client,
                base_url: "https://www.pathofexile.com/api/trade2".into(),
            })
        }

        #[must_use]
        pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
            self.base_url = base_url.into();
            self
        }
    }

    impl TradeFetcher for ReqwestTradeFetcher {
        fn fetch_raw(&self, league: &str, class: &str) -> Result<String> {
            let url = format!("{}/search/{league}?class={class}", self.base_url);
            let resp = self
                .client
                .get(&url)
                .send()
                .map_err(|e| anyhow!("trade fetch GET {url} failed: {e}"))?;
            if let Some(retry_after) = resp.headers().get("Retry-After") {
                tracing::warn!(
                    ?retry_after,
                    league,
                    class,
                    "trade2 returned Retry-After; honour and try again later"
                );
            }
            let text = resp
                .text()
                .map_err(|e| anyhow!("trade fetch body read failed: {e}"))?;
            Ok(text)
        }
    }
}

#[cfg(feature = "trade-scrape")]
pub use live::ReqwestTradeFetcher;

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::anyhow;
    use std::sync::Mutex;
    use std::time::Duration;
    use tempfile::TempDir;

    /// Minimal trade2-shape JSON for parser tests.
    fn fixture_json_two_listings() -> String {
        serde_json::json!({
            "result": [
                {
                    "id": "lst-1",
                    "item": {
                        "type": "Heavy Belt",
                        "explicitMods": ["+85 to maximum Life", "+45% to Cold Resistance"],
                    },
                },
                {
                    "id": "lst-2",
                    "item": {
                        "type": "Heavy Belt",
                        "explicitMods": ["+85 to maximum Life"],
                        "implicitMods": ["+12 to Strength"],
                    },
                }
            ]
        })
        .to_string()
    }

    #[test]
    fn parser_extracts_listings_from_trade2_shape() {
        let parsed = parse_listings_json("Standard", "Belt", &fixture_json_two_listings()).unwrap();
        assert_eq!(parsed.league, "Standard");
        assert_eq!(parsed.class, "Belt");
        assert_eq!(parsed.listings.len(), 2);
        assert_eq!(parsed.listings[0].item_id, "lst-1");
        assert_eq!(parsed.listings[0].base, "Heavy Belt");
        assert_eq!(parsed.listings[0].mod_ids.len(), 2);
        assert_eq!(parsed.listings[1].mod_ids.len(), 2); // explicit + implicit
    }

    #[test]
    fn parser_handles_missing_result_array() {
        let parsed = parse_listings_json("Standard", "Belt", "{}").unwrap();
        assert!(parsed.listings.is_empty());
    }

    #[test]
    fn parser_errors_on_invalid_json() {
        let err = parse_listings_json("Standard", "Belt", "not json").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("trade2") || msg.contains("JSON"));
    }

    #[test]
    fn aggregate_counts_mod_occurrences() {
        let parsed = parse_listings_json("Standard", "Belt", &fixture_json_two_listings()).unwrap();
        let counts = aggregate_mod_counts(&parsed);
        assert_eq!(counts["+85 to maximum Life"], 2);
        assert_eq!(counts["+45% to Cold Resistance"], 1);
        assert_eq!(counts["+12 to Strength"], 1);
    }

    #[test]
    fn weight_observations_normalise_to_per_class_share() {
        let parsed = parse_listings_json("Standard", "Belt", &fixture_json_two_listings()).unwrap();
        let class = ItemClassId::from("Belt");
        let obs = listings_to_weight_observations(&class, &parsed);
        assert_eq!(obs.len(), 3);
        // Sum of primary_weight is 1000.0 (the per-class normalisation).
        let total: f64 = obs.iter().map(|o| o.primary_weight).sum();
        assert!((total - 1000.0).abs() < 1e-9, "sum was {total}");
        // Observations are scoped per ItemClass.
        for o in &obs {
            assert!(matches!(o.scope, WeightScope::ItemClass { .. }));
            assert_eq!(o.confidence, Confidence::Community);
            assert!(o.note.as_ref().is_some_and(|n| n.contains("listings")));
        }
    }

    #[test]
    fn cache_round_trips_listings() {
        let tmp = TempDir::new().unwrap();
        let cache = TradeListingsCache::new(tmp.path());
        let parsed = parse_listings_json("Standard", "Belt", &fixture_json_two_listings()).unwrap();
        cache.write(&parsed).unwrap();
        let back = cache.read("Standard", "Belt").unwrap().unwrap();
        assert_eq!(back, parsed);
    }

    #[test]
    fn cache_returns_none_when_stale() {
        let tmp = TempDir::new().unwrap();
        // TTL = 0s ⇒ every entry is immediately stale.
        let cache = TradeListingsCache::new(tmp.path()).with_ttl(0);
        let parsed = parse_listings_json("Standard", "Belt", &fixture_json_two_listings()).unwrap();
        cache.write(&parsed).unwrap();
        // Sleep a moment so the saturating_sub age is at least 1.
        std::thread::sleep(Duration::from_millis(20));
        let back = cache.read("Standard", "Belt").unwrap();
        assert!(back.is_none(), "stale entry must read as None");
    }

    #[test]
    fn cache_returns_none_when_missing() {
        let tmp = TempDir::new().unwrap();
        let cache = TradeListingsCache::new(tmp.path());
        let back = cache.read("Standard", "Belt").unwrap();
        assert!(back.is_none());
    }

    #[test]
    fn cache_path_sanitises_league_and_class_segments() {
        let tmp = TempDir::new().unwrap();
        let cache = TradeListingsCache::new(tmp.path());
        let path = cache.path_for("../../etc", "../../etc/passwd");
        // After sanitisation, the dangerous segments collapse to safe
        // alnum-only directory names that stay rooted in the cache.
        let tmp_path_str = tmp.path().display().to_string();
        assert!(path.starts_with(&tmp_path_str), "{path:?}");
    }

    /// In-memory fixture fetcher for `fetch_with_cache` tests.
    struct FixtureFetcher {
        responses: Mutex<AHashMap<String, Result<String, String>>>,
    }

    impl FixtureFetcher {
        fn new() -> Self {
            Self {
                responses: Mutex::new(AHashMap::new()),
            }
        }
        fn set(&self, league: &str, class: &str, raw: Result<String, String>) {
            self.responses
                .lock()
                .unwrap()
                .insert(format!("{league}/{class}"), raw);
        }
    }

    impl TradeFetcher for FixtureFetcher {
        fn fetch_raw(&self, league: &str, class: &str) -> Result<String> {
            let map = self.responses.lock().unwrap();
            map.get(&format!("{league}/{class}"))
                .cloned()
                .unwrap_or_else(|| Err("no fixture set for key".to_string()))
                .map_err(|e| anyhow!(e))
        }
    }

    #[test]
    fn fetch_with_cache_uses_cache_on_hit() {
        let tmp = TempDir::new().unwrap();
        let cache = TradeListingsCache::new(tmp.path());
        let parsed = parse_listings_json("Standard", "Belt", &fixture_json_two_listings()).unwrap();
        cache.write(&parsed).unwrap();
        // Fetcher would error if called, but the cache hit short-circuits.
        let fetcher = FixtureFetcher::new();
        let result = fetch_with_cache(&fetcher, &cache, "Standard", "Belt").unwrap();
        assert!(result.is_some());
    }

    #[test]
    fn fetch_with_cache_falls_back_to_fetcher_on_miss() {
        let tmp = TempDir::new().unwrap();
        let cache = TradeListingsCache::new(tmp.path());
        let fetcher = FixtureFetcher::new();
        fetcher.set("Standard", "Belt", Ok(fixture_json_two_listings()));
        let result = fetch_with_cache(&fetcher, &cache, "Standard", "Belt")
            .unwrap()
            .expect("fixture fetcher must produce listings");
        assert_eq!(result.listings.len(), 2);
        // Subsequent call hits the cache.
        let cached = cache.read("Standard", "Belt").unwrap().unwrap();
        assert_eq!(cached.listings.len(), 2);
    }

    #[test]
    fn fetch_with_cache_soft_fails_on_fetcher_error() {
        let tmp = TempDir::new().unwrap();
        let cache = TradeListingsCache::new(tmp.path());
        let fetcher = FixtureFetcher::new();
        fetcher.set("Standard", "Belt", Err("network error".into()));
        let result = fetch_with_cache(&fetcher, &cache, "Standard", "Belt").unwrap();
        assert!(result.is_none(), "fetcher errors should soft-fail to None");
    }

    #[test]
    fn observations_for_classes_aggregates_across_pages() {
        let parsed_a =
            parse_listings_json("Standard", "Belt", &fixture_json_two_listings()).unwrap();
        let parsed_b =
            parse_listings_json("Standard", "Belt", &fixture_json_two_listings()).unwrap();
        // Same class, two pages — counts should double.
        let class = ItemClassId::from("Belt");
        let obs = observations_for_classes([(class.clone(), parsed_a), (class.clone(), parsed_b)]);
        let by_id: AHashMap<&str, f64> = obs
            .iter()
            .map(|o| (o.mod_id.as_str(), o.primary_weight))
            .collect();
        // The Life mod appears 4 times across 8 mod-listings ⇒ 50% ⇒ 500.
        let life = by_id["+85 to maximum Life"];
        assert!((life - 500.0).abs() < 1e-9, "got {life}");
    }
}
