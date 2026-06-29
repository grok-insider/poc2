//! `ninja_prices` command — apply a poe.ninja PoE2 exchange snapshot to the
//! engine's valuator.
//!
//! This is the PARALLEL source to [`super::prices`] (poe2scout). The browser
//! fetches the feed itself (the WASM build has no network stack) and hands the
//! [`poc2_market::NinjaExchangeSnapshot`] JSON across the boundary; this module
//! is pure compute over [`poc2_market::apply_ninja_to_valuator`].
//!
//! Unlike poe2scout (slug → `CurrencyId` table), poe.ninja entries are keyed by
//! display name and resolved via the engine valuator's fuzzy matcher
//! ([`poc2_market::Valuator::resolve_name`]). Entry names that don't resolve are
//! reported back as `unmatched` so the UI can surface coverage gaps instead of
//! silently dropping prices — mirroring the `ApplyPricesView` shape from
//! [`super::prices`].

use poc2_market::{apply_ninja_to_valuator, NinjaExchangeSnapshot, Valuator};

pub use super::prices::ApplyPricesView;

/// Merge `snapshot` into `valuator`, resolving each entry's display name onto a
/// `CurrencyId` via the fuzzy matcher. `unmatched` collects the (normalized)
/// names of market-data-bearing entries that didn't resolve.
pub fn apply_ninja_prices(
    valuator: &mut Valuator,
    snapshot: &NinjaExchangeSnapshot,
) -> ApplyPricesView {
    let applied = apply_ninja_to_valuator(valuator, snapshot);

    let mut unmatched: Vec<String> = snapshot
        .entries
        .iter()
        .filter(|(_, entry)| entry.has_market_data)
        .filter(|(key, _)| valuator.resolve_name(key).is_none())
        .map(|(key, _)| key.clone())
        .collect();
    unmatched.sort();

    ApplyPricesView { applied, unmatched }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    use poc2_engine::ids::CurrencyId;
    use poc2_market::NinjaPriceEntry;

    fn snapshot(entries: &[(&str, f64, bool)]) -> NinjaExchangeSnapshot {
        NinjaExchangeSnapshot {
            league: "test".into(),
            entries: entries
                .iter()
                .map(|(name, div, has)| {
                    (
                        poc2_market::name_match::normalize(name),
                        NinjaPriceEntry {
                            divine_value: *div,
                            exalt_value: *div * 200.0,
                            has_market_data: *has,
                        },
                    )
                })
                .collect::<HashMap<_, _>>(),
            fetched_at: "2026-01-01T00:00:00Z".into(),
        }
    }

    #[test]
    fn applies_resolvable_entries_and_reports_unmatched() {
        let snap = snapshot(&[
            ("Divine Orb", 1.0, true),
            ("Exalted Orb", 0.005, true),
            ("Zzzz Not A Currency", 9.0, true),
        ]);
        let mut v = Valuator::default();
        let view = apply_ninja_prices(&mut v, &snap);
        assert_eq!(view.applied, 2);
        assert_eq!(view.unmatched, vec!["zzzz not a currency".to_string()]);
        let div = v.get(&CurrencyId::from("DivineOrb")).unwrap();
        assert!((div.expected - 1.0).abs() < 1e-9);
    }

    #[test]
    fn entries_without_market_data_are_neither_applied_nor_unmatched() {
        let snap = snapshot(&[("Divine Orb", 0.0, false)]);
        let mut v = Valuator::default();
        let view = apply_ninja_prices(&mut v, &snap);
        assert_eq!(view.applied, 0);
        assert!(view.unmatched.is_empty());
    }
}
