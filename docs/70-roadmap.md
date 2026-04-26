# Roadmap

> Phased build plan. M1-M8 = v1.0. M9+ = post-v1.

## M1 — Foundation ✅

- ✅ Nix flake with Rust + Node + Wayland deps
- ✅ Cargo workspace with 7 crates + pipeline
- ✅ Tauri 2 + Svelte 5 desktop app skeleton
- ✅ GitHub Actions CI (rust + flake + frontend)
- ✅ All 9 reference repos cloned to `example-repos/`
- ✅ Foundation docs (overview, architecture, ADRs 0001-0008)

## M2 — Engine core + data pipeline (3-4 weeks)

### Pipeline (separate sub-track)
- [ ] Bundle schema design → `docs/21-bundle-schema.json`
- [ ] RePoE-fork JSON pull (mods, base_items, mods_by_base, tags, stat_translations)
- [ ] Craft of Exile `poec_data.json` pull (weights)
- [ ] poe2db.tw scrape (omens, essences, bones, catalysts, currency)
- [ ] GGG `/trade/data/stats` cached pull (post-OAuth approval)
- [ ] Normalizer with cross-validation + confidence flagging
- [ ] Bundle hot-swap mechanism in app
- [ ] First end-to-end bundle for 0.4

### Engine
- [ ] Domain types (`Item`, `ModRoll`, `ModDefinition`, `BaseType`)
- [ ] Patch versioning applied to every entity
- [ ] Mod analyzer (concept-based hybrid classification)
- [ ] Synergy graph (auto-derive + override)
- [ ] Basic currencies (Transmute → Vaal + Greater/Perfect variants)
- [ ] Essences (4 tiers × 19 types, prefix/suffix forced-removal)
- [ ] Omens (every crafting omen + 0.4 disabled-flag system)
- [ ] Catalysts, Fracturing Orb, Hinekora's Lock, Bones, Recombinator
- [ ] Performance pass — sub-ms `apply()` target
- [ ] Unit tests for hybrid handling, fracture eligibility, mod-group exclusivity
- [ ] Author `docs/11-game-mechanics.md`, `docs/30-domain-model.md`, `docs/31-engine-algorithms.md`

## M3 — Strategy + Rule layers (3 weeks)

### Strategies
- [ ] Strategy DSL design (TOML schema)
- [ ] Strategy loader + registry
- [ ] Encode 23 strategies from `docs/33-strategy-library.md` as TOML
- [ ] Encode the canonical "Triple T1 ES Body Armour Isolation" fixture
- [ ] Author `docs/37-recovery-flows.md` (3-deep recovery encoding)

### Rules
- [ ] Rule DSL design
- [ ] Forward-chain engine
- [ ] Encode ~120 rules from `docs/34-heuristics-rulebook.md` as TOML
- [ ] Editorial pass on community-attributed rules

## M4 — Advisor / Planner (3 weeks)

- [ ] Beam-search planner over Strategy + Rule candidates
- [ ] Monte Carlo evaluator
- [ ] Streaming results to UI via Tokio channels
- [ ] Recovery branch detection
- [ ] Risk slider integration
- [ ] Explanation: every recommendation cites firing rule/strategy + EV math
- [ ] **Critical test**: "Triple T1 ES" rediscovery test (advisor produces user's strategy or strictly-better)
- [ ] Author `docs/35-advisor-architecture.md`, `docs/36-decision-engine.md`

## M5 — Probability + Market (2 weeks)

- [ ] Monte Carlo lib + geometric distribution cost calculator
- [ ] Confidence intervals on weight-derived probabilities
- [ ] Currency valuator (`DivEquiv(min, expected, max)` triples)
- [ ] poe2scout / poe.ninja PoE2 price pollers
- [ ] Meta-build aggregator (poe.ninja PoE2 builds)
- [ ] Off-meta niche finder
- [ ] Author `docs/32-probability-math.md`, `docs/51-market-meta.md`

## M6 — UI v1 (4 weeks)

- [ ] Item builder (base picker, ilvl, mod state)
- [ ] Target panel (mod-concept selector with weights)
- [ ] Advisor panel (top-N recommendations + explanations + EV math)
- [ ] Recovery panel (visible only when last action failed)
- [ ] Simulation runner (run-N-trials chart)
- [ ] Recipe library (save/load/share)
- [ ] Settings (data-bundle update, price source, risk slider)
- [ ] Author `docs/41-ui-flows.md`

## M7 — Live integration (3 weeks)

- [ ] Clipboard parser (`wl-clipboard` via Tauri plugin) → `Item` struct
- [ ] `Client.txt` watcher (`inotify` on Wine prefix path)
- [ ] Wayland layer-shell overlay (gtk4-layer-shell or smithay-client-toolkit)
- [ ] Hyprland window rules + always-on-top behavior
- [ ] GGG `/trade2` OAuth integration (register early)
- [ ] Trade-search-by-current-item flow
- [ ] Author `docs/50-trade-integration.md`

## M8 — Polish + release (2 weeks)

- [ ] Performance pass (100k+ simulations/sec target)
- [ ] Auto-update with signature verification
- [ ] Cachix binary cache for fast Nix installs
- [ ] Public README with demo recording
- [ ] v1.0 release tag

## M9+ Post-v1

- [ ] Wasm plugin SDK (M9-M10)
- [ ] Strategy / rule plugins
- [ ] Hardcore + SSF support
- [ ] Cross-platform (Windows, macOS) — v2 scope decision
- [ ] Self-hosting pipeline (no external data sources)
- [ ] Empirical weight derivation from trade samples
- [ ] MCTS upgrade for the advisor
