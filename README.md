# Path of Crafting 2 (`poc2`)

A crafting **advisor** for Path of Exile 2 — beam-search optimal-path
planning with full re-plan on every state change, branching recovery
flows, live market awareness, and a trained-policy layer for long
spam-loop crafts.

The advisor is a **Rust engine that runs entirely in your browser**: a
Next.js 16 + React 19 web app (`apps/web`) hosts the engine compiled to
**WebAssembly** (`crates/poc2-wasm`) inside a Web Worker. No server, no
install — every recommendation is deterministic client-side compute over a
static data bundle.

A **desktop app** (`apps/desktop`, Electron — [ADR-0010](docs/adr/0010-desktop-shell-electron-cross-platform.md))
wraps the same web app like Discord wraps a website, and adds what a
browser can't do:

- **Item capture** (hotkey → the game's own Ctrl+C → instant import —
  same mechanism as Awakened PoE Trade).
- **Price checking** against the official PoE2 trade2 API (rate-limited,
  proxied through the main process, no CORS).
- **A poe2scout price cache** (hourly refresh, poe.ninja fallback) and a
  capability-gated **screen-region OCR price overlay**
  ([ADR-0013](docs/adr/0013-item-capture-ocr-overlay.md)) with a
  one-time calibration flow.

Targets: Linux, NixOS, and Windows 11 (macOS out of scope).

> Status: **v1.0 shipped; post-v1 work integrates on `dev` and ships from
> `master`** (see [Branch & release flow](#branch--release-flow)). Since
> v1: the **v2** crafter-helper pass ([`docs/80`](docs/80-crafter-helper-v2-plan.md)),
> the **v3** engine-training pass ([`docs/81`](docs/81-engine-training-and-rule-encoding-plan.md)),
> the **crafting-mechanics fidelity + PoE2 0.5 "Return of the Ancients"**
> pass ([`docs/83`](docs/83-crafting-fidelity-plan.md), [`docs/14`](docs/14-crafting-mechanics-cross-version.md)),
> and the **Electron desktop shell + capture + price checking** pass
> (ADR-0010/0013) have all shipped. The engine models item-level-dependent
> pools, inclusive higher-tier weighting, patch-versioned
> Minimum-Modifier-Level floors, and cross-version (0.3/0.4/0.5) +
> league (Standard/Challenge) gating. See [`CHANGELOG.md`](CHANGELOG.md)
> and [`docs/70-roadmap.md`](docs/70-roadmap.md) for what's next.

## What it does

- **Import an in-game item** by copying it in-game and pasting into the
  Item panel (`navigator.clipboard` → Rust parser → engine state), or
  build it manually in the UI. In the desktop app, press the **capture
  hotkey** (`Ctrl+Shift+D`) while hovering an item in PoE2 and it imports
  itself.
- **Price-check it**: the Price Check panel matches the item's mod lines
  to official trade-API stat ids (1,932-entry pipeline-generated table),
  builds a real trade2 query (toggleable stat filters, min bounds at 90%
  of the roll), and — in the desktop app — shows live listings with
  cheapest/median; browsers open the identical query on the trade site.
- **Declare your target mods + budget** via the Target panel
  (concept-aware, hybrid-mod aware, with archetype presets seeded from
  the base's real mod pool).
- The advisor **proposes the optimal next currency / omen** to use,
  with explanation, EV math, confidence band (Monte Carlo ± stderr),
  and full traceability back to the source rule / strategy /
  heuristic.
- You apply it, record the outcome, and the app **re-plans
  automatically** on every item/goal/risk/depth change (debounced,
  token-guarded — a newer plan supersedes an in-flight one).
- If the outcome was a failure (bricked roll, bad reveal), **recovery
  branches are surfaced** from the strategy's recovery metadata.
- **Live market prices**: poe2scout (and poe.ninja exchange as a
  parallel source) feed the planner's valuator via the WASM
  `applyPrices` / `applyNinjaPrices` boundary; the desktop shell keeps
  an hourly sqlite-backed price cache that also prices the OCR overlay.
  Everything soft-fails — planning never depends on the network.
- **In-game search regex generator** (Regex panel, inspired by
  [poe2.re](https://poe2.re)): turn your **craft target into a stash-search
  string** — paste it in game and a finished item lights up after every
  roll session; pick arbitrary mods from the base's real pool (with roll
  floors like `(8[5-9]|9\d|\d\d\d).*m life`); or build vendor shopping
  filters (class / rarity / ilvl / movement / resists). Fragments are
  computed at runtime against the live bundle pool — shortest unique
  substring, no false positives, 250-char budget meter.
- **Genesis Tree panel (0.5)** — the full in-game Breach crafting tree
  (real layout and art, PoE2-style tooltips) with curated, source-cited
  "best nodes per goal" presets: Divine/Exalt/Catalyst farming, minion
  belts, caster rings, attribute amulets, Breachstones. Verisium Alloys
  and Distilled Emotions are fully data-driven 0.5 currencies the engine
  can simulate (and, for alloys, the advisor proposes).
- **League-aware**: switch Standard ↔ Challenge (Runes of Aldur) in
  Settings — gates the Recombinator and the Corruption/Homogenising
  omens per the 0.5 rules.

## Platform

The web app runs in **any modern browser** (WebAssembly + Web Workers).
The desktop app targets **Linux, NixOS, and Windows 11**
([ADR-0010](docs/adr/0010-desktop-shell-electron-cross-platform.md)):
CI builds and tests all three paths — Nix lanes for Linux/NixOS, a
rustup + Bun lane (no Nix) plus an NSIS package on `windows-latest`, and
AppImage/deb packaging on Linux. Development uses a Nix flake on NixOS
(`nix develop` provides electron for desktop dev-runs); on Windows,
rustup honors `rust-toolchain.toml` and the same Bun commands work.

## Quick start

All commands run from the **repo root** (the root is both a Cargo workspace and
a Bun workspace):

```bash
nix develop

# Rust engine + tests
cargo build --workspace
cargo test --workspace

# Web app — Bun, all from the repo root
bun install                           # installs the apps/web + apps/desktop workspaces
bun run wasm                          # crates/poc2-wasm → apps/web/lib/wasm + public/wasm
bun run dev                           # http://localhost:3000 — opens in a real browser

# Production: a fully static, server-less export
bun run build                         # → apps/web/out/  (serve from any static host)

# Desktop shell (Electron)
bun run desktop:dev                   # against the dev server (start `bun run dev` first)
bun run desktop:start                 # serving apps/web/out over app:// (run `bun run build` first)

# Other scripts (also from root): bun run typecheck · lint · test:web · test:desktop
```

The web app needs two static assets in `apps/web/public/`: the WASM module
(`wasm/poc2_wasm_bg.wasm`, produced by `bun run wasm`) and a data bundle
(`poc2.bundle.json.gz` — a 0.5 bundle is committed; rebuild via First-run
setup). Optional regenerable assets (never committed): base-item icons
(`fetch-base-icons`), Genesis art (`fetch-genesis-assets`), and the OCR
runtime (`bun run ocr:assets`).

To clone the reference repos (~1.5 GB):

```bash
./scripts/clone-example-repos.sh
```

### First-run setup

1. **Build a data bundle** (one-shot, ~3 minutes; needs network):
   ```bash
   cargo run --release -p poc2-pipeline -- build \
     --out ~/.config/poc2/bundles/poc2.bundle.json.gz \
     --patch 0.5.0
   ```
   The bundle contains every mod, base, omen, essence, bone, catalyst,
   alloy, emotion, Genesis node, and weight observation the advisor
   reads. The shipped 0.5 bundle (schema v3) carries **3,626 mods,
   3,821 bases, 95 essences, 45 omens, 12 catalysts, 10 bones,
   13 Verisium Alloys (132 class-targets), 26 Distilled Emotions, and
   6,157 weights**. Copy it to `apps/web/public/poc2.bundle.json.gz` to
   ship it with the web app.

2. **Live prices** (optional): click "Refresh prices" in Settings to
   pull from poe2scout (the desktop shell also keeps its own hourly
   cache), or rely on the conservative built-in price bands.

3. **Hyprland always-on-top** (optional, Linux): source
   [`examples/hyprland/poc2-windowrules.conf`](examples/hyprland/) for
   float/pin window rules over the game.

## Architecture

```
WEB UI  (Next.js 16 + React 19, static export — the "Forge" console)
   │  UI thread ⇄ Web Worker (postMessage RPC)
   ▼
WASM ENGINE  (crates/poc2-wasm, wasm-bindgen) — in-memory EngineState
   │
ADVISOR  (beam search + Monte Carlo ± stderr + trained-policy uplift)
   │
   ├── STRATEGY LIBRARY  (43 codified TOML recipes)
   ├── RULE ENGINE       (142 forward-chained rules across 16 sections)
   ├── PROBABILITY       (Monte Carlo + Wilson score intervals)
   └── TRAINING          (offline Q-tables via pipeline `train-advisor`)
   │
   ▼
ENGINE CORE  (sub-µs apply(currency, item, omens); patch + league gated)
   │
   ▼
DATA BUNDLE  (schema v3, patch-versioned, hot-swappable)
   │
   ├── RePoE-fork    (mods, bases, tags)
   ├── Craft of Exile (essences, catalysts, weights)
   ├── poe2db.tw     (omens, bones; cross-validation)
   └── curated fixtures (desecrated, Vaal implicits, alloys, emotions, Genesis)

DESKTOP SHELL  (apps/desktop, Electron — optional)
   ├── app:// static serving of the web export
   ├── item capture (hotkey → game Ctrl+C → clipboard → import)
   ├── trade2 proxy (header-driven rate limiting)
   └── price cache (poe2scout hourly + poe.ninja fallback) + OCR overlay
```

Live prices (poe2scout / poe.ninja) enter through the browser or the
Electron proxy and are applied to the engine's valuator at runtime.
See [`docs/40-architecture.md`](docs/40-architecture.md) for the full
picture; [`docs/35-advisor-architecture.md`](docs/35-advisor-architecture.md)
for the planner internals; [`docs/36-decision-engine.md`](docs/36-decision-engine.md)
for the rule-priority pipeline.

## Feature status

| Area | Status |
|---|---|
| Engine core (33+ currencies, omens, essences, bones, catalysts, alloys, emotions) | ✅ shipped |
| Crafting fidelity (ilvl pools, inclusive tier weights, patch-versioned floors, league gating) | ✅ shipped (docs/83 P0–P6) |
| Beam-search advisor + Monte Carlo confidence + recovery hints | ✅ shipped |
| 0.5 content (Verisium Alloys, Distilled Emotions, jewel pool, Genesis Tree panel) | ✅ shipped |
| Web UI (item/target/guide/eligible/history/database/price/regex/genesis/tools/settings) | ✅ shipped |
| In-game search regex generator (goal / item-mods / vendor) | ✅ shipped — waystone/tablet tabs follow the data-gap work |
| Electron shell + item capture + trade2 price check | ✅ shipped (ADR-0010) |
| OCR price overlay + calibration + desktop price cache | ✅ shipped (ADR-0013) |
| Automated data refresh (watch → rebuild → diff → draft PR) | ✅ shipped (ADR-0012) |
| Trained-model planning (Q-tables in the WASM engine, ⚛ topbar chip) | ✅ shipped — drop a `train-advisor` artefact at `public/trained-models.json`; production-scale retrain is an operator step |
| Plugins phase 1 (strategy/rule emission, Settings → Plugins, ADR-0014) | ✅ shipped — custom predicates/emitters are phase 2 |
| Advisor candidates for Distilled Emotions | ✅ shipped (M14 audit) — pinned by a live-bundle test |
| Genesis birth simulation | ❌ intentionally out of scope |

## Performance

Per [`docs/35-advisor-architecture.md`](docs/35-advisor-architecture.md)
(i7-class laptop):

| Operation | Time | Budget | Margin |
|---|---|---|---|
| `plan_depth_1_top_3` | 46 µs | 1 ms | ×21 |
| `plan_depth_3_top_3_mc50` | 139 µs | 5 ms | ×35 |
| `plan_depth_5_width_8` | 151 µs | 500 ms | ×3311 |

## Branch & release flow

- `master` is the released branch; `dev` is the integration branch; work
  happens on typed branches (`feat/…`, `fix/…`, `docs/…`) cut from `dev`.
- [Conventional Commits](https://www.conventionalcommits.org) drive
  automated versioning: pushes to `master` keep a
  [release-plz](https://release-plz.dev) release PR open; merging it tags
  `vX.Y.Z`, creates the GitHub Release, and attaches the Electron
  packages (Windows NSIS + Linux AppImage/deb).
- See [`CONTRIBUTING.md`](CONTRIBUTING.md) for the full workflow.

## Documentation

- [`00-overview.md`](docs/00-overview.md) — vision, scope, decisions
- [`11-game-mechanics.md`](docs/11-game-mechanics.md) — PoE2 crafting mechanics reference
- [`14-crafting-mechanics-cross-version.md`](docs/14-crafting-mechanics-cross-version.md) — 0.3/0.4/0.5 deltas + gates
- [`30-domain-model.md`](docs/30-domain-model.md) — Item / Mod / Omen / Strategy / Rule types
- [`31-engine-algorithms.md`](docs/31-engine-algorithms.md) — apply_currency contract
- [`32-probability-math.md`](docs/32-probability-math.md) — geometric / MC / Wilson math
- [`33-strategy-library.md`](docs/33-strategy-library.md) — the strategy catalogue (0.4-era baseline)
- [`34-heuristics-rulebook.md`](docs/34-heuristics-rulebook.md) — the heuristics catalogue
- [`35-advisor-architecture.md`](docs/35-advisor-architecture.md) — beam search + scoring
- [`36-decision-engine.md`](docs/36-decision-engine.md) — production-rule synthesis
- [`37-recovery-flows.md`](docs/37-recovery-flows.md) — recovery DAGs
- [`40-architecture.md`](docs/40-architecture.md) — system architecture
- [`41-ui-flows.md`](docs/41-ui-flows.md) — web UI panels + engine RPC
- [`51-market-meta.md`](docs/51-market-meta.md) — market/price integrations
- [`70-roadmap.md`](docs/70-roadmap.md) — shipped milestones + what's next
- [`83-crafting-fidelity-plan.md`](docs/83-crafting-fidelity-plan.md) — the fidelity + 0.5 pass (shipped)
- [`apps/web/DESIGN.md`](apps/web/DESIGN.md) — the PoE2 in-game design system (UI source of truth)
- [`adr/`](docs/adr/) — 13 architecture decision records
- Historical plans (kept as implementation history): [`72`](docs/72-v1-execution-plan.md), [`80`](docs/80-crafter-helper-v2-plan.md), [`81`](docs/81-engine-training-and-rule-encoding-plan.md), [`90`](docs/90-ui-redesign.md)

## Plugins

The Wasm plugin SDK (`crates/plugin-sdk`) lets plugins ship custom
predicates, strategies, rules, and recommendation emitters (see
[`examples/plugins/`](examples/plugins/)). Per
[ADR-0014](docs/adr/0014-plugin-rewire-browser-host.md), the app now runs
a **browser-side JS plugin host**: add a plugin `.wasm` in
**Settings → Plugins** and its emitted strategies/rules join the
advisor's registries (sandboxed instantiation — no network/filesystem).
**Phase 2** (live custom predicates + recommendation emitters during
planning) is roadmap work; until then `custom` predicates evaluate to
false:

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
- [poe2.re](https://github.com/veiset/poe2.re) — the in-game search-regex tool that inspired the Regex panel (unlicensed; ours is a clean-room runtime reimplementation against the bundle pool)
- [Awakened PoE Trade](https://github.com/SnosMe/awakened-poe-trade) (MIT) — reference for item capture
- [Exiled Exchange 2](https://github.com/Kvan7/Exiled-Exchange-2) (MIT) — reference for trade stat matching
- [POE2HTC](https://github.com/Dboire9/POE2_HTC) (AGPL-3) — reference for beam-search
- [pyoe2-craftpath](https://github.com/WladHD/pyoe2-craftpath) (MIT) — reference + potential dependency
- [XileHUD](https://github.com/XileHUD/poe_overlay) (GPL-3) — reference for clipboard / overlay
