//! Upstream change detection — the trigger half of the automated data-refresh
//! loop (ADR-0012).
//!
//! The bundle's primary source is the RePoE-fork JSON published at
//! `repoe-fork.github.io/poe2/`. That feed is *unversioned*: each build silently
//! tracks whatever upstream last published. To know **when** a new game patch's
//! data has actually landed (so we can rebuild + diff + open a PR), this module:
//!
//! 1. Reads the community PoE2 patch-version pointer
//!    (`poe-tool-dev/latest-patch-version`) — the canonical "what version is live"
//!    signal shared across the tooling ecosystem.
//! 2. Hashes the three RePoE-fork files the pipeline consumes (`base_items`,
//!    `mods`, `tags`) — the *content* signal, independent of GGG's version string.
//! 3. Compares both against a committed state file
//!    (`pipeline/data/upstream_state.json`) recording what we last ingested.
//!
//! Either signal changing means "upstream moved"; the workflow then rebuilds and
//! diffs. We track both because they answer different questions: the patch
//! pointer flips the moment GGG deploys (even before RePoE-fork catches up), and
//! the SHAs flip when RePoE-fork actually republishes (which is what changes our
//! bundle). Recording both makes the PR description honest about *which* moved.

use std::path::Path;

use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::error::PipelineResult;
use crate::http::{fetch_bytes, fetch_text, sha256_hex};

/// Pointer(s) to the current live PoE2 game version, tried in order.
///
/// The **primary** is RePoE-fork's own `poe2/version.txt`, published right
/// alongside the data files we consume — so it's the GGG game version the data
/// we ingest was *generated from* (e.g. `4.5.4.1.2`). That tight correlation is
/// exactly what we want: when this string moves, the data we're about to rebuild
/// moved with it. (The poe-tool-dev `latest-patch-version` repo is PoE1-only —
/// its `latest.txt` is the `3.28.x` line — so it is intentionally **not** used.)
///
/// If the pointer is unreachable we fall back to the RePoE-fork content SHAs
/// alone, which are a sufficient change signal on their own.
pub const POE2_PATCH_POINTER_URLS: &[&str] = &["https://repoe-fork.github.io/poe2/version.txt"];

const REPOE_BASE_URL: &str = "https://repoe-fork.github.io/poe2";

/// The three RePoE-fork files the pipeline actually consumes (see
/// [`crate::sources::repoe`]). Hashing exactly these — and no more — keeps the
/// content signal aligned with what would change our bundle.
pub const REPOE_FILES: &[(&str, &str)] = &[
    ("base_items", "base_items.min.json"),
    ("mods", "mods.min.json"),
    ("tags", "tags.min.json"),
];

/// A point-in-time fingerprint of every upstream source we track.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpstreamState {
    /// Live PoE2 patch version string from the community pointer, if reachable
    /// (e.g. `"4.5.0.1"`). `None` when the pointer was unreachable.
    #[serde(default)]
    pub poe2_patch: Option<String>,
    /// SHA-256 (hex) of each consumed RePoE-fork file, keyed by short name
    /// (`base_items` / `mods` / `tags`).
    #[serde(default)]
    pub repoe_shas: std::collections::BTreeMap<String, String>,
    /// ISO-8601-ish timestamp of when this state was last written.
    #[serde(default)]
    pub last_checked: String,
}

impl UpstreamState {
    /// Load the committed state file. A missing file is treated as an empty
    /// state (first-ever run), not an error.
    pub fn load(path: &Path) -> PipelineResult<Self> {
        match std::fs::read_to_string(path) {
            Ok(s) => {
                let state: UpstreamState = serde_json::from_str(&s).map_err(|e| {
                    crate::error::PipelineError::JsonParse {
                        url: path.display().to_string(),
                        source: e,
                    }
                })?;
                Ok(state)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(UpstreamState::default()),
            Err(e) => Err(e.into()),
        }
    }

    /// Write the state file as pretty JSON (with a trailing newline so it's a
    /// well-behaved committed text file).
    pub fn save(&self, path: &Path) -> PipelineResult<()> {
        let mut json = serde_json::to_string_pretty(self).expect("UpstreamState serializes");
        json.push('\n');
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, json)?;
        Ok(())
    }
}

/// What changed between the previously-recorded state and what we just fetched.
#[derive(Debug, Clone, Serialize)]
pub struct WatchReport {
    /// `true` if any tracked signal moved (patch pointer OR any RePoE SHA).
    pub changed: bool,
    /// `true` specifically if the live patch-version string changed.
    pub patch_changed: bool,
    /// Short names of the RePoE files whose content hash changed.
    pub repoe_files_changed: Vec<String>,
    /// The previous patch string (may be `None` on first run).
    pub previous_patch: Option<String>,
    /// The freshly-observed patch string (may be `None` if the pointer 404s).
    pub current_patch: Option<String>,
    /// The freshly-observed state — what `save()` should persist on a refresh.
    pub current_state: UpstreamState,
}

impl WatchReport {
    /// Human-readable one-paragraph summary for logs / PR bodies.
    pub fn summary(&self) -> String {
        if !self.changed {
            return "No upstream change detected (patch pointer and RePoE-fork \
                    content hashes unchanged)."
                .to_string();
        }
        let mut parts = Vec::new();
        if self.patch_changed {
            parts.push(format!(
                "PoE2 patch {} → {}",
                self.previous_patch.as_deref().unwrap_or("(none)"),
                self.current_patch.as_deref().unwrap_or("(unknown)"),
            ));
        }
        if !self.repoe_files_changed.is_empty() {
            parts.push(format!(
                "RePoE-fork files updated: {}",
                self.repoe_files_changed.join(", ")
            ));
        }
        format!("Upstream change detected — {}.", parts.join("; "))
    }
}

/// Best-effort fetch of the live PoE2 patch version. Tries each known pointer
/// URL in order; returns `None` (not an error) if all are unreachable, since the
/// content SHAs are an independent, sufficient change signal.
pub async fn fetch_live_patch(client: &Client) -> Option<String> {
    for url in POE2_PATCH_POINTER_URLS {
        match fetch_text(client, url).await {
            Ok(v) if !v.is_empty() => return Some(v),
            Ok(_) => {}
            Err(e) => {
                tracing::debug!(url, error = %e, "patch pointer unreachable; trying next");
            }
        }
    }
    None
}

/// Hash every consumed RePoE-fork file. A fetch error on any file is
/// propagated — RePoE-fork is the mandatory primary source, so if it's
/// unreachable we want the watch run to fail loudly rather than silently
/// report "no change".
pub async fn fetch_repoe_shas(
    client: &Client,
) -> PipelineResult<std::collections::BTreeMap<String, String>> {
    let mut shas = std::collections::BTreeMap::new();
    for (name, file) in REPOE_FILES {
        let url = format!("{REPOE_BASE_URL}/{file}");
        let bytes = fetch_bytes(client, &url).await?;
        shas.insert((*name).to_string(), sha256_hex(&bytes));
    }
    Ok(shas)
}

/// Run a full detection pass against the previously-recorded `previous` state.
pub async fn check(client: &Client, previous: &UpstreamState) -> PipelineResult<WatchReport> {
    let current_patch = fetch_live_patch(client).await;
    let repoe_shas = fetch_repoe_shas(client).await?;

    let patch_changed = match (&previous.poe2_patch, &current_patch) {
        // Only treat a *known* difference as a change; a newly-unreachable
        // pointer (Some → None) is not a content change.
        (prev, Some(cur)) => prev.as_deref() != Some(cur.as_str()),
        (_, None) => false,
    };

    let mut repoe_files_changed = Vec::new();
    for (name, sha) in &repoe_shas {
        if previous.repoe_shas.get(name) != Some(sha) {
            repoe_files_changed.push(name.clone());
        }
    }
    repoe_files_changed.sort();

    let changed = patch_changed || !repoe_files_changed.is_empty();

    let current_state = UpstreamState {
        poe2_patch: current_patch
            .clone()
            .or_else(|| previous.poe2_patch.clone()),
        repoe_shas,
        last_checked: now_iso8601(),
    };

    Ok(WatchReport {
        changed,
        patch_changed,
        repoe_files_changed,
        previous_patch: previous.poe2_patch.clone(),
        current_patch,
        current_state,
    })
}

fn now_iso8601() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    // Epoch seconds is enough for provenance; avoids pulling a date crate.
    format!("epoch:{secs}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shas(pairs: &[(&str, &str)]) -> std::collections::BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect()
    }

    #[test]
    fn empty_state_round_trips_through_json() {
        let s = UpstreamState::default();
        let json = serde_json::to_string(&s).unwrap();
        let back: UpstreamState = serde_json::from_str(&json).unwrap();
        assert_eq!(s, back);
    }

    #[test]
    fn missing_state_file_is_empty_not_error() {
        let tmp = std::env::temp_dir().join("poc2-watch-does-not-exist-xyz.json");
        let _ = std::fs::remove_file(&tmp);
        let s = UpstreamState::load(&tmp).unwrap();
        assert_eq!(s, UpstreamState::default());
    }

    #[test]
    fn save_then_load_is_identity() {
        let tmp = std::env::temp_dir().join(format!("poc2-watch-{}.json", std::process::id()));
        let mut s = UpstreamState {
            poe2_patch: Some("4.5.0.1".into()),
            repoe_shas: shas(&[("mods", "deadbeef"), ("tags", "cafef00d")]),
            last_checked: "epoch:1".into(),
        };
        s.save(&tmp).unwrap();
        let back = UpstreamState::load(&tmp).unwrap();
        assert_eq!(s, back);
        s.poe2_patch = Some("changed".into()); // ensure we compared a real clone
        let _ = std::fs::remove_file(&tmp);
    }

    fn report_for(
        prev: &UpstreamState,
        cur_patch: Option<&str>,
        cur_shas: &[(&str, &str)],
    ) -> WatchReport {
        // Mirror `check`'s pure comparison logic without the network.
        let current_patch = cur_patch.map(str::to_string);
        let repoe_shas = shas(cur_shas);
        let patch_changed = match (&prev.poe2_patch, &current_patch) {
            (p, Some(c)) => p.as_deref() != Some(c.as_str()),
            (_, None) => false,
        };
        let mut repoe_files_changed: Vec<String> = repoe_shas
            .iter()
            .filter(|(name, sha)| prev.repoe_shas.get(*name) != Some(*sha))
            .map(|(name, _)| name.clone())
            .collect();
        repoe_files_changed.sort();
        let changed = patch_changed || !repoe_files_changed.is_empty();
        WatchReport {
            changed,
            patch_changed,
            repoe_files_changed,
            previous_patch: prev.poe2_patch.clone(),
            current_patch,
            current_state: UpstreamState {
                poe2_patch: current_patch_or(prev, cur_patch),
                repoe_shas,
                last_checked: "epoch:test".into(),
            },
        }
    }

    fn current_patch_or(prev: &UpstreamState, cur: Option<&str>) -> Option<String> {
        cur.map(str::to_string).or_else(|| prev.poe2_patch.clone())
    }

    #[test]
    fn identical_state_reports_no_change() {
        let prev = UpstreamState {
            poe2_patch: Some("4.5.0.1".into()),
            repoe_shas: shas(&[("mods", "aaa"), ("tags", "bbb")]),
            last_checked: "epoch:1".into(),
        };
        let r = report_for(&prev, Some("4.5.0.1"), &[("mods", "aaa"), ("tags", "bbb")]);
        assert!(!r.changed);
        assert!(!r.patch_changed);
        assert!(r.repoe_files_changed.is_empty());
    }

    #[test]
    fn changed_mods_sha_is_detected() {
        let prev = UpstreamState {
            poe2_patch: Some("4.5.0.1".into()),
            repoe_shas: shas(&[("mods", "aaa"), ("tags", "bbb")]),
            last_checked: "epoch:1".into(),
        };
        let r = report_for(&prev, Some("4.5.0.1"), &[("mods", "ZZZ"), ("tags", "bbb")]);
        assert!(r.changed);
        assert!(!r.patch_changed);
        assert_eq!(r.repoe_files_changed, vec!["mods".to_string()]);
    }

    #[test]
    fn new_patch_string_is_detected_even_without_sha_change() {
        let prev = UpstreamState {
            poe2_patch: Some("4.5.0.1".into()),
            repoe_shas: shas(&[("mods", "aaa")]),
            last_checked: "epoch:1".into(),
        };
        let r = report_for(&prev, Some("4.6.0.0"), &[("mods", "aaa")]);
        assert!(r.changed);
        assert!(r.patch_changed);
        assert!(r.repoe_files_changed.is_empty());
    }

    #[test]
    fn unreachable_pointer_does_not_count_as_patch_change() {
        let prev = UpstreamState {
            poe2_patch: Some("4.5.0.1".into()),
            repoe_shas: shas(&[("mods", "aaa")]),
            last_checked: "epoch:1".into(),
        };
        let r = report_for(&prev, None, &[("mods", "aaa")]);
        assert!(!r.changed);
        assert!(!r.patch_changed);
        // The previously-known patch is preserved, not nulled out.
        assert_eq!(r.current_state.poe2_patch.as_deref(), Some("4.5.0.1"));
    }

    #[test]
    fn first_run_with_no_previous_state_reports_change() {
        let prev = UpstreamState::default();
        let r = report_for(&prev, Some("4.5.0.1"), &[("mods", "aaa")]);
        assert!(r.changed);
        assert!(r.patch_changed);
        assert_eq!(r.repoe_files_changed, vec!["mods".to_string()]);
    }
}
