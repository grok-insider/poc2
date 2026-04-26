//! RePoE-fork source — JSON dumps published at `repoe-fork.github.io/poe2/`.
//!
//! See <https://github.com/repoe-fork/repoe> for the upstream.
//!
//! Files we consume:
//! - `base_items.min.json` — dictionary of base item definitions
//! - `mods.min.json` — dictionary of mod definitions
//! - `tags.min.json` — flat array of tag strings
//! - `mods_by_base.min.json` — pre-joined `base_id → [mod_id]` lookup
//! - `stat_translations/<lang>.min.json` — per-stat-id text templates (later)

//! NB: We intentionally do NOT consume `mods_by_base.min.json` from RePoE-fork.
//! Its shape is `{ item_class: { tag_set_csv: { bases: [...], mods: {...} } } }`
//! which is awkward to map onto our flat `base_id → [mod_id]` index. We
//! re-derive `mods_by_base` ourselves at normalization time by intersecting
//! base tags with each mod's `spawn_weights` (where weight > 0). This also
//! gives us better fidelity to which mods are *actually* eligible per the
//! engine's rules.

use std::collections::BTreeMap;

use poc2_data::{SourceRevision, SourceRevisions};
use reqwest::Client;
use serde::Deserialize;

use crate::error::PipelineResult;
use crate::http::{fetch_bytes, fetch_json, sha256_hex};

const BASE_URL: &str = "https://repoe-fork.github.io/poe2";

// -------------------------------------------------------------------------
// Raw RePoE-fork shapes (deserialize-only, source of truth = upstream JSON)
// -------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct RepoeBaseItem {
    #[serde(default)]
    pub domain: String,
    #[serde(default)]
    pub drop_level: u32,
    #[serde(default)]
    pub implicits: Vec<String>,
    #[serde(default)]
    pub inventory_height: u8,
    #[serde(default)]
    pub inventory_width: u8,
    #[serde(default)]
    pub item_class: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub release_state: String,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct RepoeMod {
    #[serde(default)]
    pub adds_tags: Vec<String>,
    #[serde(default)]
    pub domain: String,
    #[serde(default)]
    pub generation_type: String,
    #[serde(default)]
    pub groups: Vec<String>,
    #[serde(default)]
    pub implicit_tags: Vec<String>,
    #[serde(default)]
    pub is_essence_only: bool,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub required_level: u32,
    #[serde(default)]
    pub spawn_weights: Vec<RepoeSpawnWeight>,
    #[serde(default)]
    pub stats: Vec<RepoeStat>,
    #[serde(default)]
    pub text: String,
    #[serde(default, rename = "type")]
    pub mod_type: String,
}

#[derive(Debug, Deserialize)]
pub struct RepoeSpawnWeight {
    pub tag: String,
    pub weight: u32,
}

#[derive(Debug, Deserialize)]
pub struct RepoeStat {
    pub id: String,
    #[serde(default)]
    pub min: f64,
    #[serde(default)]
    pub max: f64,
}

#[derive(Debug, Deserialize)]
pub struct RepoeModsByBase(#[serde(default)] pub BTreeMap<String, Vec<String>>);

// -------------------------------------------------------------------------
// In-memory snapshot
// -------------------------------------------------------------------------

/// A typed snapshot of every RePoE-fork file we consume.
pub struct RepoeSnapshot {
    pub base_items: BTreeMap<String, RepoeBaseItem>,
    pub mods: BTreeMap<String, RepoeMod>,
    pub tags: Vec<String>,
    pub revisions: SourceRevisions,
}

impl RepoeSnapshot {
    pub fn count_summary(&self) -> String {
        format!(
            "RePoE: {} bases, {} mods, {} tags",
            self.base_items.len(),
            self.mods.len(),
            self.tags.len(),
        )
    }
}

// -------------------------------------------------------------------------
// Fetch
// -------------------------------------------------------------------------

pub async fn fetch(client: &Client) -> PipelineResult<RepoeSnapshot> {
    let now = current_iso8601();

    let base_url = format!("{BASE_URL}/base_items.min.json");
    let base_bytes = fetch_bytes(client, &base_url).await?;
    let base_items: BTreeMap<String, RepoeBaseItem> =
        serde_json::from_slice(&base_bytes).map_err(|e| {
            crate::error::PipelineError::JsonParse {
                url: base_url.clone(),
                source: e,
            }
        })?;
    let base_sha = sha256_hex(&base_bytes);

    let mods_url = format!("{BASE_URL}/mods.min.json");
    let mods_bytes = fetch_bytes(client, &mods_url).await?;
    let mods: BTreeMap<String, RepoeMod> = serde_json::from_slice(&mods_bytes).map_err(|e| {
        crate::error::PipelineError::JsonParse {
            url: mods_url.clone(),
            source: e,
        }
    })?;
    let mods_sha = sha256_hex(&mods_bytes);

    let tags_url = format!("{BASE_URL}/tags.min.json");
    let tags: Vec<String> = fetch_json(client, &tags_url).await?;

    let revisions = SourceRevisions(vec![
        SourceRevision {
            name: "repoe-fork.base_items".into(),
            revision: base_sha,
            url: Some(base_url),
            fetched_at: now.clone(),
        },
        SourceRevision {
            name: "repoe-fork.mods".into(),
            revision: mods_sha,
            url: Some(mods_url),
            fetched_at: now.clone(),
        },
        SourceRevision {
            name: "repoe-fork.tags".into(),
            revision: format!("count={}", tags.len()),
            url: Some(tags_url),
            fetched_at: now,
        },
    ]);

    Ok(RepoeSnapshot {
        base_items,
        mods,
        tags,
        revisions,
    })
}

fn current_iso8601() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    // Cheap re-implementation; good enough for provenance metadata.
    format!("{secs}")
}
