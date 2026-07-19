//! `resolve` command — fuzzy-resolve a noisy item/currency name onto a
//! canonical key.
//!
//! Pure compute over the matcher in [`poc2_market::name_match`]. With no
//! `candidates`, it resolves against the engine valuator's currency
//! display-name index (returning the matched [`CurrencyId`] string). With
//! `candidates`, it builds an ad-hoc [`NameIndex`] from the caller-supplied
//! keys and resolves against those instead — letting the UI fuzzy-match any
//! arbitrary list (mod text, base names, …) through the same engine.

use std::collections::HashMap;

use poc2_market::{bundled_translator, name_match::normalize, NameIndex, NameTranslator, Valuator};
use serde::{Deserialize, Serialize};

/// JSON request for resolving one name.
#[derive(Debug, Deserialize)]
pub struct ResolveNameArgs {
    pub raw: String,
    #[serde(default)]
    pub candidates: Option<Vec<String>>,
    #[serde(default)]
    pub locale: Option<String>,
}

/// JSON request for resolving multiple names against one candidate set.
#[derive(Debug, Deserialize)]
pub struct ResolveNamesArgs {
    pub raws: Vec<String>,
    #[serde(default)]
    pub candidates: Option<Vec<String>>,
    #[serde(default)]
    pub locale: Option<String>,
}

/// Result of one name resolution.
#[derive(Debug, PartialEq, Serialize)]
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
            let (index, display_names) = candidate_index(keys);
            resolve_candidate(&index, &display_names, raw, translator.as_ref())
        }
        None => resolve_currency(valuator, raw, translator.as_ref()),
    }
}

/// Resolve `raws` in input order, reusing one ad-hoc candidate index.
pub fn resolve_names(
    valuator: &Valuator,
    raws: &[String],
    candidates: Option<Vec<String>>,
    locale: Option<&str>,
) -> Vec<ResolveView> {
    let translator = locale.and_then(bundled_translator);
    match candidates {
        Some(keys) => {
            let (index, display_names) = candidate_index(keys);
            raws.iter()
                .map(|raw| resolve_candidate(&index, &display_names, raw, translator.as_ref()))
                .collect()
        }
        None => raws
            .iter()
            .map(|raw| resolve_currency(valuator, raw, translator.as_ref()))
            .collect(),
    }
}

fn resolve_candidate(
    index: &NameIndex,
    display_names: &HashMap<String, String>,
    raw: &str,
    translator: Option<&NameTranslator>,
) -> ResolveView {
    match index.resolve_with(raw, translator) {
        Some(m) => ResolveView {
            key: Some(display_names.get(&m.key).cloned().unwrap_or(m.key)),
            score: m.score,
            method: m.method.to_string(),
        },
        None => unresolved(),
    }
}

fn candidate_index(keys: Vec<String>) -> (NameIndex, HashMap<String, String>) {
    let mut display_names = HashMap::new();
    for key in &keys {
        display_names
            .entry(normalize(key))
            .or_insert_with(|| key.clone());
    }
    (NameIndex::new(keys), display_names)
}

fn resolve_currency(
    valuator: &Valuator,
    raw: &str,
    translator: Option<&NameTranslator>,
) -> ResolveView {
    // Translate a localized name to English first, then resolve over the
    // valuator's currencies (which are English-keyed).
    let english = translator
        .and_then(|t| t.translate(raw))
        .unwrap_or_else(|| raw.to_string());
    match valuator.resolve_name(&english) {
        Some(id) => ResolveView {
            key: Some(id.as_str().to_string()),
            // The valuator path returns only the id; re-score is not exposed
            // there, so report a confident match.
            score: 1.0,
            method: "currency".to_string(),
        },
        None => unresolved(),
    }
}

fn unresolved() -> ResolveView {
    ResolveView {
        key: None,
        score: 0.0,
        method: "none".to_string(),
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

    #[test]
    fn batch_preserves_input_order() {
        let v = Valuator::default();
        let raws = vec![
            "Orb of Transmutation".to_string(),
            "zzzz qqqq not a currency".to_string(),
            "Orb of Augmentation".to_string(),
        ];
        let views = resolve_names(&v, &raws, None, None);

        assert_eq!(views.len(), raws.len());
        assert_eq!(views[0].key.as_deref(), Some("OrbOfTransmutation"));
        assert!(views[1].key.is_none());
        assert_eq!(views[2].key.as_deref(), Some("OrbOfAugmentation"));
    }

    #[test]
    fn batch_resolves_against_shared_candidates() {
        let v = Valuator::default();
        let raws = vec!["greater viswn rune".to_string(), "rebirth".to_string()];
        let candidates = vec![
            "greater vision rune".to_string(),
            "rebirth".to_string(),
            "vision rune".to_string(),
        ];
        let views = resolve_names(&v, &raws, Some(candidates), None);

        assert_eq!(views[0].key.as_deref(), Some("greater vision rune"));
        assert_eq!(views[1].key.as_deref(), Some("rebirth"));
    }

    #[test]
    fn candidate_resolution_preserves_display_casing() {
        let v = Valuator::default();
        let candidates = vec!["Rune of Vital Flame".to_string(), "Ward Rune".to_string()];
        let view = resolve_name(&v, "rune of vital flame", Some(candidates), None);

        assert_eq!(view.key.as_deref(), Some("Rune of Vital Flame"));
    }

    #[test]
    fn batch_empty_input_returns_empty_output() {
        let v = Valuator::default();
        let raws = Vec::new();

        assert!(resolve_names(&v, &raws, None, None).is_empty());
        assert!(resolve_names(&v, &raws, Some(vec!["rebirth".to_string()]), None).is_empty());
    }

    #[test]
    fn batch_matches_single_name_resolution() {
        let v = Valuator::default();
        let currency_raws = vec![
            "Kugel der Transmutation".to_string(),
            "zzzz qqqq not a currency".to_string(),
        ];
        let batched = resolve_names(&v, &currency_raws, None, Some("de"));
        let singles: Vec<_> = currency_raws
            .iter()
            .map(|raw| resolve_name(&v, raw, None, Some("de")))
            .collect();
        assert_eq!(batched, singles);

        let raws = vec![
            "Miroir de Kalandra".to_string(),
            "Orbe divin".to_string(),
            "totally different thing".to_string(),
        ];
        let candidates = vec!["mirror of kalandra".to_string(), "divine orb".to_string()];
        let batched = resolve_names(&v, &raws, Some(candidates.clone()), Some("fr"));
        let singles: Vec<_> = raws
            .iter()
            .map(|raw| resolve_name(&v, raw, Some(candidates.clone()), Some("fr")))
            .collect();

        assert_eq!(batched, singles);
    }
}
