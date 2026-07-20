# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [2.0.1](https://github.com/grok-insider/poc2/compare/v2.0.0...v2.0.1) - 2026-07-20

### Other

- updated the following local packages: poc2-engine, poc2-market, poc2-data, poc2-strategies, poc2-rules, poc2-probability

## [2.0.0](https://github.com/grok-insider/poc2/releases/tag/v2.0.0) - 2026-07-19

### Added

- *(advisor,wasm)* on-demand goal solving in the engine worker (ADR-0015)
- *(advisor)* training-quality pass — exact terminals, progress lanes, essences, risk blend (artefact schema v2)
- *(advisor)* analytic transition-model builder replaces Monte Carlo training
- trained Q-models + plugin phase 1 in the web engine
- [**breaking**] migrate to Next.js/WASM web app + Electron desktop + PoE2 0.5
- *(v3)* M16.4 + corpus expansion — trained-policy planner uplift, 51-goal corpus, Tauri loader
- *(v3)* Layer 2 — M15.1 strategies, M15.2 predicates, M15.3 rules, M15.4 cross-source CI
- *(v3)* Layer 1 data-substrate fixes + Layer 3 training infrastructure
- *(v2)* crafter helper v2 — Phases A-G + IPC/UI follow-ups
- *(advisor)* expand simulator omen resolver
- *(engine,advisor)* M2.9 — performance benchmark harness
- *(advisor,engine)* M4 — beam-search optimal-path advisor

### Other

- *(advisor)* pin Distilled Emotion candidates on live data
- *(plugin-host)* F.6-F.8 — PluginManager UI + integration tests + example plugin
- *(desktop)* C.2 — streaming recommendations via Tokio + Tauri events
- C.1 — Monte Carlo aggregator with prob_stderr
- migrate seed.rs to seed_rules/*.toml + add Suggestion::tag + 67 new rules
- *(advisor)* extend DSL with ActivateOmen / Recombine and richer Reveal
- *(strategies,rules,advisor)* thread PredicateContext through advisor + rule consumers
- M1 foundation — flake, workspace, Tauri/Svelte skeleton, CI, docs
