//! Live price feeder for [poe2scout.com].
//!
//! Polls the public REST API exposed at `https://poe2scout.com/api`
//! (OpenAPI / Swagger at `/api/swagger`). The flow is:
//!
//! 1. Fetch `/api/<Realm>/Leagues` to discover the active league plus
//!    the canonical exchange rates (DivinePrice in exalts,
//!    ChaosDivinePrice in chaos per divine).
//! 2. Fetch each crafting-relevant category
//!    (`currency`, `essences`, `ritual` (omens), `abyss` (bones),
//!    `breach` (catalysts)) via
//!    `/api/<Realm>/Leagues/<League>/Currencies/ByCategory` and walk
//!    paginated results.
//! 3. Convert each [`PoeScoutCurrencyEntry`] price (in `BaseCurrency` =
//!    Exalted Orb) to divine-equivalent [`DivEquiv`] using the league's
//!    DivinePrice. The resulting feed is merged into a [`Valuator`] via
//!    [`apply_feed_to_valuator`].
//!
//! Only the HTTP fetching ([`fetch_snapshot`]) is gated behind the `net`
//! feature; the snapshot types, [`apply_feed_to_valuator`] and
//! [`default_id_mapping`] build everywhere (including the WASM advisor,
//! which receives the snapshot JSON from the browser).
//!
//! [poe2scout.com]: https://poe2scout.com/

use std::collections::HashMap;
#[cfg(feature = "net")]
use std::time::Duration;

use poc2_engine::ids::CurrencyId;
use serde::{Deserialize, Serialize};

use crate::valuator::{DivEquiv, Valuator};

/// Default base URL for the poe2scout REST API.
pub const POE2SCOUT_BASE_URL: &str = "https://poe2scout.com/api";

/// Default Realm identifier — the only one we care about for v1.
pub const POE2SCOUT_REALM: &str = "poe2";

/// Default league for patch 0.4 (Fate of the Vaal).
pub const POE2SCOUT_DEFAULT_LEAGUE: &str = "Fate of the Vaal";

/// Categories the advisor consults at startup. We deliberately skip
/// non-crafting categories (`fragments`, `runes`, `incursion`, etc.) so
/// the polling cost stays bounded.
pub const POE2SCOUT_DEFAULT_CATEGORIES: &[&str] =
    &["currency", "essences", "ritual", "abyss", "breach"];

/// One league's metadata as returned by `/api/<Realm>/Leagues`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PoeScoutLeague {
    pub value: String,
    /// Exalts per divine.
    pub divine_price: f64,
    /// Chaos per divine.
    pub chaos_divine_price: f64,
    pub base_currency_api_id: String,
    pub base_currency_text: String,
}

/// One currency-category response page.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PoeScoutCategoryResponse {
    pub current_page: u32,
    pub pages: u32,
    pub total: u32,
    pub items: Vec<PoeScoutCurrencyEntry>,
}

/// One currency item returned by ByCategory.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PoeScoutCurrencyEntry {
    pub currency_item_id: u64,
    pub item_id: u64,
    pub currency_category_id: u64,
    pub api_id: String,
    pub text: String,
    pub category_api_id: String,
    pub icon_url: Option<String>,
    /// Current price expressed in `BaseCurrency` (typically Exalted Orb).
    /// Some items ship `null` until the first price log.
    pub current_price: Option<f64>,
    pub current_quantity: Option<u64>,
}

/// Composite snapshot returned by [`fetch_snapshot`].
///
/// `entries` is keyed by `api_id` (poe2scout's slug, e.g.
/// `"perfect-exalted-orb"`); the caller maps to engine `CurrencyId`s.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoeScoutSnapshot {
    /// League the snapshot was taken against.
    pub league: String,
    /// Exalts per divine — used to convert prices to divine-equiv.
    pub divine_price_in_exalts: f64,
    /// Chaos per divine — informational; the advisor uses this to
    /// surface chaos prices when a player works in chaos.
    pub chaos_per_divine: f64,
    /// `api_id → entry` map.
    pub entries: HashMap<String, PoeScoutCurrencyEntry>,
    /// ISO-8601 timestamp of the fetch.
    pub fetched_at: String,
}

/// Errors a price fetch can raise.
#[derive(Debug, thiserror::Error)]
pub enum PriceError {
    #[cfg(feature = "net")]
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("league {0:?} not found in poe2scout response")]
    LeagueNotFound(String),
    #[error("JSON parse: {0}")]
    Json(#[from] serde_json::Error),
}

/// Fetch the live price snapshot from poe2scout.
///
/// `league` defaults to [`POE2SCOUT_DEFAULT_LEAGUE`]. Pass `None` for
/// `categories` to use [`POE2SCOUT_DEFAULT_CATEGORIES`].
#[cfg(feature = "net")]
pub async fn fetch_snapshot(
    client: &reqwest::Client,
    league: Option<&str>,
    categories: Option<&[&str]>,
) -> Result<PoeScoutSnapshot, PriceError> {
    let league_name = league.unwrap_or(POE2SCOUT_DEFAULT_LEAGUE);
    let cats = categories.unwrap_or(POE2SCOUT_DEFAULT_CATEGORIES);

    // Fetch leagues, find ours.
    let leagues_url = format!("{POE2SCOUT_BASE_URL}/{POE2SCOUT_REALM}/Leagues");
    let leagues: Vec<PoeScoutLeague> = client
        .get(&leagues_url)
        .timeout(Duration::from_secs(30))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let our_league = leagues
        .into_iter()
        .find(|l| l.value == league_name)
        .ok_or_else(|| PriceError::LeagueNotFound(league_name.to_string()))?;

    // Walk every category, paginating.
    let mut entries: HashMap<String, PoeScoutCurrencyEntry> = HashMap::new();
    for cat in cats {
        fetch_category_paginated(client, league_name, cat, &mut entries).await?;
    }

    Ok(PoeScoutSnapshot {
        league: our_league.value,
        divine_price_in_exalts: our_league.divine_price,
        chaos_per_divine: our_league.chaos_divine_price,
        entries,
        fetched_at: now_iso8601(),
    })
}

#[cfg(feature = "net")]
async fn fetch_category_paginated(
    client: &reqwest::Client,
    league: &str,
    category: &str,
    out: &mut HashMap<String, PoeScoutCurrencyEntry>,
) -> Result<(), PriceError> {
    let mut page = 1_u32;
    loop {
        let url = format!(
            "{base}/{realm}/Leagues/{league}/Currencies/ByCategory?Category={category}&Page={page}&PerPage=250",
            base = POE2SCOUT_BASE_URL,
            realm = POE2SCOUT_REALM,
            league = urlencoding::encode(league),
        );
        let resp: PoeScoutCategoryResponse = client
            .get(&url)
            .timeout(Duration::from_secs(30))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let last_page = resp.pages.max(1);
        for entry in resp.items {
            out.insert(entry.api_id.clone(), entry);
        }
        if page >= last_page {
            break;
        }
        page += 1;
    }
    Ok(())
}

/// Apply a price snapshot to a [`Valuator`], converting `current_price`
/// (denominated in exalts) to divine-equivalent triples via the
/// league's DivinePrice ratio.
///
/// Strategy:
/// - `expected = current_price / divine_price_in_exalts`
/// - `min = expected * 0.7` (optimistic 30% lower)
/// - `max = expected * 1.5` (pessimistic 50% higher)
///
/// These margins reflect the empirical volatility of late-league prices
/// (poe2scout's hourly digest swings ±30-50% on typical orbs).
///
/// `id_mapping` is a slug→`CurrencyId` map. The caller controls which
/// poe2scout `api_id`s feed which engine ids; see
/// [`default_id_mapping`] for the v1 baseline.
pub fn apply_feed_to_valuator<S: std::hash::BuildHasher>(
    valuator: &mut Valuator,
    snapshot: &PoeScoutSnapshot,
    id_mapping: &HashMap<String, CurrencyId, S>,
) -> usize {
    let mut applied = 0_usize;
    for (slug, entry) in &snapshot.entries {
        let Some(price_exalt) = entry.current_price else {
            continue;
        };
        let Some(currency_id) = id_mapping.get(slug) else {
            continue;
        };
        let expected = price_exalt / snapshot.divine_price_in_exalts;
        let band = DivEquiv {
            min: expected * 0.7,
            expected,
            max: expected * 1.5,
        };
        valuator.set(currency_id.clone(), band);
        applied += 1;
    }
    applied
}

/// The v1 baseline poe2scout slug → `CurrencyId` map.
///
/// Covers basic orbs, greater/perfect tiers, Hinekora's Lock, Fracturing
/// Orb, Mirror, plus the v2 additions (Phase F): omens (priced via the
/// `ritual` category) and essences across all four tiers + corrupted
/// variants (priced via `essences`). Slugs come from poe2scout's `api_id`
/// field.
#[must_use]
#[allow(clippy::too_many_lines)] // explicit slug → CurrencyId table is the contract
pub fn default_id_mapping() -> HashMap<String, CurrencyId> {
    let mut m = HashMap::new();
    let pairs = [
        // ---- Basic orbs + tier variants -------------------------------
        ("transmutation", "OrbOfTransmutation"),
        ("greater-orb-of-transmutation", "GreaterOrbOfTransmutation"),
        ("perfect-orb-of-transmutation", "PerfectOrbOfTransmutation"),
        ("augmentation", "OrbOfAugmentation"),
        ("greater-orb-of-augmentation", "GreaterOrbOfAugmentation"),
        ("perfect-orb-of-augmentation", "PerfectOrbOfAugmentation"),
        ("regal", "RegalOrb"),
        ("greater-regal-orb", "GreaterRegalOrb"),
        ("perfect-regal-orb", "PerfectRegalOrb"),
        ("alchemy", "OrbOfAlchemy"),
        ("exalted", "ExaltedOrb"),
        ("greater-exalted-orb", "GreaterExaltedOrb"),
        ("perfect-exalted-orb", "PerfectExaltedOrb"),
        ("annul", "OrbOfAnnulment"),
        ("chaos", "ChaosOrb"),
        ("greater-chaos-orb", "GreaterChaosOrb"),
        ("perfect-chaos-orb", "PerfectChaosOrb"),
        ("divine", "DivineOrb"),
        ("vaal", "VaalOrb"),
        ("hinekoras-lock", "HinekorasLock"),
        ("fracturing-orb", "FracturingOrb"),
        ("mirror", "MirrorOfKalandra"),
        ("artificers", "ArtificersOrb"),
        ("perfect-jewellers-orb", "PerfectJewellersOrb"),
        ("etcher", "ArcanistsEtcher"),
        ("scrap", "ArmourersScrap"),
        ("bauble", "GlassblowersBauble"),
        ("whetstone", "BlacksmithsWhetstone"),
        ("gcp", "GemcuttersPrism"),
        // ---- Omens (Phase F) ------------------------------------------
        // Mapped to the same CurrencyId namespace because the advisor's
        // scorer looks them up via `CurrencyId::from(omen.as_str())`.
        ("omen-of-whittling", "OmenOfWhittling"),
        ("omen-of-corruption", "OmenOfCorruption"),
        ("omen-of-light", "OmenOfLight"),
        ("omen-of-the-blessed", "OmenOfTheBlessed"),
        ("omen-of-sinistral-exaltation", "OmenOfSinistralExaltation"),
        ("omen-of-dextral-exaltation", "OmenOfDextralExaltation"),
        ("omen-of-greater-exaltation", "OmenOfGreaterExaltation"),
        ("omen-of-sinistral-erasure", "OmenOfSinistralErasure"),
        ("omen-of-dextral-erasure", "OmenOfDextralErasure"),
        (
            "omen-of-sinistral-crystallisation",
            "OmenOfSinistralCrystallisation",
        ),
        (
            "omen-of-dextral-crystallisation",
            "OmenOfDextralCrystallisation",
        ),
        ("omen-of-sinistral-necromancy", "OmenOfSinistralNecromancy"),
        ("omen-of-dextral-necromancy", "OmenOfDextralNecromancy"),
        ("omen-of-the-liege", "OmenOfTheLiege"),
        ("omen-of-the-sovereign", "OmenOfTheSovereign"),
        ("omen-of-the-blackblooded", "OmenOfTheBlackblooded"),
        ("omen-of-echoes-of-the-abyss", "OmenOfAbyssalEchoes"),
        ("omen-of-sanctification", "OmenOfSanctification"),
        // ---- Essences (Phase F) ---------------------------------------
        // poe2scout publishes essences under `Category=essences` with
        // slugs of the form `{quality-prefix}-essence-of-{name}` plus the
        // bare `essence-of-{name}` for the Normal tier. We register the
        // common keepers; the long tail matches via the pipeline-emitted
        // essence catalogue's slug field where available.
        ("essence-of-the-body", "EssenceOfTheBody"),
        ("essence-of-the-mind", "EssenceOfTheMind"),
        ("essence-of-flames", "EssenceOfFlames"),
        ("essence-of-ice", "EssenceOfIce"),
        ("essence-of-electricity", "EssenceOfElectricity"),
        ("essence-of-ruin", "EssenceOfRuin"),
        ("essence-of-battle", "EssenceOfBattle"),
        ("essence-of-sorcery", "EssenceOfSorcery"),
        ("essence-of-haste", "EssenceOfHaste"),
        ("essence-of-the-infinite", "EssenceOfTheInfinite"),
        ("essence-of-seeking", "EssenceOfSeeking"),
        ("essence-of-insulation", "EssenceOfInsulation"),
        ("essence-of-thawing", "EssenceOfThawing"),
        ("essence-of-grounding", "EssenceOfGrounding"),
        ("essence-of-alacrity", "EssenceOfAlacrity"),
        ("essence-of-opulence", "EssenceOfOpulence"),
        ("essence-of-command", "EssenceOfCommand"),
        ("greater-essence-of-the-body", "GreaterEssenceOfTheBody"),
        ("greater-essence-of-the-mind", "GreaterEssenceOfTheMind"),
        ("greater-essence-of-flames", "GreaterEssenceOfFlames"),
        ("greater-essence-of-ice", "GreaterEssenceOfIce"),
        (
            "greater-essence-of-electricity",
            "GreaterEssenceOfElectricity",
        ),
        ("greater-essence-of-battle", "GreaterEssenceOfBattle"),
        ("greater-essence-of-sorcery", "GreaterEssenceOfSorcery"),
        ("greater-essence-of-haste", "GreaterEssenceOfHaste"),
        (
            "greater-essence-of-the-infinite",
            "GreaterEssenceOfTheInfinite",
        ),
        ("greater-essence-of-seeking", "GreaterEssenceOfSeeking"),
        ("perfect-essence-of-the-body", "PerfectEssenceOfTheBody"),
        ("perfect-essence-of-the-mind", "PerfectEssenceOfTheMind"),
        ("perfect-essence-of-flames", "PerfectEssenceOfFlames"),
        ("perfect-essence-of-ice", "PerfectEssenceOfIce"),
        (
            "perfect-essence-of-electricity",
            "PerfectEssenceOfElectricity",
        ),
        ("perfect-essence-of-battle", "PerfectEssenceOfBattle"),
        ("perfect-essence-of-sorcery", "PerfectEssenceOfSorcery"),
        ("perfect-essence-of-haste", "PerfectEssenceOfHaste"),
        (
            "perfect-essence-of-the-infinite",
            "PerfectEssenceOfTheInfinite",
        ),
        ("perfect-essence-of-seeking", "PerfectEssenceOfSeeking"),
        // ---- Bones (Phase F) ------------------------------------------
        // poe2scout exposes bones under the `abyss` category. Slugs
        // observed: `gnawed-rib`, `preserved-rib`, etc.
        ("gnawed-rib", "GnawedRib"),
        ("gnawed-jawbone", "GnawedJawbone"),
        ("gnawed-collarbone", "GnawedCollarbone"),
        ("preserved-rib", "PreservedRib"),
        ("preserved-jawbone", "PreservedJawbone"),
        ("preserved-collarbone", "PreservedCollarbone"),
        ("preserved-cranium", "PreservedCranium"),
        ("ancient-rib", "AncientRib"),
        ("ancient-jawbone", "AncientJawbone"),
        ("ancient-collarbone", "AncientCollarbone"),
    ];
    for (slug, id) in pairs {
        m.insert(slug.to_string(), CurrencyId::from(id));
    }
    m
}

#[cfg(any(feature = "net", test))]
fn now_iso8601() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    iso8601_from_unix(secs)
}

#[cfg(any(feature = "net", test))]
#[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)]
fn iso8601_from_unix(secs: u64) -> String {
    let days = secs / 86_400;
    let secs_in_day = secs % 86_400;
    let hour = secs_in_day / 3600;
    let minute = (secs_in_day % 3600) / 60;
    let second = secs_in_day % 60;
    let z: i64 = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let mut year = (yoe as i64) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    if month <= 2 {
        year += 1;
    }
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_feed_converts_exalt_to_divine_triple() {
        let mut snapshot = PoeScoutSnapshot {
            league: "Fate of the Vaal".into(),
            divine_price_in_exalts: 200.0,
            chaos_per_divine: 25.0,
            entries: HashMap::new(),
            fetched_at: now_iso8601(),
        };
        snapshot.entries.insert(
            "divine".into(),
            PoeScoutCurrencyEntry {
                currency_item_id: 1,
                item_id: 1,
                currency_category_id: 21,
                api_id: "divine".into(),
                text: "Divine Orb".into(),
                category_api_id: "currency".into(),
                icon_url: None,
                current_price: Some(200.0), // 200 exalts -> 1 div
                current_quantity: None,
            },
        );
        snapshot.entries.insert(
            "exalted".into(),
            PoeScoutCurrencyEntry {
                currency_item_id: 2,
                item_id: 2,
                currency_category_id: 21,
                api_id: "exalted".into(),
                text: "Exalted Orb".into(),
                category_api_id: "currency".into(),
                icon_url: None,
                current_price: Some(1.0), // 1 exalt -> 1/200 div
                current_quantity: None,
            },
        );

        let mut v = Valuator::default();
        let applied = apply_feed_to_valuator(&mut v, &snapshot, &default_id_mapping());
        assert_eq!(applied, 2);

        let div = v.get(&CurrencyId::from("DivineOrb")).unwrap();
        assert!((div.expected - 1.0).abs() < 1e-9);
        let ex = v.get(&CurrencyId::from("ExaltedOrb")).unwrap();
        assert!((ex.expected - (1.0 / 200.0)).abs() < 1e-9);
    }

    #[test]
    fn entries_with_null_price_are_skipped() {
        let mut snapshot = PoeScoutSnapshot {
            league: "test".into(),
            divine_price_in_exalts: 100.0,
            chaos_per_divine: 30.0,
            entries: HashMap::new(),
            fetched_at: now_iso8601(),
        };
        snapshot.entries.insert(
            "divine".into(),
            PoeScoutCurrencyEntry {
                currency_item_id: 1,
                item_id: 1,
                currency_category_id: 21,
                api_id: "divine".into(),
                text: "Divine Orb".into(),
                category_api_id: "currency".into(),
                icon_url: None,
                current_price: None, // <-- null
                current_quantity: None,
            },
        );
        let mut v = Valuator::default();
        let applied = apply_feed_to_valuator(&mut v, &snapshot, &default_id_mapping());
        assert_eq!(applied, 0);
    }

    #[test]
    fn unknown_slug_does_not_pollute_valuator() {
        let mut snapshot = PoeScoutSnapshot {
            league: "test".into(),
            divine_price_in_exalts: 100.0,
            chaos_per_divine: 30.0,
            entries: HashMap::new(),
            fetched_at: now_iso8601(),
        };
        snapshot.entries.insert(
            "unknown-slug".into(),
            PoeScoutCurrencyEntry {
                currency_item_id: 1,
                item_id: 1,
                currency_category_id: 21,
                api_id: "unknown-slug".into(),
                text: "Unknown".into(),
                category_api_id: "currency".into(),
                icon_url: None,
                current_price: Some(50.0),
                current_quantity: None,
            },
        );
        let mut v = Valuator::default();
        let applied = apply_feed_to_valuator(&mut v, &snapshot, &default_id_mapping());
        assert_eq!(applied, 0);
    }

    #[test]
    fn iso8601_format_is_well_known() {
        assert_eq!(iso8601_from_unix(0), "1970-01-01T00:00:00Z");
        assert_eq!(iso8601_from_unix(1_577_836_800), "2020-01-01T00:00:00Z");
    }

    #[test]
    fn default_id_mapping_includes_all_basic_orbs() {
        let m = default_id_mapping();
        for slug in ["divine", "chaos", "exalted", "vaal", "hinekoras-lock"] {
            assert!(m.contains_key(slug), "missing slug: {slug}");
        }
    }

    /// Every omen id the engine implements, keyed by its canonical id string
    /// (the `Omen::new(...)` constructor arguments in `poc2_engine::omen`).
    fn engine_omen_canon() -> std::collections::HashSet<String> {
        use poc2_engine::Omen;
        [
            Omen::sinistral_exaltation(),
            Omen::dextral_exaltation(),
            Omen::greater_exaltation(),
            Omen::sinistral_annulment(),
            Omen::dextral_annulment(),
            Omen::sinistral_erasure(),
            Omen::dextral_erasure(),
            Omen::sinistral_crystallisation(),
            Omen::dextral_crystallisation(),
            Omen::sinistral_necromancy(),
            Omen::dextral_necromancy(),
            Omen::whittling(),
            Omen::light(),
            Omen::abyssal_echoes(),
            Omen::corruption(),
            Omen::sanctification(),
            Omen::blessed(),
            Omen::catalysing_exaltation(),
            Omen::blackblooded(),
            Omen::liege(),
            Omen::sovereign(),
            Omen::homogenising_exaltation(),
            Omen::homogenising_coronation(),
        ]
        .into_iter()
        .map(|o| o.id.as_str().to_string())
        .collect()
    }

    /// Essence ids are bundle-driven (`{Greater|Perfect}?EssenceOf...`); the
    /// static resolver can't validate them, so the canon test accepts the
    /// naming scheme instead.
    fn is_essence_id(s: &str) -> bool {
        let tail = s
            .strip_prefix("Greater")
            .or_else(|| s.strip_prefix("Perfect"))
            .unwrap_or(s);
        tail.starts_with("EssenceOf")
    }

    #[test]
    fn default_id_mapping_targets_known_engine_ids() {
        use poc2_engine::currency::{CurrencyResolver, DefaultCurrencyResolver};

        // Quality currencies + Mirror live only in the valuator's fallback
        // table / bundle catalogues, not in the static resolver.
        const CATALOGUE_IDS: &[&str] = &[
            "MirrorOfKalandra",
            "ArtificersOrb",
            "PerfectJewellersOrb",
            "ArcanistsEtcher",
            "ArmourersScrap",
            "GlassblowersBauble",
            "BlacksmithsWhetstone",
            "GemcuttersPrism",
        ];

        let omen_canon = engine_omen_canon();
        let resolver = DefaultCurrencyResolver::new();
        for (slug, id) in default_id_mapping() {
            let s = id.as_str();
            let known = omen_canon.contains(s)
                || resolver.resolve(&id).is_some()
                || CATALOGUE_IDS.contains(&s)
                || is_essence_id(s);
            assert!(known, "slug {slug:?} maps to unknown engine id {s:?}");
        }
    }

    /// Regression for the audit-found mismatches: poe2scout's blackblooded /
    /// echoes-of-the-abyss slugs must land on the engine's canonical omen ids.
    #[test]
    fn audited_omen_slugs_map_to_engine_canon() {
        use poc2_engine::Omen;
        let m = default_id_mapping();
        assert_eq!(
            m["omen-of-the-blackblooded"].as_str(),
            Omen::blackblooded().id.as_str()
        );
        assert_eq!(
            m["omen-of-echoes-of-the-abyss"].as_str(),
            Omen::abyssal_echoes().id.as_str()
        );
    }
}
