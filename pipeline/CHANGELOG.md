# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [2.0.2](https://github.com/grok-insider/poc2/compare/v2.0.1...v2.0.2) - 2026-07-23

### Other

- workspace release with desktop auto-updater

## [Unreleased]

## [2.0.1](https://github.com/grok-insider/poc2/compare/v2.0.0...v2.0.1) - 2026-07-20

### Other

- updated the following local packages: poc2-engine, poc2-market, poc2-data, poc2-strategies, poc2-rules, poc2-advisor

## [2.0.0](https://github.com/grok-insider/poc2/releases/tag/v2.0.0) - 2026-07-19

### Added

- *(web,desktop)* interactive regex overlay and shared item presentation
- *(advisor,wasm)* on-demand goal solving in the engine worker (ADR-0015)
- *(advisor)* training-quality pass — exact terminals, progress lanes, essences, risk blend (artefact schema v2)
- *(advisor)* analytic transition-model builder replaces Monte Carlo training
- *(pipeline,data)* ship the 0.5 non-gear crafting pools
- [**breaking**] migrate to Next.js/WASM web app + Electron desktop + PoE2 0.5
- *(v3)* M16.6b/c — bundle-aware train-advisor + corpus audit + DX fixes
- *(v3)* M16.4 + corpus expansion — trained-policy planner uplift, 51-goal corpus, Tauri loader
- *(v3)* M14.7e — CoE alias suggester subcommand
- *(v3)* M14.7a–d — bundle schema bump v2 + migration UX + trade scraper
- *(v3)* M16.6 — training corpus + train-advisor binary
- *(v2)* crafter helper v2 — Phases A-G + IPC/UI follow-ups
- *(desktop,data)* live prices + bundle essence/catalyst catalogues
- *(pipeline,market)* live data sources — poe2scout, CoE, poe2db
- *(engine,pipeline)* M2.7 — concept-based mod analyzer (hybrid classification)
- *(pipeline)* M2.3 — RePoE-fork data pull + bundle build CLI

### Fixed

- *(ci)* rustfmt fetch-unique-icons and desktop deb package metadata
- sync 0.5 defaults and web price/league plumbing
- *(v3)* pipeline class-id normalization + tag→class derivation + corpus tuning

### Other

- refresh upstream state (PoE2 4.5.4.1.2 → 4.5.4.3)
- sync all docs to the shipped web/WASM + Electron 0.5 state
- Refactor recovery hints invocation in RecoveryPanel, add client log status effect in SettingsPanel, enhance SimulationRunner with request ID handling, improve TargetPanel token validation, and extend tauri commands with database entry functionalities. Update types for rerollable mods and database entries, revise crafting rules and documentation for clarity, and enhance base icon fetching logic with improved filtering for inspectable bases.
- *(v3)* smoke-load real production trained-model artefact
- *(coe)* four-tier CoE→engine join with alias table + diagnose subcommand
- M1 foundation — flake, workspace, Tauri/Svelte skeleton, CI, docs
