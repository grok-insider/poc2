//! Craft of Exile source — `poec_data.json`.
//!
//! Public dataset published at
//! `https://www.craftofexile.com/json/poe2/main/poec_data.json` (~2.3 MB).
//! The file is a JS variable assignment of the form `poecd=<json>`; we
//! strip the prefix before parsing.
//!
//! ## Sections we consume
//!
//! - `bgroups` — base-group definitions (Body Armours, Helmets, ...)
//! - `bases` — concrete base items (`Wyrmscale Coat`, etc.)
//! - `modifiers` — mod definitions with `id_modifier`, `affix`,
//!   `name_modifier`, `modgroups`, `mtypes`, `hybrid` flag.
//! - `tiers` — `{mod_id: {base_id: [{ilvl, weighting, nvalues, tord}]}}`
//!   weight + tier table (THE source of crafting probability data)
//! - `essences` — 81 essences with `name_essence`, tooltip, tiers map
//!   keyed by base_id
//! - `catalysts` — 12 catalyst types with name + tag pipe-string
//! - `socketables` — runes / soul cores / talismans with id + type + name
//!
//! ## Mod-id mapping
//!
//! CoE's `id_modifier` is an opaque integer (e.g. `"6286"`), independent
//! of RePoE-fork's `ModId` strings (e.g. `"LocalIncreasedEnergyShield2"`).
//! The normalizer joins them by **case-insensitive substring match of
//! `name_modifier`** against RePoE-fork mod names — imperfect but
//! tractable. Mismatches degrade gracefully: weights without a known
//! engine `ModId` are dropped from the bundle's `weights` section.

use std::collections::BTreeMap;

use poc2_data::{SourceRevision, SourceRevisions};
use reqwest::Client;
use serde::Deserialize;

use crate::error::{PipelineError, PipelineResult};
use crate::http::{fetch_bytes, sha256_hex};

const COE_DATA_URL: &str = "https://www.craftofexile.com/json/poe2/main/poec_data.json";
const POECD_PREFIX: &str = "poecd=";

// -------------------------------------------------------------------------
// Raw JSON shapes
// -------------------------------------------------------------------------

/// Top-level CoE document.
#[derive(Debug, Deserialize)]
pub struct CoeData {
    pub bitems: Section<CoeBaseItem>,
    pub bases: Section<CoeBase>,
    pub bgroups: Section<CoeBaseGroup>,
    pub modifiers: Section<CoeModifier>,
    pub mgroups: Section<CoeModGroup>,
    pub mtypes: Section<CoeModType>,
    pub catalysts: Section<CoeCatalyst>,
    pub essences: Section<CoeEssence>,
    /// `{base_id_string -> [mod_id_string]}`
    #[serde(default)]
    pub basemods: BTreeMap<String, Vec<String>>,
    /// `{mod_id_string -> [base_id_string]}`
    #[serde(default)]
    pub modbases: BTreeMap<String, Vec<String>>,
    /// `{mod_id_string -> {base_id_string -> [TierEntry]}}`
    #[serde(default, deserialize_with = "deserialize_tiers")]
    pub tiers: TiersByMod,
}

/// Type alias for the nested mod→base→tier table.
pub type TiersByMod = BTreeMap<String, BTreeMap<String, Vec<CoeTierEntry>>>;

#[derive(Debug, Deserialize)]
pub struct Section<T> {
    pub seq: Vec<T>,
}

#[derive(Debug, Deserialize)]
pub struct CoeBaseItem {
    pub id_bitem: String,
    pub id_base: String,
    pub name_bitem: String,
    #[serde(default)]
    pub drop_level: Option<String>,
    #[serde(default)]
    pub imgurl: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CoeBase {
    pub id_bgroup: String,
    pub id_base: String,
    pub name_base: String,
    #[serde(default)]
    pub is_jewellery: Option<String>,
    #[serde(default)]
    pub base_type: Option<String>,
    #[serde(default)]
    pub is_legacy: Option<String>,
    #[serde(default)]
    pub is_martial: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CoeBaseGroup {
    pub id_bgroup: String,
    pub name_bgroup: String,
    pub max_affix: String,
    pub is_rare: String,
    pub is_craftable: String,
    pub max_sockets: String,
}

#[derive(Debug, Deserialize)]
pub struct CoeModifier {
    pub id_modifier: String,
    /// JSON-encoded array of group strings, e.g. `["Life"]`.
    pub modgroups: Option<String>,
    /// `"prefix"` / `"suffix"` / `"corrupted"` / `"implicit"` / `"enchant"`.
    pub affix: String,
    pub name_modifier: String,
    /// Pipe-delimited mtype ids, e.g. `"|28|1|"`.
    #[serde(default)]
    pub mtypes: Option<String>,
    pub hybrid: String,
}

#[derive(Debug, Deserialize)]
pub struct CoeModGroup {
    pub id_mgroup: String,
    pub name_mgroup: String,
}

#[derive(Debug, Deserialize)]
pub struct CoeModType {
    pub id_mtype: String,
    pub poedb_id: Option<String>,
    pub name_mtype: String,
}

#[derive(Debug, Deserialize)]
pub struct CoeCatalyst {
    pub id_catalyst: String,
    pub name_catalyst: String,
    /// Pipe-delimited tags, e.g. `"|life|"`.
    pub tags: String,
}

#[derive(Debug, Deserialize)]
pub struct CoeEssence {
    pub id_essence: String,
    pub name_essence: String,
    /// JSON-encoded array of tooltip lines (one per item-class clause).
    pub tooltip: String,
    /// JSON-encoded `{base_id_string: [[{mod, id, ilvl}]]}`.
    pub tiers: String,
    /// `"0"` for normal essences; `"1"` for corrupted essences.
    pub corrupt: String,
}

#[derive(Debug, Deserialize)]
pub struct CoeTierEntry {
    pub ilvl: String,
    pub weighting: String,
    /// JSON-encoded values matrix. Optional.
    pub nvalues: Option<String>,
    /// Tier order index (0 = T1).
    #[serde(default)]
    pub tord: i32,
    pub alias: Option<String>,
}

fn deserialize_tiers<'de, D>(deserializer: D) -> Result<TiersByMod, D::Error>
where
    D: serde::Deserializer<'de>,
{
    BTreeMap::deserialize(deserializer)
}

// -------------------------------------------------------------------------
// Snapshot
// -------------------------------------------------------------------------

/// Typed in-memory snapshot of the CoE data feed.
pub struct CoeSnapshot {
    pub data: CoeData,
    pub revisions: SourceRevisions,
}

impl CoeSnapshot {
    pub fn count_summary(&self) -> String {
        format!(
            "CoE: {} mods, {} bases, {} bgroups, {} essences, {} catalysts, {} tier entries",
            self.data.modifiers.seq.len(),
            self.data.bases.seq.len(),
            self.data.bgroups.seq.len(),
            self.data.essences.seq.len(),
            self.data.catalysts.seq.len(),
            self.data.tiers.values().map(BTreeMap::len).sum::<usize>(),
        )
    }
}

// -------------------------------------------------------------------------
// Fetch
// -------------------------------------------------------------------------

/// Fetch and parse the CoE poec_data.json (~2.3 MB).
pub async fn fetch(client: &Client) -> PipelineResult<CoeSnapshot> {
    let bytes = fetch_bytes(client, COE_DATA_URL).await?;
    let sha = sha256_hex(&bytes);

    let raw = std::str::from_utf8(&bytes).map_err(|e| PipelineError::Other {
        message: format!("CoE data is not UTF-8: {e}"),
    })?;
    let json_part = raw.strip_prefix(POECD_PREFIX).unwrap_or(raw).trim();

    let data: CoeData = serde_json::from_str(json_part).map_err(|e| PipelineError::JsonParse {
        url: COE_DATA_URL.into(),
        source: e,
    })?;

    let revisions = SourceRevisions(vec![SourceRevision {
        name: "craftofexile.poec_data".into(),
        revision: sha,
        url: Some(COE_DATA_URL.into()),
        fetched_at: now_iso8601(),
    }]);

    Ok(CoeSnapshot { data, revisions })
}

fn now_iso8601() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    format!("{secs}")
}

/// Parse the JSON-encoded `tooltip` array embedded inside a CoeEssence.
pub fn parse_essence_tooltip(s: &str) -> Vec<String> {
    serde_json::from_str(s).unwrap_or_default()
}

/// Parse the JSON-encoded `tiers` map embedded inside a CoeEssence.
///
/// The shape is `{base_id: [[{mod, id, ilvl}]]}` where the outer array
/// is per-tier and the inner array is per-mod-in-that-tier (usually 1).
pub fn parse_essence_tiers(s: &str) -> BTreeMap<String, Vec<Vec<EssenceTierMod>>> {
    serde_json::from_str(s).unwrap_or_default()
}

/// One mod entry inside an essence's tier list.
#[derive(Debug, Clone, Deserialize)]
pub struct EssenceTierMod {
    pub r#mod: String,
    pub id: String,
    pub ilvl: String,
}

/// Helper: split CoE's pipe-delimited tag string into Vec<String>.
/// `"|life|jewellery_attribute|"` → `["life", "jewellery_attribute"]`.
pub fn split_pipes(s: &str) -> Vec<String> {
    s.split('|')
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_pipes_strips_empty_tokens() {
        assert_eq!(
            split_pipes("|life|attribute|"),
            vec!["life".to_string(), "attribute".to_string()]
        );
        assert_eq!(split_pipes("").len(), 0);
        assert_eq!(split_pipes("|").len(), 0);
    }

    #[test]
    fn parse_essence_tooltip_handles_empty() {
        assert_eq!(parse_essence_tooltip("[]").len(), 0);
        assert_eq!(parse_essence_tooltip("invalid").len(), 0);
    }

    #[test]
    fn parse_essence_tooltip_handles_real_payload() {
        let s = r#"["One Handed Melee Weapon: Adds # to # Physical","Two Handed Melee Weapon: Adds # to #"]"#;
        let v = parse_essence_tooltip(s);
        assert_eq!(v.len(), 2);
        assert!(v[0].contains("Physical"));
    }

    #[test]
    fn parse_essence_tiers_real_shape() {
        let s = r#"{"13":[[{"mod":"5118","id":"LocalAddedPhysicalDamage5","ilvl":"46"}]]}"#;
        let parsed = parse_essence_tiers(s);
        assert_eq!(parsed.len(), 1);
        let tiers = parsed.get("13").unwrap();
        assert_eq!(tiers.len(), 1);
        assert_eq!(tiers[0][0].id, "LocalAddedPhysicalDamage5");
    }
}
