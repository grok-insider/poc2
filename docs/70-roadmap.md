# Roadmap

> Phased build plan. M1-M8 = v1.0. M9+ = post-v1.

## M1 — Foundation ✅

- ✅ Nix flake with Rust + Node + Wayland deps
- ✅ Cargo workspace with 8 crates + pipeline
- ✅ Tauri 2 + Svelte 5 desktop app skeleton
- ✅ GitHub Actions CI (rust + flake + frontend)
- ✅ All 9 reference repos cloned to `example-repos/`
- ✅ Foundation docs (overview, architecture, ADRs 0001-0008)

## M2 — Engine core + data pipeline ✅ (mostly)

### Pipeline
- ✅ Bundle schema design → `crates/data/src/bundle.rs`
- ✅ RePoE-fork JSON pull (mods, base_items, mods_by_base, tags)
- ✅ Normalizer with cross-validation + concept classification
- ✅ Pipeline CLI: `cargo run -p poc2-pipeline -- build --out *.bundle.json.gz`
- ✅ First end-to-end bundle for 0.4 (2740 bases, 2123 mods, 168KB gz)
- [ ] Craft of Exile `poec_data.json` pull (weights) — **deferred to M5**
- [ ] poe2db.tw scrape (omens, essences, bones, catalysts) — **deferred** until upstream RePoE-fork exposes the JSON or we add a static catalogue
- [ ] GGG `/trade/data/stats` cached pull (post-OAuth approval)
- [ ] Bundle hot-swap mechanism in app

### Engine
- ✅ Domain types (`Item`, `ModRoll`, `ModDefinition`, `BaseType`)
- ✅ Patch versioning applied to every entity
- ✅ Mod analyzer (concept-based hybrid classification)
- ✅ Basic currencies (Transmute → Vaal + Greater/Perfect variants)
- ✅ Essences (Lesser/Normal/Greater/Perfect/Corrupted, prefix/suffix forced-removal via Crystallisation)
- ✅ Omens (22 omen presets + `OmenSet::consume` patch-range honoring)
- ✅ Fracturing Orb (4-mod requirement, hidden-desecrated awareness)
- ✅ Hinekora's Lock (preview/commit byte-equality under same seed)
- ✅ Bones + Well of Souls reveal
- ✅ Catalysts (tagged-quality jewelry/jewel currency)
- ✅ Recombinator (two-input combine with mod-group exclusivity + fracture preservation)
- ✅ Currency resolver (`DefaultCurrencyResolver`)
- ✅ Performance pass — sub-microsecond `apply()` (244-563 ns per op)
- ✅ Unit tests for hybrid handling, fracture eligibility, mod-group exclusivity (118 tests)
- ✅ Author `docs/11-game-mechanics.md`, `docs/30-domain-model.md`, `docs/31-engine-algorithms.md`
- [ ] Synergy graph — **skipped** (synergy is implicit via currency apply paths)

## M3 — Strategy + Rule layers ✅ (seed)

### Strategies
- ✅ Strategy DSL design (TOML schema)
- ✅ Strategy loader + registry + executor + predicate evaluator
- ✅ 3 seed strategies (3xT1 ES Body Armour, Apprentice Blueprint, Whittling Cleanup)
- [ ] Encode the remaining 20 strategies from `docs/33-strategy-library.md` as TOML
- [ ] Author `docs/37-recovery-flows.md` (3-deep recovery encoding)

### Rules
- ✅ Rule DSL design + forward-chain engine
- ✅ 25 seed rules covering rarity progression, fracture, recovery,
  Vaal, bones, catalysts, erasure, sanctification
- [ ] Encode the remaining ~95 rules from `docs/34-heuristics-rulebook.md` as TOML
- [ ] Editorial pass on community-attributed rules

## M4 — Advisor / Planner ✅

- ✅ Beam-search planner over Strategy + Rule + Heuristic candidates
- ✅ Risk slider integration (cautious ↔ greedy)
- ✅ Recovery branch detection (strategy-step-attached hints)
- ✅ Explanation: every recommendation cites firing rule/strategy + EV math
- ✅ **Critical test**: canonical "Triple T1 ES" rediscovery test —
  advisor's top recommendation for the user's worked-example state is
  Perfect Transmute, traceable to either rule R001 or strategy S2
- ✅ Performance: depth-3 plan in 3.08 µs (5 orders of magnitude under
  ADR-0007's 2s budget)
- [ ] Monte Carlo evaluator over the candidate set (M5+ refinement)
- [ ] Streaming results to UI via Tokio channels (M6+ polish)
- [ ] Author `docs/35-advisor-architecture.md`, `docs/36-decision-engine.md`

## M5 — Probability + Market ✅ (probability primitives + valuator)

- ✅ Monte Carlo lib (`run_n_trials`, `run_until_success`)
- ✅ Geometric distribution cost calculator
- ✅ Currency valuator (`DivEquiv(min, expected, max)` triples)
- ✅ Conservative default prices (1div=50-180ex, 1div=3-30chaos, 1mirror=1500-6000div)
- [ ] poe2scout / poe.ninja PoE2 price pollers (M5.3 — network)
- [ ] Meta-build aggregator (poe.ninja PoE2 builds) (M5.4)
- [ ] Off-meta niche finder
- [ ] Author `docs/32-probability-math.md`, `docs/51-market-meta.md`

## M6 — UI v1 ✅ (skeleton)

- ✅ Tauri IPC: `recommend(args)` returns Vec<Recommendation>
- ✅ Bundle loading on startup ($POC2_BUNDLE / $XDG_CONFIG_HOME / $XDG_DATA_HOME)
- ✅ User-strategy loading from $XDG_CONFIG_HOME/poc2/strategies/
- ✅ Item builder (rarity, ilvl, base, slot summary, flags)
- ✅ Advisor panel (top-N + risk slider + depth slider + live re-plan)
- ✅ Clipboard import button + manual paste textarea
- ✅ Pings + meta strip (patch / rule_count / strategy_count / mod_count)
- [ ] Target panel (mod-concept selector with weights) — UI design pending
- [ ] Recovery panel (visible only when last action failed)
- [ ] Simulation runner (run-N-trials chart)
- [ ] Recipe library (save/load/share)
- [ ] Settings (data-bundle update, price source, risk slider persistence)
- [ ] Author `docs/41-ui-flows.md`

## M7 — Live integration ✅ (clipboard)

- ✅ Clipboard parser (PoE2 in-game text → ParsedItem → engine::Item)
- ✅ Tauri `read_clipboard_item` + `parse_item_text` commands
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
