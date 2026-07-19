# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [2.0.0](https://github.com/grok-insider/poc2/releases/tag/v2.0.0) - 2026-07-19

### Added

- *(market)* price-id mappings for 0.5 alloys and emotions
- *(market)* add locale name translation (de/fr/pt/ru/sp) for fuzzy matcher
- *(market)* add poe.ninja exchange price source resolved via fuzzy matcher
- *(market)* add clean-room fuzzy item-name matcher + WASM resolveName
- [**breaking**] migrate to Next.js/WASM web app + Electron desktop + PoE2 0.5
- *(v2)* crafter helper v2 — Phases A-G + IPC/UI follow-ups
- *(pipeline,market)* live data sources — poe2scout, CoE, poe2db
- *(advisor,engine)* M4 — beam-search optimal-path advisor
- *(rules,market)* M3.d rule engine + M5.2 valuator

### Fixed

- sync 0.5 defaults and web price/league plumbing

### Other

- *(desktop)* Phase E — poe.ninja meta aggregator + off-meta finder
- M1 foundation — flake, workspace, Tauri/Svelte skeleton, CI, docs
