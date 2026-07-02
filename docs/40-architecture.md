# Architecture

> System architecture for Path of Crafting 2 (current: web/WASM app +
> optional Electron desktop shell, bundle schema v3, patch 0.5).

## Layers

```
                            ┌──────────────────────┐
                            │   ADVISOR            │
                            │  beam-search planner │◄───── risk slider · market context
                            │  full re-plan        │◄───── trained Q-tables (PlanInput.trained_models)
                            └─────────┬────────────┘
              ┌───────────────────────┼─────────────────────────┐
              ▼                       ▼                         ▼
      ┌──────────────┐        ┌──────────────┐         ┌─────────────────┐
      │  STRATEGY    │        │  RULE        │         │  PROBABILITY    │
      │  LIBRARY     │        │  ENGINE      │         │  & EV LAYER     │
      │  43 TOMLs    │        │  142 rules   │         │  MC ± stderr    │
      └──────┬───────┘        └──────┬───────┘         └────────┬────────┘
             └───────────────────────┼──────────────────────────┘
                                     ▼
                         ┌──────────────────────┐
                         │  ENGINE CORE         │
                         │  apply(currency,     │  patch + league gated;
                         │  item, omens)        │  sub-µs hot path
                         └──────────┬───────────┘
                                    ▼
                  ┌─────────────────────────────────┐
                  │  DATA BUNDLE (schema v3)        │
                  │  patch-versioned, hot-swappable │
                  └────────────┬────────────────────┘
                               │
          ┌────────────────────┼──────────────────────┐
          ▼                    ▼                      ▼
   ┌─────────────┐      ┌────────────┐      ┌──────────────────────┐
   │  RePoE-fork │      │  poe2db.tw │      │  Craft of Exile      │
   │  mods/bases │      │  omens     │      │  essences/catalysts  │
   │  tags       │      │  bones     │      │  weights             │
   └─────────────┘      └────────────┘      └──────────────────────┘
          + curated fixtures: desecrated pools, Vaal implicits,
            alloys.json, emotions.json, brequel_tree.json (Genesis)

  Live channels (runtime, all soft-fail):
   • poe2scout        → currency prices (browser fetch / desktop proxy / desktop cache)
   • poe.ninja        → exchange prices (parallel source, fuzzy name-matched)
   • GGG /trade2      → price-check search+fetch (desktop main-process proxy)
   • Clipboard        → item import (paste, or desktop hotkey → game Ctrl+C)
   • Capture daemon   → ws://127.0.0.1:17771/ws (ADR-0011, optional)
   • Screen region    → OCR price overlay (ADR-0013, desktop only)
```

## Process model

The app is a static Next.js export (no server) running entirely in the
browser, with an optional Electron shell around it:

1. **UI thread** (React 19 — the "Forge" console in `apps/web`) — renders
   the UI and dispatches typed RPC calls over `postMessage`
   (`apps/web/lib/engine/client.ts`).
2. **Web Worker hosting the WASM engine**
   (`apps/web/lib/engine/engine.worker.ts` + `crates/poc2-wasm`) — loads
   `/wasm/poc2_wasm_bg.wasm` and `/poc2.bundle.json.gz` once, builds an
   in-memory `EngineState`, then answers `recommend` / `parse` /
   `eligibleMods` / `recordOutcome` / `runNTrials` / `applyPrices` /
   `genesisTree` / … off the UI thread, so planning never blocks. A
   re-plan is a **single** `recommend` call; the store token-discards
   superseded results (no progressive streaming).
3. **Electron main process** (`apps/desktop`, optional — ADR-0010/0013):
   serves the export over a privileged `app://` scheme, registers capture
   hotkeys, proxies the trade2 API with header-driven rate limiting,
   keeps the hourly poe2scout price cache (node:sqlite), and owns the
   overlay/calibration windows. The **preload** exposes exactly one
   bridge, `window.poc2Desktop` (contract mirrored in
   `apps/web/lib/desktop.ts`); the web app feature-detects it and
   degrades gracefully in a plain browser.

The advisor's beam search runs inside the Web Worker (WASM); Rust panics
forward to `console.error`. (The original Tauri 2 + Svelte frontend was
retired — see ADR-0001's amendment and ADR-0010.)

## Patch + league versioning

Every entity carries `patch_min` / `patch_max`:

```rust
struct PatchRange { min: Option<PatchVersion>, max: Option<PatchVersion> }
```

- Mods, currencies, omens, essences, bones, catalysts, alloys, emotions —
  versioned at the data-bundle level.
- Strategies and rules — versioned in TOML (`patch_min = "0.4.0"`).
- The bundle declares its `game_patch`; loaders filter entities to those
  whose `PatchRange` contains it.
- **League** (`Standard` | `Challenge`) rides on `ApplyContext` and
  `PlanInput.league` for gates that differ inside one patch — e.g. the
  Recombinator and the Corruption/Homogenising omens are Standard-only
  in 0.5. User-switchable from Settings via the WASM `setLeague`.

Full delta matrix: [`14-crafting-mechanics-cross-version.md`](14-crafting-mechanics-cross-version.md).

## Sub-microsecond `apply()`

The advisor's beam search runs tens of thousands of
`apply(currency, item, omens)` calls during a re-plan. Constraints:

- No allocations in the hot path. `Item` is small (`SmallVec` for mod
  slots, fixed-size arrays for fractures).
- Mod pools precomputed at bundle load; weight resolution is
  tag-intersection (leftmost-tag-wins) with inclusive higher-tier
  summation (`ModRegistry::inclusive_weight_for`).
- `Currency::apply` is **atomic on failure** — the orchestrator
  (`apply_currency_with_bases`) snapshots and restores the item on `Err`.
- Beam width / depth are user-configurable (web defaults: width 5,
  depth 4).
- Cancellation: a new state arriving bumps the store's re-plan token; the
  stale result is discarded on arrival.

## Data bundle

A bundle is a single JSON (or gzipped JSON) document containing the
entire dataset the engine needs. **Schema v3** (see
`crates/data/src/lib.rs` for the version history — v1/v2 bundles are
hard-rejected with a rebuild instruction):

```jsonc
{
  "header": {
    "schema_version": 3,
    "engine_schema": 1,
    "game_patch": "0.5.0",
    "built_at": "…", "built_by": "poc2-pipeline@…",
    "sources": [ /* upstream revisions for provenance (ADR-0012) */ ]
  },
  "mods": [...],              // incl. explicit tier ordinals (v3)
  "base_items": [...],
  "item_classes": [...],
  "tags": [...],
  "currencies": {...},
  "omens": {...},
  "essences": {...},          // per-class target variants
  "bones": {...},
  "catalysts": {...},
  "alloys": {...},            // 13 Verisium Alloys × class targets (0.5)
  "emotions": {...},          // 26 Distilled Emotions × jewel-base targets (0.5)
  "genesis": {...},           // Genesis Tree nodes + curated presets (0.5)
  "stat_translations": {...},
  "weights": [...],           // CoE-derived numerical weights, confidence-flagged
  "synergy_edges": [...], "synergy_overrides": [...],
  "concepts": [...], "concept_map": {...}
}
```

Bundles are produced by `pipeline/` (`cargo run -p poc2-pipeline -- build
--patch 0.5.0`). The web app ships one as a static asset
(`apps/web/public/poc2.bundle.json.gz`); operator copies live under
`~/.config/poc2/bundles/`. The `data-watch.yml` workflow (ADR-0012)
detects upstream changes, rebuilds, diffs, and opens a draft PR.

## Hybrid mods (concept-based matching)

A "hybrid" mod is a single-affix mod producing multiple distinct concepts
(e.g., `+X% ES AND +Y Life`). The engine handles them via a **concept
map**:

1. RePoE-fork's `mods.json` lists each mod's `stats: [{id, min, max}, …]`
2. The pipeline computes a `Concept` per `stat-id` (atomic semantic group)
3. Each mod is annotated with `concept_set: Set<Concept>`
4. Targets are concept-based: `{ concept: "EnergyShield", min_tier: 1 }`
   matches any mod whose `concept_set` contains `EnergyShield`
5. A hybrid `ES + Life` mod simultaneously satisfies `EnergyShield` and
   `Life` targets

This is required for the canonical "Triple T1 ES Body Armour" test
fixture, where the user accepts hybrid ES mods alongside flat ES mods.

## Desktop shell specifics (ADR-0010 / 0011 / 0013)

- **Serving**: `app://poc2/` privileged scheme over `apps/web/out` — the
  export's root-absolute asset URLs work without a localhost server.
  Hence the hard rule: **web runtime asset URLs stay origin-relative**.
- **Capture**: snapshot clipboard → inject the game's own Ctrl+C
  (per-platform backends) → poll for `Item Class:` text → push to the
  renderer (`ingestExternalItemText`) → restore clipboard. Wayland
  compositors without portal hotkeys bind the second-instance flags
  (`poc2-desktop --capture / --scan / --recalibrate`).
- **Trade proxy**: `POST /api/trade2/search/{league}` +
  `GET /api/trade2/fetch/…` run in main via `electron.net`, throttled by
  the API's `X-Rate-Limit-*` headers.
- **Price cache**: hourly poe2scout catalogue (11 categories) into
  node:sqlite (JSON/memory fallback), poe.ninja fallback rows appended
  after (first-write-wins keeps poe2scout authoritative). Snapshot flows
  to the renderer once per scan — no per-lookup IPC.
- **OCR overlay**: capability gate decides full click-through window
  (win32/X11/probe-pass Wayland) vs. degraded in-app panel
  (Hyprland/wlroots). Region calibrated once (`/calibrate`), scans are
  single hotkey-triggered passes OCR'd renderer-side (tesseract.js from
  vendored `/ocr/` assets).

## Platform notes

- The web app runs in any modern browser; no platform code outside
  `apps/desktop` and marked operator tooling.
- **flake.nix** provides the Rust toolchain (+ wasm32), wasm-bindgen /
  binaryen, Bun + Node, and electron for NixOS dev-runs. Windows dev uses
  rustup + Bun without Nix (CI proves this lane).
- Paths resolve via the `XDG_CONFIG_HOME → HOME → USERPROFILE/APPDATA`
  chain; `.gitattributes` enforces LF so `include_str!` matches across
  OSes.
- PoE2 under Proton/Wine: clipboard capture works via the game's own
  Ctrl+C; Hyprland users source `examples/hyprland/poc2-windowrules.conf`
  for always-on-top float rules (ADR-0009's layer-shell deferral stands).
