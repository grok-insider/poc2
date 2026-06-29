# Path of Crafting 2 (`poc2`)

A crafting **advisor** for Path of Exile 2 — beam-search optimal-path
planning with full re-plan on every state change, branching recovery
flows, live market awareness, and a Wasm plugin SDK for community
extensions.

The advisor is a **Rust engine that runs entirely in your browser**: a
Next.js 16 + React 19 web app (`apps/web`) hosts the engine compiled to
**WebAssembly** (`crates/poc2-wasm`) inside a Web Worker. No server, no
install — every recommendation is deterministic client-side compute over a
static data bundle. (The previous Tauri + Svelte desktop app has been
replaced by this web app.)

A **desktop app** (`apps/desktop`, Electron — [ADR-0010](docs/adr/0010-desktop-shell-electron-cross-platform.md))
wraps the same web app like Discord wraps a website, and adds what a
browser can't do: **item capture** (hotkey → the game's own Ctrl+C →
instant import) and **price checking** against the official PoE2 trade
API (rate-limited, proxied through the main process). Targets: Linux,
NixOS, and Windows 11.

> Status: **v1.0 shipped + post-v1 iterations on `main`.** v1.0 covered all
> 8 milestones (M1-M8) + Phases A-G ([`docs/72-v1-execution-plan.md`](docs/72-v1-execution-plan.md)).
> Since then the **v2** crafter-helper pass ([`docs/80`](docs/80-crafter-helper-v2-plan.md))
> and **v3** engine-training pass ([`docs/81`](docs/81-engine-training-and-rule-encoding-plan.md))
> shipped, and a **crafting-mechanics fidelity + PoE2 0.5 "Return of the
> Ancients"** pass is in progress ([`docs/83`](docs/83-crafting-fidelity-plan.md),
> [`docs/14`](docs/14-crafting-mechanics-cross-version.md)). The engine now
> models item-level-dependent pools, inclusive higher-tier weighting,
> patch-versioned Minimum-Modifier-Level floors, and cross-version (0.3/0.4/0.5)
> gating. See [`CHANGELOG.md`](CHANGELOG.md).

## What it does

- **Import an in-game item** by copying it in-game and pasting into the
  Item panel (`navigator.clipboard` → Rust parser → engine state), or
  build it manually in the UI. In the desktop app, press the **capture
  hotkey** (`Ctrl+Shift+D`) while hovering an item in PoE2 and it imports
  itself — same mechanism as Awakened PoE Trade.
- **Price-check it**: the Price Check panel matches the item's mod lines
  to official trade-API stat ids (1,932-entry pipeline-generated table),
  builds a real trade2 query (toggleable stat filters, min bounds at 90%
  of the roll), and — in the desktop app — shows live listings with
  cheapest/median; browsers open the identical query on the trade site.
- **Declare your target mods + budget** via the Target panel
  (concept-aware, hybrid-mod aware).
- The advisor **proposes the optimal next currency / omen** to use,
  with explanation, EV math, confidence band (Monte Carlo ± stderr),
  and full traceability back to the source rule / strategy /
  heuristic.
- You apply it; the app sees the new state via clipboard and
  **re-plans automatically** with progressively-deeper streaming
  results (depth 1 → 3 → final).
- If the outcome was a failure (bricked roll, bad reveal), **recovery
  branches are surfaced first** via the Recovery panel.
- **Live market prices** from poe2scout drive cost ranking; the
  off-meta finder surfaces niche crafting goals based on
  poe.ninja PoE2 build popularity.
- **Genesis Tree panel (0.5)** — the full in-game Breach crafting tree
  (all 248 nodes, real layout, PoE2-style tooltips) with curated,
  source-cited "best nodes per goal" presets: Divine/Exalt/Catalyst
  farming, minion belts, caster rings, attribute amulets, Breachstones.
  Verisium Alloys and Distilled Emotions are fully data-driven 0.5
  currencies the engine can simulate and the advisor can propose.
- **Wasm plugin SDK** lets the community ship custom predicates,
  strategies, rules, and recommendation emitters; the advisor
  threads plugin dispatch into the planner's beam search.

## Platform

The web app runs in **any modern browser** (WebAssembly + Web Workers).
The desktop app targets **Linux, NixOS, and Windows 11**
([ADR-0010](docs/adr/0010-desktop-shell-electron-cross-platform.md)):
CI builds and tests all three paths — Nix lanes for Linux/NixOS, a
rustup + Bun lane (no Nix) plus an NSIS package on `windows-latest`, and
AppImage/deb packaging on Linux. Development uses a Nix flake on NixOS
(`nix develop` provides electron for desktop dev-runs); on Windows,
rustup honors `rust-toolchain.toml` and the same Bun commands work. The
old desktop-only constraints
([`docs/adr/0002-platform-nixos-only.md`](docs/adr/0002-platform-nixos-only.md))
applied to the retired Tauri app.

## Quick start

All commands run from the **repo root** (the root is both a Cargo workspace and
a Bun workspace):

```bash
nix develop

# Rust engine + tests
cargo build --workspace
cargo test --workspace

# Web app — Bun, all from the repo root
bun install                           # installs the apps/web workspace
bun run wasm                          # crates/poc2-wasm → apps/web/lib/wasm + public/wasm
bun run dev                           # http://localhost:3000 — opens in a real browser

# Production: a fully static, server-less export
bun run build                         # → apps/web/out/  (serve from any static host)

# Other web scripts (also from root): bun run typecheck · bun run lint
```

The web app needs two static assets in `apps/web/public/`: the WASM module
(`wasm/poc2_wasm_bg.wasm`, produced by `scripts/build-wasm.sh`) and a data
bundle (`poc2.bundle.json.gz`, copied from `~/.config/poc2/bundles/` — see
First-run setup).

To clone the reference repos (~1.5 GB):

```bash
./scripts/clone-example-repos.sh
```

### First-run setup

1. **Build a data bundle** (one-shot, ~3 minutes; needs network):
   ```bash
   cargo run --release -p poc2-pipeline -- build \
     --out ~/.config/poc2/bundles/poc2.bundle.json.gz \
     --patch 0.4.0
   ```
   The bundle contains every mod, base, omen, essence, bone,
   catalyst, and weight observation the advisor reads. ~211 KB
   gzipped covering 2 740 bases, 2 123 mods, 81 essences, 44 omens,
   12 catalysts, 10 bones, 2 951 weights.

2. **Hyprland always-on-top** (optional but recommended):
   ```bash
   # ~/.config/hypr/hyprland.conf
   source = /path/to/poc2/examples/hyprland/poc2-windowrules.conf
   ```
   See
   [`examples/hyprland/`](examples/hyprland/) for the NixOS module
   variant + per-rule explanations.

3. **Live prices** (optional): click "Refresh prices" in Settings
   to pull from poe2scout, or set the auto-refresh interval.

## Architecture

```
WEB UI  (Next.js 16 + React 19, static export — the "Forge" console)
   │  UI thread ⇄ Web Worker (postMessage)
   ▼
WASM ENGINE  (crates/poc2-wasm, wasm-bindgen) — in-memory EngineState
   │
ADVISOR  (beam-search + Monte Carlo + streaming)
   │
   ├── STRATEGY LIBRARY  (23 codified TOML recipes)
   ├── RULE ENGINE       (113 forward-chained rules across 14 sections)
   ├── PROBABILITY       (Monte Carlo + Wilson score intervals)
   └── PLUGINS           (Wasm Component-Model + capability gates)
   │
   ▼
ENGINE CORE  (sub-µs apply(currency, item, omens))
   │
   ▼
DATA BUNDLE  (patch-versioned, hot-swappable)
   │
   ├── RePoE-fork    (mods, bases, tags)
   ├── Craft of Exile (essences, catalysts, weights ≥80% join)
   ├── poe2db.tw     (omens, bones)
   └── poe2scout     (live currency prices)
```

See [`docs/40-architecture.md`](docs/40-architecture.md) for the full
picture; [`docs/35-advisor-architecture.md`](docs/35-advisor-architecture.md)
for the planner internals; [`docs/36-decision-engine.md`](docs/36-decision-engine.md)
for the rule-priority pipeline.

## v1.0 feature matrix

| Phase | Deliverable | Status |
|---|---|---|
| M1 | Foundation (flake, workspace, CI) | ✅ |
| M2 | Engine core + data pipeline | ✅ |
| M3 | Strategy DSL + rule engine | ✅ |
| M4 | Beam-search advisor | ✅ |
| M5 | Probability + market | ✅ |
| M6 | UI v1 | ✅ |
| M7 | Clipboard import + Client.txt watcher | ✅ |
| M8 | Polish + release | ✅ |
| A.1 | PredicateContext threaded through advisor + rules | ✅ |
| A.2 | DSL action extensions (ActivateOmen, Recombine, Reveal floors) | ✅ |
| A.3 | CoE→engine mod-id join refinement | ✅ |
| A.4 | 23 strategies encoded (full /docs/33 coverage) | ✅ |
| A.5 | 113 rules across 14 section files (full /docs/34 coverage) | ✅ |
| A.6 | Bundle hot-swap via reload_bundle | ✅ |
| A.7 | docs/32, 36, 37, 51 | ✅ |
| B.1-B.4 | Target / Recovery / Settings / Recipe panels | ✅ |
| C.1-C.3 | Monte Carlo + streaming + simulation runner | ✅ |
| D.1-D.3 | Client.txt watcher + Hyprland + trade URL | ✅ |
| E.1-E.2 | poe.ninja meta + off-meta finder | ✅ |
| F.1-F.8 | Wasm Plugin SDK | ✅ |
| G.1-G.2 | Perf pass + release | ✅ |

## Performance

Per [`docs/35-advisor-architecture.md`](docs/35-advisor-architecture.md)
(i7-class laptop):

| Operation | Time | Budget | Margin |
|---|---|---|---|
| `plan_depth_1_top_3` | 46 µs | 1 ms | ×21 |
| `plan_depth_3_top_3_mc50` | 139 µs | 5 ms | ×35 |
| `plan_depth_5_width_8` | 151 µs | 500 ms | ×3311 |

## Documentation

- [`00-overview.md`](docs/00-overview.md) — vision, scope, decisions
- [`11-game-mechanics.md`](docs/11-game-mechanics.md) — PoE2 0.4 crafting mechanics reference
- [`13-patch-0.4-changes.md`](docs/13-patch-0.4-changes.md) — Fate of the Vaal patch deltas
- [`30-domain-model.md`](docs/30-domain-model.md) — Item / Mod / Omen / Strategy / Rule types
- [`31-engine-algorithms.md`](docs/31-engine-algorithms.md) — apply_currency contract
- [`32-probability-math.md`](docs/32-probability-math.md) — geometric / MC / Wilson math
- [`33-strategy-library.md`](docs/33-strategy-library.md) — 23 codified strategies
- [`34-heuristics-rulebook.md`](docs/34-heuristics-rulebook.md) — 120-rule catalogue
- [`35-advisor-architecture.md`](docs/35-advisor-architecture.md) — beam search + scoring
- [`36-decision-engine.md`](docs/36-decision-engine.md) — production-rule synthesis
- [`37-recovery-flows.md`](docs/37-recovery-flows.md) — recovery DAGs
- [`40-architecture.md`](docs/40-architecture.md) — system architecture
- [`41-ui-flows.md`](docs/41-ui-flows.md) — UI components + IPC commands
- [`51-market-meta.md`](docs/51-market-meta.md) — poe.ninja meta aggregator
- [`70-roadmap.md`](docs/70-roadmap.md) — milestones M1-M9+
- [`72-v1-execution-plan.md`](docs/72-v1-execution-plan.md) — Phases A-G
- [`adr/`](docs/adr/) — 9 architecture decision records

## Plugins

The Wasm plugin SDK (`crates/plugin-sdk`) lets the community extend
the advisor with custom predicates, strategies, rules, and
recommendation emitters. See
[`examples/plugins/`](examples/plugins/) for a working example +
build/install instructions.

```toml
# Reference a plugin custom predicate from a rule:
[[rule]]
when = { custom = { plugin_id = "predicate-meta-build-match", name = "matches_meta_build", args = { ascendancy = "Stormweaver" } } }
```

## License

[MIT](LICENSE). Game data and weights are credited to their sources;
see [`docs/adr/0003-data-sources.md`](docs/adr/0003-data-sources.md)
and [`docs/adr/0005-license-mit.md`](docs/adr/0005-license-mit.md).

## Related projects

- [pathofcrafting.net](https://pathofcrafting.net/) — web crafting simulator (separate brand, not this project)
- [Craft of Exile](https://craftofexile.com/?game=poe2) — community gold-standard simulator
- [POE2HTC](https://github.com/Dboire9/POE2_HTC) (AGPL-3) — reference for beam-search
- [pyoe2-craftpath](https://github.com/WladHD/pyoe2-craftpath) (MIT) — reference + potential dependency
- [XileHUD](https://github.com/XileHUD/poe_overlay) (GPL-3) — reference for clipboard / overlay
