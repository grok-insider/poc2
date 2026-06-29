//! `resolve` command — fuzzy-resolve a noisy item/currency name onto a
//! canonical key.
//!
//! Pure compute over the matcher in [`poc2_market::name_match`]. With no
//! `candidates`, it resolves against the engine valuator's currency
//! display-name index (returning the matched [`CurrencyId`] string). With
//! `candidates`, it builds an ad-hoc [`NameIndex`] from the caller-supplied
//! keys and resolves against those instead — letting the UI fuzzy-match any
//! arbitrary list (mod text, base names, …) through the same engine.

use poc2_market::{bundled_translator, NameIndex, Valuator};
use serde::Serialize;

/// Result of one name resolution.
#[derive(Debug, Serialize)]
pub struct ResolveView {
    /// The matched canonical key (a `CurrencyId` string in valuator mode, or
    /// the matched candidate in ad-hoc mode), or `None` if nothing matched.
    pub key: Option<String>,
    /// Confidence score in `[0, 1]` (`0.0` when unmatched).
    pub score: f64,
    /// Match stage: `"exact"`, `"prefix"`, `"fuzzy"`, `"skeleton"`, or
    /// `"none"` when unresolved.
    pub method: String,
}

/// Resolve `raw` to a canonical key.
///
/// When `candidates` is `Some`, the lookup is over an ad-hoc index built from
/// those keys (the matched candidate is returned verbatim). When `None`, the
/// lookup is over the valuator's currencies and the returned `key` is the
/// engine [`CurrencyId`] string.
///
/// `locale` optionally names a bundled locale (`de`/`fr`/`pt`/`ru`/`sp`); when
/// set, a localized client name is translated to its canonical English form
/// before scoring. An unknown locale code is ignored (English passthrough).
pub fn resolve_name(
    valuator: &Valuator,
    raw: &str,
    candidates: Option<Vec<String>>,
    locale: Option<&str>,
) -> ResolveView {
    let translator = locale.and_then(bundled_translator);
    match candidates {
        Some(keys) => {
            let index = NameIndex::new(keys);
            match index.resolve_with(raw, translator.as_ref()) {
                Some(m) => ResolveView {
                    key: Some(m.key),
                    score: m.score,
                    method: m.method.to_string(),
                },
                None => ResolveView {
                    key: None,
                    score: 0.0,
                    method: "none".to_string(),
                },
            }
        }
        None => {
            // Translate a localized name to English first, then resolve over
            // the valuator's currencies (which are English-keyed).
            let english = translator
                .as_ref()
                .and_then(|t| t.translate(raw))
                .unwrap_or_else(|| raw.to_string());
            match valuator.resolve_name(&english) {
                Some(id) => ResolveView {
                    key: Some(id.as_str().to_string()),
                    // The valuator path returns only the id; re-score is not
                    // exposed there, so report a confident match.
                    score: 1.0,
                    method: "currency".to_string(),
                },
                None => ResolveView {
                    key: None,
                    score: 0.0,
                    method: "none".to_string(),
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use poc2_engine::ids::CurrencyId;

    #[test]
    fn ad_hoc_candidates_resolve_via_fuzzy() {
        let v = Valuator::default();
        let cands = vec![
            "greater vision rune".to_string(),
            "rebirth".to_string(),
            "vision rune".to_string(),
        ];
        let view = resolve_name(&v, "greater viswn rune", Some(cands), None);
        assert_eq!(view.key.as_deref(), Some("greater vision rune"));
        assert!(view.score >= 0.84);
        assert!(matches!(view.method.as_str(), "fuzzy" | "skeleton"));
    }

    #[test]
    fn ad_hoc_unmatched_reports_none() {
        let v = Valuator::default();
        let cands = vec!["rebirth".to_string()];
        let view = resolve_name(&v, "totally different thing", Some(cands), None);
        assert!(view.key.is_none());
        assert_eq!(view.method, "none");
        assert!((view.score - 0.0).abs() < 1e-9);
    }

    #[test]
    fn valuator_mode_resolves_currency_id() {
        let v = Valuator::default();
        let view = resolve_name(&v, "Orb of Transmutation", None, None);
        assert_eq!(
            view.key.as_deref(),
            Some(CurrencyId::from("OrbOfTransmutation").as_str())
        );
    }

    #[test]
    fn valuator_mode_unmatched_reports_none() {
        let v = Valuator::default();
        let view = resolve_name(&v, "zzzz qqqq not a currency", None, None);
        assert!(view.key.is_none());
        assert_eq!(view.method, "none");
    }

    #[test]
    fn locale_translates_before_currency_resolve() {
        let v = Valuator::default();
        // German "Kugel der Transmutation" → "Orb of Transmutation" → id.
        let view = resolve_name(&v, "Kugel der Transmutation", None, Some("de"));
        assert_eq!(
            view.key.as_deref(),
            Some(CurrencyId::from("OrbOfTransmutation").as_str())
        );
        // Unknown locale code is ignored (English passthrough still works).
        let view2 = resolve_name(&v, "Orb of Transmutation", None, Some("xx"));
        assert_eq!(
            view2.key.as_deref(),
            Some(CurrencyId::from("OrbOfTransmutation").as_str())
        );
    }

    #[test]
    fn locale_translates_ad_hoc_candidates() {
        let v = Valuator::default();
        let cands = vec!["mirror of kalandra".to_string(), "divine orb".to_string()];
        // French localized name resolves to the English candidate.
        let view = resolve_name(&v, "Miroir de Kalandra", Some(cands), Some("fr"));
        assert_eq!(view.key.as_deref(), Some("mirror of kalandra"));
    }
}
