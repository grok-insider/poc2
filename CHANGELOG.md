# Changelog

All notable changes to Path of Crafting 2.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/);
this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.0.0] — 2026-04 (release)

First public release. NixOS + Hyprland only per ADR-0002 + ADR-0009.

### Added — engine + data

- **Engine core** with sub-µs `apply_currency`. 15 currencies +
  Greater/Perfect tiers, 22 omens, fracturing orb, Hinekora's Lock
  (preview/commit byte-equality), bones + reveal, catalysts,
  recombinator, full hybrid-mod handling.
- **Data pipeline** (poc2-pipeline) producing 211 KB gzipped bundles
  from RePoE-fork + Craft of Exile + poe2db.tw. ≥80% CoE→engine
  mod-id join via the four-tier strategy (alias / essence-xref /
  name-substring / template-tokens) — see Phase A.3.
- **23 codified strategies** (full coverage of /docs/33), each
  shipping as TOML with preconditions, target spec, step graph,
  abandon criteria, and recovery hints.
- **113 production rules** across 14 sections (full coverage of
  /docs/34), TOML-driven via `crates/rules/seed_rules/`.

### Added — advisor

- **Beam-search optimal-path planner** with configurable
  `(width, depth, top_n, risk, mc_samples)` (Phase M4).
- **Monte Carlo aggregator** with `prob_stderr` confidence bands
  (Phase C.1; default 50 samples per candidate; depth-3 perf 139 µs
  vs 5 ms budget).
- **Streaming recommendations** at depth 1 → 3 → final via
  Tokio + Tauri events; cancellable on new requests (Phase C.2).
- **PredicateContext** threading: cost-aware, stash-aware,
  valuator-aware, sale-price-aware, plugin-aware predicates fire
  mid-plan (Phases A.1 + F.3).

### Added — UI (Tauri 2 + Svelte 5)

- **Item builder + clipboard import** (M6 / M7).
- **Target panel** (Phase B.1) with concept picker, hybrid toggle,
  budget triple editor; persists to `~/.config/poc2/state.toml`.
- **Recovery panel** (Phase B.2) surfacing strategy-step recovery
  hints when `lastFailed=true`.
- **Settings panel** (Phase B.3) with bundle hot-swap, league
  dropdown (poe2scout `/Leagues`), prices auto-refresh, Client.txt
  watcher, plugin manager, off-meta crafting hints.
- **Recipe library** (Phase B.4) save/load/share via
  `~/.config/poc2/recipes/<name>.toml`.
- **Simulation runner** (Phase C.3) with inline-SVG histogram of
  the change-count distribution + per-trial cost.

### Added — live integration

- **Client.txt watcher** (Phase D.1) via `notify` crate;
  area / player / death / whisper events emitted on
  `client-log://event`.
- **Hyprland always-on-top** (Phase D.2 / ADR-0009) via
  windowrulev2 recipes; example configs in `examples/hyprland/`.
- **Trade URL search** (Phase D.3) builds
  `pathofexile.com/trade2/search/...` deep-links from the current
  item state and opens via `tauri-plugin-shell`.

### Added — market

- **poe2scout live price feed** (M5.3) with conservative defaults +
  per-currency band overrides + UI refresh button.
- **poe.ninja meta-build aggregator** (Phase E.1) with permissive
  deserializer; soft-fails on endpoint absence.
- **Off-meta niche finder** (Phase E.2) ranks concepts by
  `demand_share / sqrt(competition + 1)`; surfaced in Settings as a
  "What to craft right now" card.

### Added — Wasm Plugin SDK (Phase F)

- **`poc2-plugin-host` crate** with wasmtime engine + capability
  gating + per-plugin sandboxing (fuel budget) + predicate dispatch
  cache (4096-entry LRU).
- **`poc2-plugin-sdk` crate** with `declare_predicate!`,
  `declare_strategies!`, `declare_rules!`,
  `declare_recommendation_emitter!` macros.
- **7 capabilities** (read_engine / read_market / read_advisor_state /
  register_predicate / emit_strategies / emit_rules /
  emit_recommendations) declared per plugin in `poc2-plugin.toml`.
- **`ItemPredicate::Custom`** variant referenceable from any rule
  or strategy TOML; dispatch routed through the host's
  `PluginPredicateDispatch` trait.
- **Example plugin** (`examples/plugins/predicate-ilvl-min/`)
  demonstrates the `declare_predicate!` macro end-to-end.
- **Plugin Manager UI** lists every loaded plugin with id, version,
  capabilities, strategy/rule counts; reload-from-disk button.

### Added — documentation

- 9 ADRs (`docs/adr/0001` through `docs/adr/0009`).
- 14 architecture / mechanics / strategy / rules / UI / market /
  recovery / probability / decision-engine docs.
- `examples/hyprland/` + `examples/plugins/` with working
  configs / source.

### Performance (verified by `cargo bench --bench advisor_plan`)

| Operation | Time | Budget | Margin |
|---|---|---|---|
| `plan_depth_1_top_3` | 46 µs | 1 ms | ×21 |
| `plan_depth_3_top_3` | 46 µs | 50 ms | ×1086 |
| `plan_depth_3_top_3_mc50` | 139 µs | 5 ms | ×35 |
| `plan_depth_5_width_8` | 151 µs | 500 ms | ×3311 |

### Removed

- Phase G.1's planned beam-search memoization (was: canonicalize
  Item by tier-set). Measured numbers showed no need at v1 scale;
  deferred to v1.x as an "if needed" optimization.

### Tests

- 317 workspace tests passing across 11 crates + the desktop app.
- Canonical rediscovery test (`crates/advisor/tests/canonical_rediscovery.rs`)
  asserts the advisor's top-N includes Perfect Transmute traceable
  to rule R001 or strategy `3xt1-es-body-armour-isolation` step S2.
- 11 plugin-host integration tests verifying load + dispatch +
  cache + capability-gate + perf budget.
