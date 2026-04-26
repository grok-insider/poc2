# ADR-0004 — Mod weight sourcing strategy

- Status: Accepted
- Date: 2026-04-26

## Context

GGG removed mod spawn weights from PoE2 client data files. RePoE-fork's `mods.json` `spawn_weights` field is mostly `1`/`0` (tag-eligibility flags), not real spawn weights. The only numerical weight data in circulation is empirically reverse-engineered:

- **Craft of Exile** `poec_data.json` — primary community source, used by most tools.
- **poe2db.tw** `/Modifiers` page — has a "Weight" column derived from community datasets.
- **Krakenbul / Prohibited Library Discord** — original recombinator-derived spreadsheet (manual export).

Without weights, we cannot compute spawn probabilities, expected costs, or rank strategies by EV. **This is the single biggest data risk on the project.**

## Decision

Use **Craft of Exile as primary**, **poe2db.tw as cross-check**, and surface confidence in the UI.

```
weight_for(mod, base, ilvl) -> WeightObservation {
    primary:    f64,            // CoE
    secondary:  Option<f64>,    // poe2db
    confidence: Confidence,     // Verified | Community | Experimental
}
```

Confidence rules:
- Both within ±5% → `Verified`
- Both within ±25% → `Community`
- >25% disagreement OR only one source → `Experimental`

The advisor uses `primary` for ranking but widens its confidence interval based on the `confidence` field. UI shows "T1 chance: 12% ± 4% (Community)".

## Long-term: empirical weight derivation

In v2, we plan to derive our own weights via trade-site sampling:

1. Scrape live trade results for an item class.
2. Histogram the observed mod frequencies.
3. Bayesian-update the published weights against observation.
4. Publish refined weights with confidence intervals.

This is a research project; not in v1 scope.

## Failure modes

- Craft of Exile changes their data shape → pipeline test fails, we revert to poe2db-only.
- poe2db.tw deprecates the Weight column → we still have CoE; confidence drops everywhere.
- A patch significantly changes weights and neither source updates → users complain; we manually refresh from Krakenbul spreadsheet.

## Implications

- Bundle schema includes `weights[]` as a separate top-level array, not embedded in `mods[]`. This lets us update weights without touching mod definitions.
- Every recommendation in the advisor UI carries a confidence badge.
- Strategies that work in extreme weight regimes (e.g., relying on a 0.5%-weight mod) are flagged as "high-variance".

## Trust ranking (community-derived)

| Source | Trust for v1 |
|---|---|
| CoE poec_data | High — used by virtually every PoE2 tool |
| poe2db.tw | Medium-high — community-maintained, mostly fresh |
| Krakenbul spreadsheet | High but manual — used as ground truth when sources disagree |
| RePoE-fork weights | Low — placeholders, do not use |
| Self-derived (v2) | TBD — research |
