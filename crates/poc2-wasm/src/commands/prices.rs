//! `prices` command — apply a poe2scout price snapshot to the engine's
//! valuator.
//!
//! The browser fetches the feed itself (the WASM build has no network
//! stack) and hands the snapshot JSON across the boundary; this module is
//! pure compute over [`poc2_market::apply_feed_to_valuator`] with the v1
//! [`poc2_market::default_id_mapping`]. Slugs the mapping doesn't know are
//! reported back so the UI can surface coverage gaps instead of silently
//! dropping prices.

use poc2_market::{apply_feed_to_valuator, default_id_mapping, PoeScoutSnapshot, Valuator};
use serde::Serialize;

/// Summary of one snapshot application.
#[derive(Debug, Serialize)]
pub struct ApplyPricesView {
    /// Feed entries that mapped to an engine currency id and carried a price.
    pub applied: usize,
    /// Feed slugs with no engine mapping (sorted for stable display).
    pub unmatched: Vec<String>,
}

/// Merge `snapshot` into `valuator` via the default slug → `CurrencyId` map.
pub fn apply_prices(valuator: &mut Valuator, snapshot: &PoeScoutSnapshot) -> ApplyPricesView {
    let mapping = default_id_mapping();
    let applied = apply_feed_to_valuator(valuator, snapshot, &mapping);
    let mut unmatched: Vec<String> = snapshot
        .entries
        .keys()
        .filter(|slug| !mapping.contains_key(*slug))
        .cloned()
        .collect();
    unmatched.sort();
    ApplyPricesView { applied, unmatched }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    use poc2_engine::ids::CurrencyId;
    use poc2_market::PoeScoutCurrencyEntry;

    fn entry(slug: &str, price: Option<f64>) -> PoeScoutCurrencyEntry {
        PoeScoutCurrencyEntry {
            currency_item_id: 1,
            item_id: 1,
            currency_category_id: 21,
            api_id: slug.into(),
            text: slug.into(),
            category_api_id: "currency".into(),
            icon_url: None,
            current_price: price,
            current_quantity: None,
        }
    }

    fn snapshot(entries: &[(&str, Option<f64>)]) -> PoeScoutSnapshot {
        PoeScoutSnapshot {
            league: "test".into(),
            divine_price_in_exalts: 200.0,
            chaos_per_divine: 25.0,
            entries: entries
                .iter()
                .map(|(slug, price)| ((*slug).to_string(), entry(slug, *price)))
                .collect::<HashMap<_, _>>(),
            fetched_at: "2026-01-01T00:00:00Z".into(),
        }
    }

    #[test]
    fn applies_mapped_entries_and_reports_unmatched() {
        let snap = snapshot(&[
            ("divine", Some(200.0)),
            ("exalted", Some(1.0)),
            ("some-unknown-slug", Some(3.0)),
        ]);
        let mut v = Valuator::default();
        let view = apply_prices(&mut v, &snap);
        assert_eq!(view.applied, 2);
        assert_eq!(view.unmatched, vec!["some-unknown-slug".to_string()]);
        let div = v.get(&CurrencyId::from("DivineOrb")).unwrap();
        assert!((div.expected - 1.0).abs() < 1e-9);
    }

    #[test]
    fn mapped_entry_without_price_counts_as_matched_but_not_applied() {
        let snap = snapshot(&[("divine", None)]);
        let mut v = Valuator::default();
        let view = apply_prices(&mut v, &snap);
        assert_eq!(view.applied, 0);
        assert!(view.unmatched.is_empty());
    }
}
