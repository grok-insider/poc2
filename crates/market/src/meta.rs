//! Meta-build aggregator + off-meta niche finder (Phase E).
//!
//! Polls poe.ninja's PoE2 builds endpoint, caches the snapshot
//! locally, and exposes an `off_meta` ranking that combines build
//! popularity with live trade prices to surface niche crafting goals.
//!
//! Per /docs/51-market-meta.md.
//!
//! ## Endpoint shape (best-effort)
//!
//! poe.ninja's PoE2 builds endpoint is not yet published in their
//! official API docs as of 2026-04. The deserializer is intentionally
//! permissive (`#[serde(default)]` on every optional field) so a
//! schema drift doesn't kill the build. The shape we expect mirrors
//! poe.ninja's PoE1 builds JSON.

use std::collections::HashMap;
use std::time::Duration;

use poc2_engine::ids::ItemClassId;
use serde::{Deserialize, Serialize};

use crate::prices::PoeScoutSnapshot;

/// Default poe.ninja PoE2 builds endpoint base.
pub const POE_NINJA_BUILDS_BASE_URL: &str = "https://poe.ninja/api/data/poe2";

/// Default league for the v1 baseline.
pub const POE_NINJA_DEFAULT_LEAGUE: &str = "Fate of the Vaal";

/// Errors a meta fetch can raise.
#[derive(Debug, thiserror::Error)]
pub enum MetaError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("JSON parse: {0}")]
    Json(#[from] serde_json::Error),
}

// ---------------------------------------------------------------------
// Snapshot types
// ---------------------------------------------------------------------

/// One build extracted from poe.ninja.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetaBuild {
    /// URL-safe slug, derived from name + ascendancy.
    pub id: String,
    pub name: String,
    pub ascendancy: String,
    /// Number of profiles using this build (popularity proxy).
    pub popularity: u32,
    /// Concept ids the build's gear emphasises (life, energy_shield,
    /// fire_damage, etc.). Drives `off_meta` ranking.
    #[serde(default)]
    pub key_concepts: Vec<String>,
    /// Item-class slots the build invests in (BodyArmour, Helmet, ...).
    #[serde(default)]
    pub base_choices: Vec<ItemClassId>,
}

/// Composite snapshot returned by [`fetch_meta_snapshot`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetaSnapshot {
    pub league: String,
    pub fetched_at: String,
    pub builds: Vec<MetaBuild>,
}

impl MetaSnapshot {
    /// Total population sample (sum of build populations).
    #[must_use]
    pub fn total_popularity(&self) -> u32 {
        self.builds.iter().map(|b| b.popularity).sum()
    }

    /// Build a `concept → cumulative popularity` map for use by the
    /// off-meta finder.
    #[must_use]
    pub fn concept_demand(&self) -> HashMap<String, u32> {
        let mut out: HashMap<String, u32> = HashMap::new();
        for b in &self.builds {
            for c in &b.key_concepts {
                *out.entry(c.clone()).or_insert(0) += b.popularity;
            }
        }
        out
    }
}

// ---------------------------------------------------------------------
// Off-meta niche finder
// ---------------------------------------------------------------------

/// One candidate craft target ranked by demand-vs-competition.
#[derive(Debug, Clone, Serialize)]
pub struct NicheTarget {
    /// Concept id this niche is tracking (e.g. `"ColdSpellSkills"`).
    pub concept: String,
    /// Cumulative popularity of builds that want this concept.
    pub demand: u32,
    /// Demand fraction in `[0, 1]`.
    pub demand_share: f64,
    /// Number of builds emphasising this concept (proxy for
    /// market competition).
    pub competition: u32,
    /// Demand-to-competition ratio. Higher = better niche.
    pub score: f64,
    /// Free-form rationale for the UI.
    pub rationale: String,
}

/// Rank niches by `demand_share / sqrt(competition + 1)` —
/// rewards high-demand low-competition concepts.
///
/// The `prices` snapshot is currently unused but threaded through
/// for forward-compat; v1.x adds a per-currency cost factor that
/// downweights niches whose typical inputs are expensive.
#[must_use]
pub fn off_meta(snapshot: &MetaSnapshot, _prices: Option<&PoeScoutSnapshot>) -> Vec<NicheTarget> {
    let total = f64::from(snapshot.total_popularity()).max(1.0);
    let demand_map = snapshot.concept_demand();
    let mut out: Vec<NicheTarget> = demand_map
        .into_iter()
        .map(|(concept, demand)| {
            let competition_count = snapshot
                .builds
                .iter()
                .filter(|b| b.key_concepts.iter().any(|c| c == &concept))
                .count();
            let competition = u32::try_from(competition_count).unwrap_or(u32::MAX);
            #[allow(clippy::cast_precision_loss)]
            let demand_share = f64::from(demand) / total;
            #[allow(clippy::cast_precision_loss)]
            let score = demand_share / f64::from(competition + 1).sqrt();
            NicheTarget {
                rationale: format!(
                    "{demand} builds want this concept; {competition} crafters listed"
                ),
                concept,
                demand,
                demand_share,
                competition,
                score,
            }
        })
        .collect();
    out.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    out
}

// ---------------------------------------------------------------------
// Fetch
// ---------------------------------------------------------------------

/// Fetch the live meta snapshot from poe.ninja.
///
/// `league` defaults to [`POE_NINJA_DEFAULT_LEAGUE`].
///
/// Returns an empty snapshot when the endpoint is unreachable
/// (poe.ninja's PoE2 surface is still beta as of 2026-04 and may
/// 404 — failing soft keeps the rest of the advisor working).
pub async fn fetch_meta_snapshot(
    client: &reqwest::Client,
    league: Option<&str>,
) -> Result<MetaSnapshot, MetaError> {
    let league_name = league.unwrap_or(POE_NINJA_DEFAULT_LEAGUE);
    let url = format!(
        "{POE_NINJA_BUILDS_BASE_URL}/builds?league={}",
        urlencoding::encode(league_name)
    );
    let response = client
        .get(&url)
        .timeout(Duration::from_secs(30))
        .send()
        .await?;
    if !response.status().is_success() {
        // poe.ninja's PoE2 endpoint may not exist yet for all leagues;
        // return an empty snapshot rather than erroring.
        return Ok(MetaSnapshot {
            league: league_name.to_string(),
            fetched_at: now_iso8601(),
            builds: Vec::new(),
        });
    }
    // Permissive parse: accept either the official builds JSON or an
    // empty array.
    let raw_text = response.text().await?;
    let builds: Vec<MetaBuild> = serde_json::from_str(&raw_text).unwrap_or_default();
    Ok(MetaSnapshot {
        league: league_name.to_string(),
        fetched_at: now_iso8601(),
        builds,
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

    fn snapshot_with_builds(builds: Vec<MetaBuild>) -> MetaSnapshot {
        MetaSnapshot {
            league: "Test".into(),
            fetched_at: "0".into(),
            builds,
        }
    }

    fn build(name: &str, popularity: u32, concepts: &[&str]) -> MetaBuild {
        MetaBuild {
            id: name.to_lowercase(),
            name: name.into(),
            ascendancy: "Tester".into(),
            popularity,
            key_concepts: concepts.iter().map(|s| (*s).to_string()).collect(),
            base_choices: vec![],
        }
    }

    #[test]
    fn concept_demand_sums_per_concept() {
        let snap = snapshot_with_builds(vec![
            build("A", 100, &["Life", "FireResistance"]),
            build("B", 50, &["FireResistance"]),
            build("C", 25, &["EnergyShield"]),
        ]);
        let demand = snap.concept_demand();
        assert_eq!(*demand.get("Life").unwrap(), 100);
        assert_eq!(*demand.get("FireResistance").unwrap(), 150);
        assert_eq!(*demand.get("EnergyShield").unwrap(), 25);
    }

    #[test]
    fn off_meta_ranks_low_competition_higher() {
        // Niche concept = "ColdSpellSkills" with 1 build at 80 pop;
        // crowded concept = "Life" with 4 builds at 80 pop combined.
        let snap = snapshot_with_builds(vec![
            build("A", 20, &["Life"]),
            build("B", 20, &["Life"]),
            build("C", 20, &["Life"]),
            build("D", 20, &["Life"]),
            build("E", 80, &["ColdSpellSkills"]),
        ]);
        let niches = off_meta(&snap, None);
        // ColdSpellSkills should beat Life because 80/sqrt(2) > 80/sqrt(5).
        let cold_idx = niches.iter().position(|n| n.concept == "ColdSpellSkills");
        let life_idx = niches.iter().position(|n| n.concept == "Life");
        assert!(cold_idx.is_some());
        assert!(life_idx.is_some());
        assert!(cold_idx < life_idx);
    }

    #[test]
    fn empty_snapshot_returns_no_niches() {
        let snap = snapshot_with_builds(vec![]);
        assert!(off_meta(&snap, None).is_empty());
    }

    #[test]
    fn total_popularity_sums_all_builds() {
        let snap =
            snapshot_with_builds(vec![build("A", 100, &["Life"]), build("B", 50, &["Mana"])]);
        assert_eq!(snap.total_popularity(), 150);
    }
}
