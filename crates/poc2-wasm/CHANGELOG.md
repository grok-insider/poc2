# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [2.0.0](https://github.com/grok-insider/poc2/releases/tag/v2.0.0) - 2026-07-19

### Added

- *(desktop)* add native OCR market overlays
- *(advisor,wasm)* on-demand goal solving in the engine worker (ADR-0015)
- *(advisor)* training-quality pass — exact terminals, progress lanes, essences, risk blend (artefact schema v2)
- *(pipeline,data)* ship the 0.5 non-gear crafting pools
- plugin phase 2 — live custom predicates in the web engine (ADR-0014)
- trained Q-models + plugin phase 1 in the web engine
- *(market)* add locale name translation (de/fr/pt/ru/sp) for fuzzy matcher
- *(market)* add poe.ninja exchange price source resolved via fuzzy matcher
- *(market)* add clean-room fuzzy item-name matcher + WASM resolveName
- [**breaking**] migrate to Next.js/WASM web app + Electron desktop + PoE2 0.5

### Fixed

- *(wasm)* no-op clearPluginDispatch must not wipe the trained-model warm start
