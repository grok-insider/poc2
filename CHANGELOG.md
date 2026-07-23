# Changelog

All notable changes to Path of Crafting 2.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/);
this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [2.0.2](https://github.com/grok-insider/poc2/compare/v2.0.1...v2.0.2) - 2026-07-23

### Added

- *(desktop)* GitHub Releases auto-updater (`electron-updater`) for packaged NSIS/AppImage installs — Settings + tray, user-confirm install; CI attaches `latest.yml` / `latest-linux.yml` feeds

### Fixed

- *(web,market)* expose Client language for reward-scan OCR

### Other

- *(ci)* full guard allowlist and admin manual-version-bump
- *(ci)* include data bundle in desktop packages

## [2.0.1] - 2026-07-20

- chore: release v2.0.1
- fix(web,market): expose Client language for reward-scan OCR
- fix(ci): include data bundle in desktop packages
- fix(ci): upload desktop installers even without latest*.yml feeds
- ci: add Package release workflow for attaching installers to a tag
- fix(ci): allow re-packaging a release tag via workflow_dispatch
- docs(changelog): generate release notes for v2.0.0
- chore: release v2.0.0

## [2.0.0] - 2026-07-19

- chore: release v2.0.0
- ci: re-trigger release-plz after 2.0.0 baseline
- fix(ci): set 2.0.0 as first public GitHub release baseline
- fix(desktop): use unscoped deb packageName for electron-builder
- fix(ci): rustfmt fetch-unique-icons and desktop deb package metadata
- feat(web,desktop): interactive regex overlay and shared item presentation
- feat(desktop): add native OCR market overlays
- Add Hyprland overlay plugin transport
- fix(wasm): no-op clearPluginDispatch must not wipe the trained-model warm start
- data: refresh upstream state (PoE2 4.5.4.1.2 → 4.5.4.3)
- feat(advisor,wasm): on-demand goal solving in the engine worker (ADR-0015)
- feat(advisor): training-quality pass — exact terminals, progress lanes, essences, risk blend (artefact schema v2)
- feat(advisor): analytic transition-model builder replaces Monte Carlo training
- docs: roadmap/README/AGENTS sync — plugin phase 2 + data-gap pools
- feat(web): Regex Waystone + Tablet tabs
- feat(pipeline,data): ship the 0.5 non-gear crafting pools
- feat: plugin phase 2 — live custom predicates in the web engine (ADR-0014)
- fix(plugin-sdk): arena I/O used absolute addresses as Vec indices
- docs: roadmap + README + AGENTS sync for the shipped roadmap batch
- ci(release): sync desktop version to the tag + dual-org gate
- refactor(web): unify screenshot OCR onto the vendored /ocr runtime
- fix(desktop,web): hydrate the persisted OCR region on overlay mount
- feat(market): price-id mappings for 0.5 alloys and emotions
- test(advisor): pin Distilled Emotion candidates on live data
- feat: trained Q-models + plugin phase 1 in the web engine
- feat(web): in-game search regex generator (Regex panel)
- docs: sync all docs to the shipped web/WASM + Electron 0.5 state
- ci(data-watch): repair previous-bundle cache restore
- fix: sync 0.5 defaults and web price/league plumbing
- feat(desktop): poe.ninja fallback for prices poe2scout doesn't carry
- feat(web): surface the poe2scout price cache in Settings + follow league
- fix(web): strip advanced-format roll ranges in gear price-check matcher
- feat(web): price the OCR overlay from the poe2scout cache
- feat(desktop): poe2scout price cache (node:sqlite) with hourly refresh
- fix(web): lower OCR overlay icon-crop to avoid clipping item names
- fix(desktop): overlay region propagation + self-capture during scan
- fix(web): pin non-SIMD Tesseract core for the OCR overlay
- feat(market): add locale name translation (de/fr/pt/ru/sp) for fuzzy matcher
- feat(web): Tesseract.js OCR scan and price-row overlay with fuzzy name resolution
- feat(desktop): region capture, capability gate, and price-overlay/calibration windows
- feat(market): add poe.ninja exchange price source resolved via fuzzy matcher
- feat(market): add clean-room fuzzy item-name matcher + WASM resolveName
- ci: adopt master/dev branch model + automated release-plz pipeline
- feat!: migrate to Next.js/WASM web app + Electron desktop + PoE2 0.5
- Refactor recovery hints invocation in RecoveryPanel, add client log status effect in SettingsPanel, enhance SimulationRunner with request ID handling, improve TargetPanel token validation, and extend tauri commands with database entry functionalities. Update types for rerollable mods and database entries, revise crafting rules and documentation for clarity, and enhance base icon fetching logic with improved filtering for inspectable bases.
- ui(desktop): refine v3 layout — trained-policy badge, dedupe success display, compact base panel, slim cost dock
- docs: correct trained_model_status invocation in AGENTS.md
- test(v3): smoke-load real production trained-model artefact
- fix(v3): pipeline class-id normalization + tag→class derivation + corpus tuning
- feat(v3): M16.6b/c — bundle-aware train-advisor + corpus audit + DX fixes
- feat(v3): M16.4 + corpus expansion — trained-policy planner uplift, 51-goal corpus, Tauri loader
- feat(v3): M14.7e — CoE alias suggester subcommand
- feat(v3): M14.7a–d — bundle schema bump v2 + migration UX + trade scraper
- feat(v3): M16.6 — training corpus + train-advisor binary
- feat(v3): Layer 2 — M15.1 strategies, M15.2 predicates, M15.3 rules, M15.4 cross-source CI
- feat(v3): Layer 1 data-substrate fixes + Layer 3 training infrastructure
- Implement feature X to enhance user experience and optimize performance
- feat(v2): crafter helper v2 — Phases A-G + IPC/UI follow-ups
- release: G.2 — README + CHANGELOG + roadmap update for v1.0
- perf: G.1 — re-bench post-Phase-F + document memoization defer
- desktop(plugin-host): F.6-F.8 — PluginManager UI + integration tests + example plugin
- plugin-host(plugin-sdk,strategies,advisor): F.1-F.5 — Wasm plugin runtime + custom predicates
- market(desktop): Phase E — poe.ninja meta aggregator + off-meta finder
- desktop: D.3 — trade-search URL adapter + AdvisorPanel button
- docs(d.2): ADR-0009 — defer Wayland layer-shell to v1.1; ship Hyprland rules instead
- desktop: D.1 — Client.txt watcher with notify + Tauri events
- desktop(ui): C.3 — SimulationRunner.svelte + run_n_trials command
- advisor(desktop): C.2 — streaming recommendations via Tokio + Tauri events
- advisor: C.1 — Monte Carlo aggregator with prob_stderr
- desktop(ui): B.4 — RecipeLibrary.svelte + recipe CRUD commands
- desktop(ui): B.3 — SettingsPanel.svelte + list_leagues command
- desktop(ui): B.2 — RecoveryPanel.svelte + recovery_hints command
- desktop(ui): B.1 — TargetPanel.svelte + persisted Goal state
- docs: A.7 — author docs/32 (probability) + 36 (decision engine) + 37 (recovery flows) + 51 (market meta)
- desktop: hot-swap bundle without app restart
- rules: migrate seed.rs to seed_rules/*.toml + add Suggestion::tag + 67 new rules
- strategies: encode #19 #21 #22 #23 (cross-cutting + Belton's Four-T1 Rubric)
- strategies: encode #16 #17 #20 (Recombinator + Magic exit + Mark of Abyss)
- strategies: encode #12 #13 (Vaal corruption + double corruption)
- strategies: encode #9 #10 #14 #15 (omen/lock/bone tactics)
- strategies: encode #5 #6 #11 (essence/exalt/sanctification finishers)
- pipeline(coe): four-tier CoE→engine join with alias table + diagnose subcommand
- strategies(advisor): extend DSL with ActivateOmen / Recombine and richer Reveal
- engine(strategies,rules,advisor): thread PredicateContext through advisor + rule consumers
- docs: M2-M8 v1 execution plan
- docs(roadmap): mark M2/M3/M5 fully shipped; refine remaining work
- feat(strategies,rules): expand seed catalogues to 8 strategies / 45 rules
- feat(desktop,data): live prices + bundle essence/catalyst catalogues
- feat(pipeline,market): live data sources — poe2scout, CoE, poe2db
- docs: add 35-advisor-architecture.md and 41-ui-flows.md
- docs(roadmap): mark M2-M7 progress; defer pipeline scrapes to M5
- feat(advisor): expand simulator omen resolver
- feat(desktop): expand strategy loader
- feat(strategies,rules): expand seed catalogues
- feat(engine): wire Catalysts into the currency resolver
- feat(engine): M2.5 — Catalysts + Recombinator
- feat(engine,advisor): M2.9 — performance benchmark harness
- feat(desktop): bundle loading on startup
- feat(parser,desktop): M7 — PoE2 clipboard item-text parser
- feat(desktop): M6 — Tauri IPC + Svelte advisor panel
- feat(advisor,engine): M4 — beam-search optimal-path advisor
- feat(rules,market): M3.d rule engine + M5.2 valuator
- feat(strategies,probability): M3.c + M5.1 — predicate eval, executor, prob primitives
- feat(strategies): M3 — strategy DSL + canonical worked-example fixture
- docs: M2.10 — engine reference docs (game mechanics, domain, algorithms, 0.4)
- feat(engine,pipeline): M2.7 — concept-based mod analyzer (hybrid classification)
- test(engine): integration test for the user's Triple T1 ES body armour craft
- feat(engine): M2.5 — Essences (Lesser/Normal/Greater/Perfect/Corrupted)
- feat(engine): M2.6 — Omen system + integration with Exalt/Annul/Chaos/Bone
- feat(engine): M2.5 — Hinekora's Lock + apply/preview/commit orchestration
- feat(engine): M2.5 — Bones + Well-of-Souls reveal
- feat(engine): M2.5 — Fracturing Orb (the user's 'checkpoint' mechanic)
- feat(engine): M2.4e — Greater + Perfect variants of Transmute/Aug/Regal/Exalt/Chaos
- feat(engine): M2.4d — Divine Orb + Vaal Orb
- feat(engine): M2.4c — Alchemy / Exalt / Chaos / Annul
- feat(engine): M2.4a+b — ModRegistry + Currency trait + Transmute/Augment/Regal
- feat(pipeline): M2.3 — RePoE-fork data pull + bundle build CLI
- feat(data): M2.2 — bundle schema, validation, JSON+gzip I/O
- feat(engine): M2.1 — domain types (Item, BaseType, ModDefinition, ids, tags)
- chore: M1 foundation — flake, workspace, Tauri/Svelte skeleton, CI, docs

## [Unreleased] — crafting-mechanics fidelity + PoE2 0.5

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
