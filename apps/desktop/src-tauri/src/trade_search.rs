//! Trade-search URL adapter (Phase D.3).
//!
//! v1 ships URL-only trade integration: build a deep-link to the
//! official `pathofexile.com/trade2/search/<league>/...` page from
//! the current item's mods, and open it in the user's default
//! browser via `tauri-plugin-shell`.
//!
//! GGG OAuth integration ships in v1.x. For v1 we encode the search
//! state into the URL fragment (`#`) so it survives the redirect
//! through the trade page's hash-router.

use poc2_engine::item::{AffixType, Item, ModRoll};
use serde::Serialize;

/// Build a `pathofexile.com/trade2/search/<league>/...` URL preset
/// to look for items matching the given item's mods.
///
/// Encoding strategy:
/// - Item class becomes a `type` filter (best-effort).
/// - Each non-fractured mod's text template becomes a `stat` line
///   (we don't have the GGG-internal stat-id mapping yet — Phase E.1
///   adds that via poe.ninja's stat-id table).
/// - Fractured mods are pre-marked `fractured = true`.
/// - ilvl is encoded as a `min` filter.
///
/// Returns the resulting URL as a string. Also returns a structured
/// `TradeSearchSummary` so the UI can show what's about to be sent.
#[must_use]
pub fn build_trade_search_url(item: &Item, league: &str) -> TradeSearchSummary {
    let league_slug = urlencoding::encode(league);
    let mod_lines: Vec<TradeModLine> = item
        .prefixes
        .iter()
        .chain(item.suffixes.iter())
        .map(mod_line_for)
        .collect();
    // The trade2 URL accepts a JSON-encoded query in the fragment;
    // we encode a minimal-but-useful query that the trade page will
    // pick up via its hash-router.
    let query = serde_json::json!({
        "query": {
            "status": { "option": "online" },
            "type": item.base.as_str(),
            "filters": {
                "misc_filters": {
                    "filters": {
                        "ilvl": { "min": item.ilvl }
                    }
                }
            },
            "stats": [
                {
                    "type": "and",
                    "filters": mod_lines
                        .iter()
                        .map(|m| serde_json::json!({
                            "id": m.text_template.clone().unwrap_or_default(),
                            "value": { "min": m.values.first().copied().unwrap_or(0.0) },
                            "disabled": false
                        }))
                        .collect::<Vec<_>>()
                }
            ]
        }
    });
    let encoded = urlencoding::encode(&query.to_string()).into_owned();
    let url = format!("https://www.pathofexile.com/trade2/search/{league_slug}#q={encoded}");
    TradeSearchSummary {
        url,
        league: league.to_string(),
        item_class: item.base.as_str().to_string(),
        ilvl_min: item.ilvl,
        mod_lines,
    }
}

#[derive(Debug, Serialize)]
pub struct TradeSearchSummary {
    pub url: String,
    pub league: String,
    pub item_class: String,
    pub ilvl_min: u32,
    pub mod_lines: Vec<TradeModLine>,
}

#[derive(Debug, Serialize, Clone)]
pub struct TradeModLine {
    pub mod_id: String,
    pub affix_type: String,
    pub fractured: bool,
    pub text_template: Option<String>,
    pub values: Vec<f64>,
}

fn mod_line_for(roll: &ModRoll) -> TradeModLine {
    TradeModLine {
        mod_id: roll.mod_id.as_str().to_string(),
        affix_type: match roll.affix_type {
            AffixType::Prefix => "prefix",
            AffixType::Suffix => "suffix",
            AffixType::Implicit => "implicit",
            AffixType::Enchantment => "enchantment",
        }
        .to_string(),
        fractured: roll.is_fractured,
        text_template: None, // populated by the engine's mod registry in M5+
        values: roll.values.iter().copied().collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use poc2_engine::ids::{ItemClassId, ModId};
    use poc2_engine::item::{ModRoll, QualityKind, Rarity};
    use poc2_engine::mods::ModKind;
    use smallvec::smallvec;

    fn fixture_rare_with_one_mod() -> Item {
        Item {
            base: ItemClassId::from("BodyArmour").as_str().into(),
            ilvl: 82,
            rarity: Rarity::Rare,
            corrupted: false,
            sanctified: false,
            mirrored: false,
            quality: 0,
            quality_kind: QualityKind::Untagged,
            implicits: smallvec![],
            prefixes: smallvec![ModRoll {
                mod_id: ModId::from("LocalIncreasedEnergyShield5"),
                affix_type: AffixType::Prefix,
                kind: ModKind::Explicit,
                values: smallvec![60.0],
                is_fractured: false,
            }],
            suffixes: smallvec![],
            enchantments: smallvec![],
            hidden_desecrated: None,
            sockets: smallvec![],
            hinekora_lock: None,
        }
    }

    #[test]
    fn url_includes_league_and_item_class() {
        let item = fixture_rare_with_one_mod();
        let summary = build_trade_search_url(&item, "Fate of the Vaal");
        assert!(summary
            .url
            .contains("https://www.pathofexile.com/trade2/search/Fate%20of%20the%20Vaal"));
        assert_eq!(summary.item_class, "BodyArmour");
        assert_eq!(summary.ilvl_min, 82);
        assert_eq!(summary.mod_lines.len(), 1);
        assert_eq!(summary.mod_lines[0].mod_id, "LocalIncreasedEnergyShield5");
    }

    #[test]
    fn url_marks_fractured_mods() {
        let mut item = fixture_rare_with_one_mod();
        item.prefixes[0].is_fractured = true;
        let summary = build_trade_search_url(&item, "Fate of the Vaal");
        assert!(summary.mod_lines[0].fractured);
    }
}
