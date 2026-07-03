# Roadmap

> Shipped milestones + current/next work. M1–M8 = v1.0 (patch 0.4,
> Tauri-era — historical). Post-v1 passes: v2 (`docs/80`), v3 (`docs/81`),
> crafting fidelity + 0.5 (`docs/83`), desktop shell (ADR-0010), OCR price
> overlay (ADR-0013). **The "Current / Next" section at the bottom is the
> source of truth for unfinished work.**

## Shipped

### v1.0 — M1–M8 + Phases A–G (patch 0.4, 2026-04) ✅ historical

The full v1 build: Nix flake + Cargo workspace + CI (M1); engine core +
data pipeline (M2 — domain types, patch versioning, all basic currencies,
essences, omens, Fracturing/Hinekora's/bones/catalysts/recombinator,
sub-µs `apply()`); strategy + rule DSLs (M3); beam-search advisor with the
canonical "Triple T1 ES" rediscovery test (M4); Monte Carlo + market
valuator + poe2scout poller (M5); UI v1 (M6); clipboard import (M7);
polish + `v1.0.0` tag (M8). Phases A–G filled in full strategy/rule
coverage, target/recovery/settings/recipe panels, MC + simulation runner,
poe.ninja meta, the Wasm plugin SDK, and the perf pass.

Superseded/dropped v1 items (do not resurrect without an ADR):

- The **Tauri 2 + Svelte 5 desktop app** — replaced by the web/WASM app;
  native features returned via the Electron shell (ADR-0010).
- The **Client.txt watcher** — no browser equivalent; capture replaced it.
- The **Wayland layer-shell overlay** — deferred by ADR-0009 (still
  standing); Hyprland `windowrulev2` recipes + the ADR-0013 plain-window
  overlay are the shipped alternatives.
- GGG `/trade2` OAuth — public trade2 endpoints need no session.

### v2 — crafter-helper pass (`docs/80`) ✅

Advisor candidate legality (no illegal recommendations), full mod-pool
outcome dialog, concept-occupancy heuristics, tier-fix candidates,
omen-aware bone reveals, `LoopEstimate` recurring-step compression,
risk-slider variance weighting, per-base art plumbing.

### v3 — engine training + rule encoding (`docs/81`) ✅

Numerical CoE weights consumed at runtime (`ModRegistry`), `BaseRegistry`
class gating, `*_ONLY` flag enforcement, bundle schema v2, the offline
training pipeline (`train-advisor`: transition-model learning + Q-value
iteration, path-length + cost rewards), `PlanInput.trained_models`
consumption in the planner, and the expanded strategy/rule catalogue
(now 43 strategies, 142 rules across 16 sections).

### Crafting-mechanics fidelity + 0.5 content (`docs/83`) ✅

- P1–P4: ilvl-dependent pools, inclusive higher-tier weighting,
  keep-≥1-tier Min-Mod-Level exception, patch-versioned floors
  (`MinModLevelVariant`), explicit tier ordinals, tag-intersection
  weighting, bone-size/lord-omen fidelity, `League` on `ApplyContext` +
  `PlanInput.league`, cross-version gating (Recombinator, Corruption
  omen Standard-only).
- P5/P6: bundle **schema v3**; `ModKind::Crafted` + 0.5 mod caps;
  **Verisium Alloys** end-to-end (13 alloys × 132 class-targets, advisor
  candidates); **Distilled Emotions** (26 emotions × 96 jewel-base
  targets, engine apply); jewel mod pool; **Genesis Tree panel** (real
  tree data + curated presets; UI-only); patch-gated Vaal/Sanctification
  multiply semantics.
- Post-P6 audits: the 31-class audit-matrix sweep + poe2db
  cross-validation pass (real desecrated pools, real Vaal implicit pools,
  catalyst 0.5 model, essence class-targeting, alloy affix fixups).

### M10 — Desktop shell + capture + price checking (ADR-0010) ✅

- `apps/desktop` Electron shell (`app://` scheme over the static export,
  preload bridge `window.poc2Desktop`, single-instance flag forwarding).
- Item capture: hotkey → game Ctrl+C → clipboard → import (Windows
  `uiohook-napi`; Linux `hyprctl sendshortcut` → `ydotool` → `wtype`;
  APT semantics). Plus the standalone Hyprland capture daemon
  (`crates/capture`, ADR-0011).
- Price checking: pipeline `fetch-trade-stats` table (1,932 entries) →
  trade2 search/fetch proxied in main (header-driven rate limiting) →
  Price Check panel; browser fallback deep links.
- Live price snapshots → WASM `applyPrices` / `applyNinjaPrices` →
  planner valuator.
- CI: windows-latest lane (rustup + Bun, no Nix) + electron-builder
  artifacts (AppImage/deb, NSIS); release-plz release flow.

### OCR price overlay + price cache (ADR-0013) ✅

Capability-gated screen-region capture (silent on win32/X11, portal on
Wayland), `/calibrate` drag-select flow, `/overlay` click-through price
plates (full mode) or in-app panel (degraded mode on Hyprland/wlroots),
renderer-side Tesseract OCR with row-locking, and the desktop poe2scout
price cache (hourly, node:sqlite, poe.ninja fallback) that prices the
overlay and is surfaced in Settings.

### In-game search regex generator (Regex panel) ✅

Inspired by [poe2.re](https://github.com/veiset/poe2.re) (unlicensed —
clean-room reimplementation). Pure-TS lib (`apps/web/lib/regex/`) +
panel with three tabs: **Goal** (the craft target as a "the item is
done" stash-search string — per-spec terms, per-mod-group tier→roll
floors, precision-first skipping of unidentifiable mods), **Item mods**
(free pool selection with roll floors + negated unwanted group),
**Vendor** (class/rarity/ilvl/movement/resists/attributes/mod-family
shopping filters). Shortest-unique fragments are computed at runtime
against the live bundle pool (digit-free, roll-safe, `^`/`$` anchors),
so patterns can never drift from the data; assembly enforces the game's
250-char budget. Bun-tested (exhaustive digit-range verification +
no-false-positive property tests).

### Automated data refresh (ADR-0012) ✅

`poc2-pipeline watch` (patch pointer + RePoE hash detection),
`diff-bundle` markdown changelogs, `audit-matrix` legality sweeps, and
the `data-watch.yml` cron workflow that opens draft PRs against `dev`.

## Current / Next

Ordered by expected value; none are started unless noted.

- [ ] **Wire trained models into the WASM engine.** The planner consumes
  `PlanInput.trained_models` and `train-advisor` produces artefacts, but
  the web engine passes `None`. Needs: artefact loading over the worker
  boundary (fetch a `trained-models.json` static asset), cache lookup by
  `(goal_hash, item_class)`, and a UI badge when the Q-policy drove the
  pick. Then run the **production retrain** (`--samples 100000`) on the
  0.5 bundle.
- [ ] **Advisor candidates for Distilled Emotions.** Engine apply works;
  candidate generation needs base-level item fidelity (real jewel base
  names on the item). 5 Ancient-emotion targets stay display-only until
  RePoE exports their mods.
- [ ] **Re-wire the plugin host.** SDK + wasmtime host are built and
  tested but the app plans with `plugin_dispatch: None`. Decide the
  shape: native-only (desktop main process) vs. wasm-in-wasm is a real
  design question — needs a small ADR before code.
- [ ] **Price-id mappings for 0.5 currencies.** `default_id_mapping`
  (Rust) lacks alloys/emotions/bones-by-slug for poe2scout; the desktop
  cache already prices them by name. Extend the map + tests.
- [ ] **Remaining data gaps** (tracked since the poe2db pass): mod pools
  for Waystones (109), Precursor Tablets (83), Relics (139), Life/Mana
  Flasks (57), Charms (51), Inscribed Ultimatum (31), Expedition
  Logbooks (21); "Thrud's Might" weapon mechanic; Preserved Vertebrae
  (waystone desecration); Breach Ring quality caps (40/45); Essence of
  the Abyss granting only one of its two Mark mods per class; Vaal
  Catalysing Infuser; the 5 Expedition Saga omens. Landing the
  Waystone/Tablet/Relic pools also unlocks **Waystone/Tablet/Relic tabs
  in the Regex panel** (the lib is domain-agnostic — it just needs the
  mod texts in the bundle).
- [ ] **Overlay polish:** persisted-region hydration on the overlay
  route's first load (today the first scan can race the calibration
  push); portal restore-token reuse on Wayland (`windowState.portalToken`
  is persisted but not passed to the portal yet).
- [ ] **Unify the two OCR paths** — the item-screenshot OCR
  (`lib/ocr.ts`) should use the same vendored origin-relative `/ocr/`
  runtime as the overlay (`lib/ocr/tesseract.ts`).
- [ ] **Repo metadata alignment:** decide the canonical GitHub org
  (Cargo metadata says `anomalyco`, release workflow gates on
  `grok-insider`), and sync `apps/desktop/package.json` versioning with
  release-plz (document or automate).

## Deferred / out of scope (unchanged decisions)

- Cachix binary cache; Hardcore/SSF support; macOS support; self-hosted
  data pipeline; empirical weight derivation from trade samples; MCTS
  advisor upgrade; real Wayland layer-shell overlay (ADR-0009); GGG
  `/trade2` OAuth; plugin component-model migration + marketplace
  (ADR-0008 future work); beam-search memoization (bench margins are
  huge); Genesis birth simulation (explicit scope decision).
