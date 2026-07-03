# UI Flows (web app)

> Describes the current web frontend in `apps/web/` — Next.js 16 +
> React 19, static export, rendered as the PoE2-styled "Forge" console.
> The UI design system (colors, fonts, component recipes) lives in
> [`apps/web/DESIGN.md`](../apps/web/DESIGN.md) — all new UI follows it.
> (The original Tauri 2 + Svelte 5 frontend this doc used to describe was
> retired; see ADR-0010.)

## Shell layout (`components/Console.tsx`)

```
┌──────────────────────────────────────────────────────────────────────┐
│ ⬡ PATH OF CRAFTING   <base · rarity · ilvl>   [capture] ⟲ patch ⟳  │  topbar
├────┬────────────────────────────────┬────────────────────────────────┤
│ ▣  │  THE BENCH                     │  ACTIVE PANE                   │
│ ◎  │  ItemCard (poe2db-style popup) │  one panel per rail section    │
│ ⚑  │  TargetSummary                 │                                │
│ ▦  │                                │                                │
│ …  │                                │                                │
├────┴────────────────────────────────┴────────────────────────────────┤
│  LedgerDock: spent · next · projected · risk/depth · [Record outcome]│  dock
└──────────────────────────────────────────────────────────────────────┘
```

Rail sections → panels (one component per workflow):

| Section | Component | What it does |
|---|---|---|
| Item | `ItemEditor` | paste import, manual edit, screenshot OCR, parse preview + unresolved lines |
| Target | `TargetEditor` | concept palette (seeded from the base's real eligible pool), archetype presets, tiers/hybrid/budget |
| Guide | `GuidePanel` | hero recommendation + alternatives + success band (MC ± stderr) + recovery hints + traceability chip |
| Eligible | `EligibleTab` | the rollable mod pool for the item's base/ilvl, weights + gates made explicit |
| History | `HistoryTab` | recorded outcomes, undo, cost ledger |
| Database | `DatabasePanel` | bases + materials browser (engine `listDatabaseEntries` / `databaseEntryDetail`) |
| Price | `PricePanel` | trade2 stat filters from the imported item, live listings (desktop proxy) or deep link (browser) |
| Regex | `RegexPanel` | in-game search-string generator: Goal (craft target → stash-search), Item mods / Waystone / Tablet (pool selection + roll floors + `!`unwanted), Vendor (shopping filters); 250-char budget meter, copy/auto-copy |
| Genesis Tree | `GenesisPanel` | full-bleed 0.5 Breach tree with real art + curated goal presets (engine `genesisTree`) |
| Tools | `ToolsPanel` | simulation runner (`runNTrials`) + recipe library (IndexedDB) |
| Settings | `SettingsPanel` | market league + price refresh, engine League ruleset, desktop price-cache status, capture diagnostics, plugins (add/remove `.wasm`), notes, data/reset |

`OutcomeDialog` (modal) records what actually happened in game:
add/remove/reroll mod, rarity changes — validated by the engine
(`checkCanApply`, `recordOutcome`, `rerollableMods`).

Extra routes for the desktop shell (inert stubs in a plain browser):

- `/overlay` — the ADR-0013 price overlay surface (click-through plates
  in full mode, in-app panel in degraded mode; OCR + price resolution run
  here).
- `/calibrate` — full-screen drag-select to calibrate the screen region
  the overlay scans.

## State (`lib/store.ts` — Zustand)

Single source of truth: item, goal, recommendations, risk/depth, history
(with undo), active section, eligible pool, parse metadata, league +
engine league, capture status, notes.

- **Re-plan on every state change**: `setItem/setGoal/setRisk/setDepth`
  → `replan()` → one worker `recommend` call. A monotonically increasing
  token discards superseded results — no streaming, no blocking.
- **Persistence is middleware**: one debounced store subscriber diffs the
  persisted slice and writes to IndexedDB (`lib/persist.ts`,
  `idb-keyval`). Never call `persist()` manually.
- **External item text enters through `ingestExternalItemText`** — the
  seam shared by the desktop capture bridge (`lib/bootDesktop.ts`), the
  ADR-0011 capture daemon (`lib/captureBridge.ts`,
  `ws://127.0.0.1:17771/ws`), and screenshot OCR (`lib/ocr.ts`).

## Engine RPC (`lib/engine/`)

The worker (`engine.worker.ts`) loads `/wasm/poc2_wasm_bg.wasm` + the
bundle `/poc2.bundle.json.gz` once, constructs the WASM `Engine`, then
serves generic `{ id, method, args }` messages. Complex args cross as
JSON strings; string results parse back to objects.

The typed client (`client.ts`) mirrors every Engine method:

| Group | Methods |
|---|---|
| metadata | `patch`, `modCount`, `league`, `setLeague` |
| planning | `recommend(item, goal, risk, depth, topN)` |
| import | `parseItemText` |
| apply/record | `checkCanApply`, `recordOutcome` |
| inspect | `eligibleMods`, `rerollableMods` |
| recovery | `recoveryHints(strategyId, stepId)` |
| simulate | `runNTrials` |
| database | `listBases`, `listDatabaseEntries`, `databaseEntryDetail` |
| prices | `applyPrices` (poe2scout), `applyNinjaPrices` (poe.ninja) |
| resolve | `resolveName` (fuzzy name → canonical key; OCR + prices) |
| genesis | `genesisTree` |
| trained models | `trainedModelCount` (the worker loads the optional `/trained-models.json` at boot via `loadTrainedModels`; ⚛ topbar chip) |
| plugins | `setPluginContent` (phase 1 emission), `setPluginDispatch`/`clearPluginDispatch` (phase 2 live predicates; wired worker-side via the `__loadPlugins` transfer message) |

New engine-boundary methods must update `client.ts` + `lib/types.ts`
together, then typecheck.

## Desktop bridge (`lib/desktop.ts`)

`window.poc2Desktop` is the only contact surface with Electron (typed
contract mirrored in `apps/desktop/src/preload.ts` — change both or
neither). The web app **never imports Electron**; every feature detects
the bridge and no-ops in a plain browser. Surface: capture
(`onItemText`, `captureNow`, `captureStatus`), trade proxy
(`tradeSearch`, `tradeFetch`), allowlisted `fetchJson`, overlay/region
(`capabilities`, `captureRegion`, `overlayShow/Hide/SetRegion`,
`calibrateRegion`, `onRegionCalibrated`, `onOverlayState`), and the price
cache (`pricesSnapshot/Status/Refresh/SetLeague`).

## Key flows

### Import → plan → record

1. Item text arrives (paste / capture / OCR) → `parse` → real bundle base
   id + rolled values + unresolved lines surfaced in the Item pane.
2. Any item/goal/risk/depth change → `replan()` → GuidePanel renders the
   hero recommendation with EV math, MC confidence band, and the source
   rule/strategy chip.
3. User applies the action in game, records the outcome
   (`OutcomeDialog`) → engine mutates the item → history entry (undo
   keeps the pre-state) → automatic re-plan.

### Price check

`PricePanel` matches the imported item's raw lines against
`public/trade-stats.json` (1,932 pipeline-generated entries,
EE2-semantics matcher in `lib/trade/statIndex.ts`), builds a trade2 query
(`lib/trade/queryBuilder.ts`; min bound = roll × 0.9), then either runs
it through the desktop proxy (grouped listings, cheapest/median, unknown
bases degrade to stats-only) or opens the trade-site deep link.

### Goal → stash-search regex

The Regex panel's Goal tab (also reachable via the ⧉ button on the
bench's Target card) compiles the current target into an in-game search
string: per spec, qualifying mods' `text_template` lines are reduced to
shortest-unique fragments against the base's full pool
(`lib/regex/shortestUnique.ts`), tier floors become per-mod-group roll
floors (`(8[5-9]|9\d|\d\d\d).*m life`), and the terms AND-combine under
the game's 250-char budget (`lib/regex/searchString.ts`). Precision
first: mods whose text can't be told apart from non-qualifying mods are
skipped with a warning rather than emitted as false-positive patterns.
Inspired by poe2.re (unlicensed — clean-room reimplementation; fragments
are computed at runtime from the bundle, never vendored).

### Prices → planner

Settings "Refresh prices" assembles the poe2scout snapshot (browser fetch
or bridge `fetchJson`), applies it via `applyPrices`, and immediately
re-plans so recommendations use the fresh valuator. The desktop price
cache (hourly, sqlite) additionally feeds the OCR overlay and is shown in
Settings; `setLeague` (and boot) point it at the active league.

### OCR price overlay (desktop, ADR-0013)

Scan hotkey (`Ctrl+Shift+S` / `poc2-desktop --scan`) → main positions the
overlay (full mode) or signals the in-app panel (degraded) → `/overlay`
runs ONE pass: `captureRegion` → preprocess (icon-crop, invert, upscale)
→ tesseract (vendored `/ocr/` runtime) → `extractRows` → `resolveName`
against the price-cache candidates → row-locked price plates.
Portal-denied capture falls back to the clipboard item path. Calibration:
`Ctrl+Shift+C` / `--recalibrate` → `/calibrate` drag-select → persisted
region.

## Bundle loading

The web app fetches `/poc2.bundle.json.gz` (a gitignored local asset —
0.5 / schema v3). To build/refresh it:

```bash
cargo run --release -p poc2-pipeline -- build \
  --out ~/.config/poc2/bundles/poc2.bundle.json.gz --patch 0.5.0
cp ~/.config/poc2/bundles/poc2.bundle.json.gz apps/web/public/
```

Schema-mismatched bundles are hard-rejected by the loader with a rebuild
instruction (`crates/data/src/lib.rs`).

## Hyprland always-on-top (optional)

Per ADR-0009, always-on-top is a documented Hyprland config recipe, not a
layer-shell surface — source
[`examples/hyprland/poc2-windowrules.conf`](../examples/hyprland/) and
run PoE2 in borderless windowed mode.
