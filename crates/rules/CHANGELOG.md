# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [2.0.0](https://github.com/grok-insider/poc2/releases/tag/v2.0.0) - 2026-07-19

### Added

- [**breaking**] migrate to Next.js/WASM web app + Electron desktop + PoE2 0.5
- *(v3)* Layer 2 — M15.1 strategies, M15.2 predicates, M15.3 rules, M15.4 cross-source CI
- *(v3)* Layer 1 data-substrate fixes + Layer 3 training infrastructure
- *(strategies,rules)* expand seed catalogues to 8 strategies / 45 rules
- *(strategies,rules)* expand seed catalogues
- *(advisor,engine)* M4 — beam-search optimal-path advisor
- *(rules,market)* M3.d rule engine + M5.2 valuator

### Other

- Refactor recovery hints invocation in RecoveryPanel, add client log status effect in SettingsPanel, enhance SimulationRunner with request ID handling, improve TargetPanel token validation, and extend tauri commands with database entry functionalities. Update types for rerollable mods and database entries, revise crafting rules and documentation for clarity, and enhance base icon fetching logic with improved filtering for inspectable bases.
- *(plugin-sdk,strategies,advisor)* F.1-F.5 — Wasm plugin runtime + custom predicates
- migrate seed.rs to seed_rules/*.toml + add Suggestion::tag + 67 new rules
- *(strategies,rules,advisor)* thread PredicateContext through advisor + rule consumers
- M1 foundation — flake, workspace, Tauri/Svelte skeleton, CI, docs
