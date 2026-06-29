//! On-disk price cache for poe2scout snapshots.
//!
//! ## Why a cache exists
//!
//! `crate::prices::fetch_snapshot` walks ~5 paginated category endpoints
//! per refresh and depends on poe2scout being reachable. When the user
//! launches the desktop app offline (or poe2scout is briefly down) we
//! still want the advisor to score with *something* — yesterday's prices
//! are vastly better than no prices, since the engine's risk model
//! tolerates noisy cost estimates.
//!
//! Phase F of `docs/80-crafter-helper-v2-plan.md` (§6.F.1) specifies the
//! cache lives under `~/.config/poc2/cache/prices/<league>.json` with a
//! default TTL of 1 hour. Past that the cache is stale and a refresh is
//! required; the file remains as the offline fallback.
//!
//! ## Disk layout
//!
//! ```text
//! ~/.config/poc2/cache/prices/
//!   ├── Fate of the Vaal.json
//!   ├── Standard.json
//!   └── ...
//! ```
//!
//! Each JSON file is the serialized `PoeScoutSnapshot` plus a
//! `cached_at` ISO-8601 timestamp. Schema is forward-compatible — new
//! fields default-deserialize to their zero value.

use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::prices::PoeScoutSnapshot;

/// Default TTL — 1 hour. Past this point the cache is stale and a
/// refresh is preferred, but the cache is still kept for offline reads.
pub const DEFAULT_TTL: Duration = Duration::from_secs(60 * 60);

/// Cached snapshot wrapper — same data poe2scout returned plus a
/// timestamp so the consumer can compare against `DEFAULT_TTL`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedSnapshot {
    /// Original snapshot payload.
    pub snapshot: PoeScoutSnapshot,
    /// Unix timestamp (seconds) at which the cache entry was written.
    pub cached_at_unix: u64,
}

impl CachedSnapshot {
    /// Wrap a freshly fetched snapshot with the current timestamp.
    pub fn now(snapshot: PoeScoutSnapshot) -> Self {
        let cached_at_unix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        Self {
            snapshot,
            cached_at_unix,
        }
    }

    /// True iff the cache entry is older than `ttl`.
    pub fn is_stale(&self, ttl: Duration) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        now.saturating_sub(self.cached_at_unix) > ttl.as_secs()
    }

    /// Age of the cache entry in seconds (saturating at zero for
    /// future-dated entries — clock skew safety).
    pub fn age_secs(&self) -> u64 {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        now.saturating_sub(self.cached_at_unix)
    }
}

/// Errors that can surface while reading or writing the cache. None of
/// these escape the `try_load`/`store` callers as hard failures — the
/// caller decides whether a missing cache file is a soft miss or a real
/// error.
#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("serde: {0}")]
    Serde(#[from] serde_json::Error),
    #[error(
        "none of XDG_CONFIG_HOME, HOME, APPDATA, USERPROFILE set — cannot resolve cache directory"
    )]
    NoConfigHome,
}

/// Resolve the directory the price cache lives in — typically
/// `~/.config/poc2/cache/prices`. Honors `XDG_CONFIG_HOME` when set;
/// falls back to `$HOME/.config`, then on Windows `%APPDATA%` (already a
/// per-user config root, so `poc2` nests directly under it) and finally
/// `%USERPROFILE%\.config` (mirrors the unix layout for msys-style shells
/// without `APPDATA`). Unix behavior is unchanged: `HOME` is checked
/// before either Windows variable.
pub fn default_cache_dir() -> Result<PathBuf, CacheError> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .or_else(|| std::env::var_os("APPDATA").map(PathBuf::from))
        .or_else(|| std::env::var_os("USERPROFILE").map(|h| PathBuf::from(h).join(".config")))
        .ok_or(CacheError::NoConfigHome)?;
    Ok(base.join("poc2").join("cache").join("prices"))
}

/// Path to the cache file for a given league name. Sanitises the league
/// string so user-supplied input can't traverse outside the cache dir
/// (slashes and parent references are replaced with underscores).
pub fn cache_file_for_league(dir: &std::path::Path, league: &str) -> PathBuf {
    let safe: String = league
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '\0' | '\n' | '\r' => '_',
            c => c,
        })
        .collect();
    let safe = safe.trim_matches(|c: char| c == '.' || c.is_whitespace());
    dir.join(format!("{}.json", if safe.is_empty() { "_" } else { safe }))
}

/// Try to load a previously cached snapshot for `league`. Returns `Ok(None)`
/// when the file doesn't exist (cache miss, no error), `Ok(Some(_))` when
/// it parses, or `Err(_)` when the file exists but is unreadable / corrupt.
///
/// Missing parent directories return `Ok(None)` (we treat them as misses).
pub fn try_load(dir: &std::path::Path, league: &str) -> Result<Option<CachedSnapshot>, CacheError> {
    let path = cache_file_for_league(dir, league);
    if !path.exists() {
        return Ok(None);
    }
    let bytes = std::fs::read(&path)?;
    let cached: CachedSnapshot = serde_json::from_slice(&bytes)?;
    Ok(Some(cached))
}

/// Persist `cached` to the per-league file. Creates the cache directory
/// when missing. Atomic via a temp-file-rename so concurrent loaders
/// never see a half-written file.
pub fn store(
    dir: &std::path::Path,
    league: &str,
    cached: &CachedSnapshot,
) -> Result<(), CacheError> {
    std::fs::create_dir_all(dir)?;
    let path = cache_file_for_league(dir, league);
    let tmp = path.with_extension("json.tmp");
    let serialized = serde_json::to_vec_pretty(cached)?;
    std::fs::write(&tmp, &serialized)?;
    std::fs::rename(&tmp, &path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use tempfile::TempDir;

    fn fake_snapshot() -> PoeScoutSnapshot {
        PoeScoutSnapshot {
            league: "Test League".into(),
            divine_price_in_exalts: 100.0,
            chaos_per_divine: 30.0,
            entries: HashMap::new(),
            fetched_at: "2026-04-26T00:00:00Z".into(),
        }
    }

    #[test]
    fn round_trip_stores_and_loads() {
        let dir = TempDir::new().unwrap();
        let league = "Fate of the Vaal";
        let cached = CachedSnapshot::now(fake_snapshot());
        store(dir.path(), league, &cached).unwrap();
        let loaded = try_load(dir.path(), league).unwrap().unwrap();
        assert_eq!(loaded.snapshot.league, "Test League");
        assert_eq!(loaded.cached_at_unix, cached.cached_at_unix);
    }

    #[test]
    fn missing_file_returns_none() {
        let dir = TempDir::new().unwrap();
        assert!(try_load(dir.path(), "absent").unwrap().is_none());
    }

    #[test]
    fn league_name_with_slashes_is_sanitised() {
        let dir = TempDir::new().unwrap();
        let path = cache_file_for_league(dir.path(), "evil/../league");
        assert!(path
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("evil_"));
    }

    #[test]
    fn league_name_empty_falls_back_to_underscore_filename() {
        let dir = TempDir::new().unwrap();
        let path = cache_file_for_league(dir.path(), "");
        assert_eq!(path.file_name().unwrap().to_str().unwrap(), "_.json");
    }

    #[test]
    fn fresh_entry_is_not_stale() {
        let cached = CachedSnapshot::now(fake_snapshot());
        assert!(!cached.is_stale(DEFAULT_TTL));
    }

    #[test]
    fn ancient_entry_is_stale() {
        let mut cached = CachedSnapshot::now(fake_snapshot());
        cached.cached_at_unix = 0; // 1970
        assert!(cached.is_stale(DEFAULT_TTL));
        assert!(cached.age_secs() > DEFAULT_TTL.as_secs());
    }

    /// Snapshots a set of env vars and restores them on drop (panic-safe),
    /// so env mutation can't leak into other tests in the process.
    struct EnvGuard {
        saved: Vec<(&'static str, Option<std::ffi::OsString>)>,
    }

    impl EnvGuard {
        fn capture(keys: &[&'static str]) -> Self {
            let saved = keys.iter().map(|k| (*k, std::env::var_os(k))).collect();
            Self { saved }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            for (key, value) in &self.saved {
                match value {
                    Some(v) => std::env::set_var(key, v),
                    None => std::env::remove_var(key),
                }
            }
        }
    }

    /// All fallback branches in one test: env mutation must stay
    /// sequential because the test harness shares process env across
    /// threads, and no other test in this crate reads these vars.
    #[test]
    fn default_cache_dir_fallback_chain() {
        const KEYS: [&str; 4] = ["XDG_CONFIG_HOME", "HOME", "APPDATA", "USERPROFILE"];
        let _guard = EnvGuard::capture(&KEYS);
        let clear = || {
            for key in KEYS {
                std::env::remove_var(key);
            }
        };

        clear();
        std::env::set_var("XDG_CONFIG_HOME", "/xdg");
        std::env::set_var("HOME", "/home/u");
        let dir = default_cache_dir().unwrap();
        assert_eq!(dir, PathBuf::from("/xdg/poc2/cache/prices"));

        clear();
        std::env::set_var("HOME", "/home/u");
        std::env::set_var("APPDATA", "/appdata");
        let dir = default_cache_dir().unwrap();
        assert_eq!(dir, PathBuf::from("/home/u/.config/poc2/cache/prices"));

        clear();
        std::env::set_var("APPDATA", "/appdata");
        std::env::set_var("USERPROFILE", "/profile");
        let dir = default_cache_dir().unwrap();
        assert_eq!(dir, PathBuf::from("/appdata/poc2/cache/prices"));

        clear();
        std::env::set_var("USERPROFILE", "/profile");
        let dir = default_cache_dir().unwrap();
        assert_eq!(dir, PathBuf::from("/profile/.config/poc2/cache/prices"));

        clear();
        let err = default_cache_dir().unwrap_err();
        assert!(matches!(err, CacheError::NoConfigHome));
    }

    #[test]
    fn corrupt_file_surfaces_error() {
        let dir = TempDir::new().unwrap();
        let path = cache_file_for_league(dir.path(), "corrupt");
        std::fs::write(&path, b"not json").unwrap();
        let err = try_load(dir.path(), "corrupt").unwrap_err();
        assert!(matches!(err, CacheError::Serde(_)));
    }
}
