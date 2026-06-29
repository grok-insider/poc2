//! Fuzzy item-name matcher.
//!
//! A clean-room, WASM-safe resolver that maps a noisy user/OCR-supplied
//! item name onto one of a known set of canonical keys. It is deliberately
//! conservative: it prefers returning `None` over a wrong match, and each
//! accepted match reports *how* it was found (`exact` / `prefix` / `fuzzy`
//! / `skeleton`) plus a `[0, 1]` confidence score.
//!
//! ## Pipeline
//!
//! Every key and query is first **normalized** (lowercased, punctuation
//! flattened to single spaces) and **folded** (Latin diacritics stripped).
//! [`NameIndex::resolve`] then tries, in order:
//!
//! 1. **exact** — the normalized query equals a normalized key.
//! 2. **prefix** — for queries of length ≥ 10, the shortest key that
//!    `starts_with` the query (handles trailing truncation).
//! 3. **fuzzy** — Levenshtein similarity `1 - dist/max_len` ≥ `0.84`
//!    (≥ `0.92` is flagged high-confidence), with `±3`-length candidate
//!    pruning so we only score keys of a plausible size.
//! 4. **skeleton** — collapse OCR-confusable glyph classes (e.g. `m`/`n`/`u`
//!    → `n`, `0`/`o` → `o`) and compare skeletons; accepted at similarity
//!    ≥ `0.72`, or at a low floor of `0.55` only when it beats the
//!    second-best skeleton candidate by a margin ≥ `0.18`.
//!
//! The matcher carries no locale knowledge: translation (mapping a
//! localized name to its canonical English key) is a separate concern with
//! a clearly-marked seam in [`NameIndex::resolve`].

use std::collections::HashMap;

use strsim::levenshtein as strsim_levenshtein;

/// Fuzzy similarity floor for an accepted Levenshtein match.
const FUZZY_ACCEPT: f64 = 0.84;
/// Similarity at/above which a fuzzy match is treated as high-confidence.
const FUZZY_HIGH: f64 = 0.92;
/// Minimum query length before the prefix-truncation rule is allowed.
const PREFIX_MIN_LEN: usize = 10;
/// Skeleton similarity floor for an outright accepted skeleton match.
const SKELETON_ACCEPT: f64 = 0.72;
/// Low skeleton floor, accepted only when the margin over the runner-up
/// clears [`SKELETON_MARGIN`].
const SKELETON_LOW_FLOOR: f64 = 0.55;
/// Required lead over the second-best skeleton candidate at the low floor.
const SKELETON_MARGIN: f64 = 0.18;
/// `±N` length window used to prune fuzzy/skeleton candidates.
const LEN_WINDOW: usize = 3;

/// Lowercase, flatten every non-alphanumeric run to a single space, trim.
///
/// `"  Greater   Vision-Rune!! "` → `"greater vision rune"`.
#[must_use]
pub fn normalize(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut pending_space = false;
    for ch in s.chars() {
        if ch.is_alphanumeric() {
            if pending_space && !out.is_empty() {
                out.push(' ');
            }
            pending_space = false;
            for lc in ch.to_lowercase() {
                out.push(lc);
            }
        } else {
            // Any non-alphanumeric char (incl. existing whitespace) becomes a
            // boundary; collapsing happens because we only emit one space
            // before the next alphanumeric run.
            pending_space = true;
        }
    }
    out
}

/// Fold common Latin diacritics down to their ASCII base letter; characters
/// outside the handled set (including all non-Latin scripts) pass through
/// unchanged. Operates per-`char`, so it is safe on already-normalized or
/// raw input alike.
#[must_use]
pub fn fold(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            // a-family
            'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ă' | 'ą' => out.push('a'),
            'À' | 'Á' | 'Â' | 'Ã' | 'Ä' | 'Å' | 'Ā' | 'Ă' | 'Ą' => out.push('A'),
            // e-family
            'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ĕ' | 'ė' | 'ę' | 'ě' => out.push('e'),
            'È' | 'É' | 'Ê' | 'Ë' | 'Ē' | 'Ĕ' | 'Ė' | 'Ę' | 'Ě' => out.push('E'),
            // i-family
            'ì' | 'í' | 'î' | 'ï' | 'ī' | 'ĭ' | 'į' | 'ı' => out.push('i'),
            'Ì' | 'Í' | 'Î' | 'Ï' | 'Ī' | 'Ĭ' | 'Į' | 'İ' => out.push('I'),
            // o-family
            'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' | 'ŏ' | 'ő' => out.push('o'),
            'Ò' | 'Ó' | 'Ô' | 'Õ' | 'Ö' | 'Ø' | 'Ō' | 'Ŏ' | 'Ő' => out.push('O'),
            // u-family
            'ù' | 'ú' | 'û' | 'ü' | 'ū' | 'ŭ' | 'ů' | 'ű' | 'ų' => out.push('u'),
            'Ù' | 'Ú' | 'Û' | 'Ü' | 'Ū' | 'Ŭ' | 'Ů' | 'Ű' | 'Ų' => out.push('U'),
            // n / c with diacritics
            'ñ' | 'ń' | 'ņ' | 'ň' => out.push('n'),
            'Ñ' | 'Ń' | 'Ņ' | 'Ň' => out.push('N'),
            'ç' | 'ć' | 'ĉ' | 'ċ' | 'č' => out.push('c'),
            'Ç' | 'Ć' | 'Ĉ' | 'Ċ' | 'Č' => out.push('C'),
            // y with diacritics
            'ý' | 'ÿ' => out.push('y'),
            'Ý' | 'Ÿ' => out.push('Y'),
            // sharp-s and common ligatures expand to ASCII digraphs
            'ß' => out.push_str("ss"),
            'æ' => out.push_str("ae"),
            'Æ' => out.push_str("AE"),
            'œ' => out.push_str("oe"),
            'Œ' => out.push_str("OE"),
            other => out.push(other),
        }
    }
    out
}

/// Collapse OCR-confusable glyph classes on the *normalized* form of `s`.
///
/// Classes (each member maps to the class representative):
/// - `{w, m, n, u}` → `n`
/// - `{r, v}` → `r`
/// - `{i, l, j, t, 1}` → `i`
/// - `{o, 0, e, c}` → `o`
/// - `{4, a}` → `a`
///
/// Everything else is preserved. The input is normalized first so spacing
/// and punctuation never affect the skeleton.
#[must_use]
pub fn skeleton(s: &str) -> String {
    let normalized = normalize(s);
    let mut out = String::with_capacity(normalized.len());
    for ch in normalized.chars() {
        let mapped = match ch {
            'w' | 'm' | 'n' | 'u' => 'n',
            'r' | 'v' => 'r',
            'i' | 'l' | 'j' | 't' | '1' => 'i',
            'o' | '0' | 'e' | 'c' => 'o',
            '4' | 'a' => 'a',
            other => other,
        };
        out.push(mapped);
    }
    out
}

/// Levenshtein edit distance between two strings (via `strsim`).
#[must_use]
pub fn levenshtein(a: &str, b: &str) -> usize {
    strsim_levenshtein(a, b)
}

/// Normalized similarity in `[0, 1]`: `1 - dist / max(len_a, len_b)`.
/// Two empty strings are treated as identical.
//
// Lengths here are short item-name strings, so the `usize → f64` casts can
// never lose precision in practice.
#[allow(clippy::cast_precision_loss)]
fn similarity(a: &str, b: &str) -> f64 {
    let la = a.chars().count();
    let lb = b.chars().count();
    let max_len = la.max(lb);
    if max_len == 0 {
        return 1.0;
    }
    let dist = levenshtein(a, b);
    1.0 - (dist as f64) / (max_len as f64)
}

/// A resolved match: which canonical key, the confidence score in `[0, 1]`,
/// and the stage that produced it.
#[derive(Debug, Clone, PartialEq)]
pub struct NameMatch {
    /// The canonical (normalized) key that matched.
    pub key: String,
    /// Confidence score in `[0, 1]`.
    pub score: f64,
    /// One of `"exact"`, `"prefix"`, `"fuzzy"`, `"skeleton"`.
    pub method: &'static str,
}

impl NameMatch {
    /// Whether this is a high-confidence match: an `exact`/`prefix` hit, or
    /// a `fuzzy`/`skeleton` hit whose similarity clears [`FUZZY_HIGH`].
    #[must_use]
    pub fn is_high_confidence(&self) -> bool {
        matches!(self.method, "exact" | "prefix") || self.score >= FUZZY_HIGH
    }
}

/// An immutable index over a set of canonical keys, built once and queried
/// many times. Stores the normalized keys, a length-bucketed map for `±3`
/// candidate pruning, and a precomputed skeleton per key.
#[derive(Debug, Clone, Default)]
pub struct NameIndex {
    /// Normalized canonical keys (parallel to `skeletons`).
    keys: Vec<String>,
    /// Per-key OCR skeleton (parallel to `keys`).
    skeletons: Vec<String>,
    /// Normalized-key char length → indices into `keys`.
    by_len: HashMap<usize, Vec<usize>>,
    /// Normalized key → its index (exact-match fast path).
    exact: HashMap<String, usize>,
}

impl NameIndex {
    /// Build an index from an iterator of canonical keys. Keys are
    /// normalized on the way in; duplicates (after normalization) keep
    /// their first occurrence. Empty keys are skipped.
    pub fn new<I, S>(keys: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut idx = NameIndex::default();
        for raw in keys {
            let norm = normalize(raw.as_ref());
            if norm.is_empty() || idx.exact.contains_key(&norm) {
                continue;
            }
            let pos = idx.keys.len();
            let len = norm.chars().count();
            idx.by_len.entry(len).or_default().push(pos);
            idx.skeletons.push(skeleton(&norm));
            idx.exact.insert(norm.clone(), pos);
            idx.keys.push(norm);
        }
        idx
    }

    /// Number of indexed keys.
    #[must_use]
    pub fn len(&self) -> usize {
        self.keys.len()
    }

    /// Whether the index holds no keys.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    /// Candidate key indices whose normalized length is within `±LEN_WINDOW`
    /// of `len` (the cheap pruning step before scoring).
    fn candidates_near(&self, len: usize) -> Vec<usize> {
        let lo = len.saturating_sub(LEN_WINDOW);
        let hi = len + LEN_WINDOW;
        let mut out = Vec::new();
        for l in lo..=hi {
            if let Some(bucket) = self.by_len.get(&l) {
                out.extend_from_slice(bucket);
            }
        }
        out
    }

    /// Resolve `query` to its best canonical key, or `None` if nothing
    /// clears the acceptance thresholds.
    // Lengths cast to `f64` for the prefix score are short item names; the
    // cast cannot lose precision in practice.
    #[allow(clippy::cast_precision_loss)]
    #[must_use]
    pub fn resolve(&self, query: &str) -> Option<NameMatch> {
        // TODO(locales): a NameTranslator would run here first — map a
        // localized display name to its canonical English form before any
        // normalization/scoring. Locales are a separate follow-up; for now
        // resolution operates directly on the (folded) query.
        let q = normalize(&fold(query));
        if q.is_empty() {
            return None;
        }
        let q_len = q.chars().count();

        // 1) exact ---------------------------------------------------------
        if let Some(&pos) = self.exact.get(&q) {
            return Some(NameMatch {
                key: self.keys[pos].clone(),
                score: 1.0,
                method: "exact",
            });
        }

        // 2) prefix (only for reasonably long queries) ---------------------
        if q_len >= PREFIX_MIN_LEN {
            let mut best: Option<usize> = None;
            for (i, key) in self.keys.iter().enumerate() {
                if key.starts_with(&q) {
                    // Shortest qualifying key = least-ambiguous completion.
                    match best {
                        Some(b) if self.keys[b].len() <= key.len() => {}
                        _ => best = Some(i),
                    }
                }
            }
            if let Some(i) = best {
                let key = &self.keys[i];
                let score = (q_len as f64) / (key.chars().count() as f64);
                return Some(NameMatch {
                    key: key.clone(),
                    score,
                    method: "prefix",
                });
            }
        }

        // 3) fuzzy (Levenshtein over length-pruned candidates) -------------
        let mut best_fuzzy: Option<(usize, f64)> = None;
        for &i in &self.candidates_near(q_len) {
            let sim = similarity(&q, &self.keys[i]);
            if sim >= FUZZY_ACCEPT && best_fuzzy.is_none_or(|(_, s)| sim > s) {
                best_fuzzy = Some((i, sim));
            }
        }
        if let Some((i, sim)) = best_fuzzy {
            // The score *is* the similarity, so a caller can recover the
            // high-confidence band itself by comparing against `FUZZY_HIGH`
            // (see [`NameMatch::is_high_confidence`]).
            return Some(NameMatch {
                key: self.keys[i].clone(),
                score: sim,
                method: "fuzzy",
            });
        }

        // 4) skeleton (OCR-confusable collapse) ----------------------------
        let q_skel = skeleton(&q);
        let q_skel_len = q_skel.chars().count();
        let mut ranked: Vec<(usize, f64)> = Vec::new();
        for &i in &self.candidates_near(q_skel_len) {
            let sim = similarity(&q_skel, &self.skeletons[i]);
            ranked.push((i, sim));
        }
        ranked.sort_by(|a, b| b.1.total_cmp(&a.1));
        if let Some(&(best_i, best_sim)) = ranked.first() {
            let second = ranked.get(1).map_or(0.0, |&(_, s)| s);
            let accept = best_sim >= SKELETON_ACCEPT
                || (best_sim >= SKELETON_LOW_FLOOR && (best_sim - second) >= SKELETON_MARGIN);
            if accept {
                return Some(NameMatch {
                    key: self.keys[best_i].clone(),
                    score: best_sim,
                    method: "skeleton",
                });
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_index() -> NameIndex {
        NameIndex::new([
            "greater vision rune",
            "vision rune",
            "rebirth",
            "orb of transmutation",
            "perfect orb of transmutation",
            "mirror of kalandra",
            "exalted orb",
        ])
    }

    #[test]
    fn normalize_flattens_punctuation_and_case() {
        assert_eq!(
            normalize("  Greater   Vision-Rune!! "),
            "greater vision rune"
        );
        assert_eq!(normalize("ORB of Transmutation"), "orb of transmutation");
        assert_eq!(normalize("---"), "");
    }

    #[test]
    fn fold_strips_latin_diacritics_and_passes_others() {
        assert_eq!(fold("Mädchen"), "Madchen");
        assert_eq!(fold("naïve café"), "naive cafe");
        assert_eq!(fold("straße"), "strasse");
        assert_eq!(fold("Señor"), "Senor");
        // Non-Latin passes through untouched.
        assert_eq!(fold("日本語"), "日本語");
    }

    #[test]
    fn skeleton_collapses_confusable_classes() {
        // m/n/u/w -> n ; o/0/e/c -> o ; i/l/j/t/1 -> i ; r/v -> r ; 4/a -> a
        assert_eq!(skeleton("vision"), skeleton("rision"));
        assert_eq!(skeleton("o0ec"), "oooo");
        assert_eq!(skeleton("iljt1"), "iiiii");
    }

    #[test]
    fn levenshtein_matches_strsim() {
        assert_eq!(levenshtein("kitten", "sitting"), 3);
        assert_eq!(levenshtein("same", "same"), 0);
    }

    #[test]
    fn typo_resolves_to_vision_rune_via_fuzzy() {
        let idx = sample_index();
        let m = idx.resolve("greater viswn rune").expect("should resolve");
        assert_eq!(m.key, "greater vision rune");
        assert!(m.score >= FUZZY_ACCEPT, "score {} below floor", m.score);
        assert!(matches!(m.method, "fuzzy" | "skeleton"));
    }

    #[test]
    fn vision_does_not_resolve_to_rebirth() {
        let idx = sample_index();
        if let Some(m) = idx.resolve("vision") {
            assert_ne!(
                m.key, "rebirth",
                "vision must never collapse onto rebirth (got {m:?})"
            );
        }
    }

    #[test]
    fn exact_name_reports_exact_method() {
        let idx = sample_index();
        let m = idx.resolve("Mirror of Kalandra").expect("exact match");
        assert_eq!(m.key, "mirror of kalandra");
        assert_eq!(m.method, "exact");
        assert!((m.score - 1.0).abs() < 1e-9);
    }

    #[test]
    fn garbled_name_recovers_via_skeleton() {
        let idx = sample_index();
        // o/0, e/c, m/n/u confusions on "vision rune" with no clean
        // Levenshtein neighbour: forces the skeleton stage.
        let m = idx.resolve("visi0n rvne").expect("skeleton recovery");
        assert_eq!(m.key, "vision rune");
        assert_eq!(m.method, "skeleton");
    }

    #[test]
    fn prefix_truncation_resolves() {
        let idx = sample_index();
        // ≥10 chars and a unique completion → prefix stage.
        let m = idx
            .resolve("perfect orb of transmut")
            .expect("prefix match");
        assert_eq!(m.key, "perfect orb of transmutation");
        // "perfect orb of transmut" is not itself a key; should be prefix.
        let m2 = idx.resolve("mirror of k").expect("prefix match 2");
        assert_eq!(m2.key, "mirror of kalandra");
        assert_eq!(m2.method, "prefix");
    }

    #[test]
    fn unrelated_query_returns_none() {
        let idx = sample_index();
        assert!(idx
            .resolve("completely unrelated gibberish xyzzy")
            .is_none());
    }

    #[test]
    fn empty_query_returns_none() {
        let idx = sample_index();
        assert!(idx.resolve("   !!!  ").is_none());
    }
}
