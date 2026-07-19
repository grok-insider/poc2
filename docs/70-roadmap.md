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
panel with five tabs: **Goal** (the craft target as a "the item is
done" stash-search string — per-spec terms, per-mod-group tier→roll
floors, precision-first skipping of unidentifiable mods), **Item mods**
(free pool selection with roll floors + negated unwanted group),
**Waystone** and **Tablet** (the 0.5 data-gap pools — wanted/unwanted
map- and tablet-mod strings), **Vendor**
(class/rarity/ilvl/movement/resists/attributes/mod-family shopping
filters). Shortest-unique fragments are computed at runtime against the
live bundle pool (digit-free, roll-safe, `^`/`$` anchors, same-group
tiers never disqualify a line), so patterns can never drift from the
data; assembly enforces the game's 250-char budget. Bun-tested
(exhaustive digit-range verification + no-false-positive property
tests).

### Automated data refresh (ADR-0012) ✅

`poc2-pipeline watch` (patch pointer + RePoE hash detection),
`diff-bundle` markdown changelogs, `audit-matrix` legality sweeps, and
the `data-watch.yml` cron workflow that opens draft PRs against `dev`.

### Trained models in the WASM engine (M16.4 wiring) ✅

`Engine.loadTrainedModels` merges `train-advisor` artefacts (schema-
guarded — stale bundles are refused and planning stays heuristic) and
`recommend` passes the cache to the planner; the worker loads the
optional `/trained-models.json` static asset at boot; the topbar shows a
⚛ chip with the model count. Validated end-to-end with a 51-goal smoke
train against the shipped 0.5 bundle (non-degenerate `V_path`). The
artefact is an **operator asset** (never committed): produce it with
`train-advisor --bundle … --out …` and copy to
`apps/web/public/trained-models.json`.

### Distilled Emotion advisor candidates ✅

Shipped by the M14 audit (base-name matching through
`PlanInput.base_registry`) — the old "needs base-level fidelity" note
was stale. Now pinned by a live-bundle regression test
(`live_bundle_proposes_emotion_on_matching_jewel_base`); the live-bundle
smoke tests find the web app's local bundle asset when present (they
skip-pass where no bundle exists, e.g. CI).
Still open upstream: 5 Ancient-emotion targets stay display-only until
RePoE exports their mods.

### Plugin re-wire phases 1 + 2 (ADR-0014) ✅

Decision: **browser-side JS host** (wasmtime can't run in wasm32;
blocking IPC to Electron main can't serve a sync planner). Phase 1:
plugin `.wasm` files managed from Settings → Plugins (IndexedDB
persisted), instantiated sandboxed (no imports), SDK emission exports
(`list_strategies`/`list_rules`) fed to `Engine.setPluginContent` (set
semantics, warn-and-skip per document). Phase 2: **live custom
predicates** — plugin instances live in the engine worker, the engine
calls a synchronous JS dispatch (`Engine.setPluginDispatch`) during
planning, guarded by strike-based auto-disable (ADR-0008's perf
contract, post-hoc). Verified in-browser end-to-end with the SDK-built
example plugin (its emitted rule fires through its own predicate).
Shipping this surfaced and fixed a latent v1 **SDK arena bug** that
trapped every real emission call (absolute addresses used as `Vec`
indices; only WAT fixtures had ever been tested).

### 0.5 data-gap pools: Waystones / Tablets / Relics / Flasks / Charms / Ultimatum ✅

The pipeline now ingests the non-gear crafting surfaces RePoE exports
(they were always real data — the old filter just dropped everything
outside `domain == "item"`): **Waystones** (class `Map`, 16 tier bases +
108 mods ≈ poe2db's 109), **Precursor Tablets** (`TowerAugmentation`,
81 ≈ 83), **Sanctum Relics** (136 ≈ 139), **Life/Mana Flasks** (57 + 52;
57 exact), **Charms** (`UtilityFlask`, 51 exact), and **Inscribed
Ultimatum** (31 exact). Pool isolation is pinned per surface at
ingestion (`pool_domain_classes`) because many surface mods spawn on
`default` with positive weight — the naive tag→class derivation leaked
85 of them into the Ring pool. Guarded by
`pipeline/tests/live_bundle_pools.rs` (counts + both-way isolation);
audit-matrix now sweeps 39 classes / 2,059 checks with 0 failures.
Clipboard import learns the in-game display names ("Waystones",
"Precursor Tablets", "Charms", "Inscribed Ultimatum"). Sanctified
Relics genuinely have no craftable pool upstream (0 is correct).
Bundle: **4,100 mods / 3,843 bases / 75 classes**.

### 0.5 price-id mappings ✅

`default_id_mapping` covers all 13 Verisium Alloys + 26 Distilled
Emotions (kebab-slug convention), cross-checked against the shipped
bundle's catalogues by `crates/market/tests/id_mapping_05.rs`.

### Overlay + OCR polish ✅

The `/overlay` route hydrates the persisted calibrated region on mount
(`getCaptureRegion` bridge call) — the first hotkey scan no longer races
the calibration push. Item-screenshot OCR now shares the overlay's
vendored origin-relative `/ocr/` runtime (one Tesseract setup, works
over `app://`). Portal restore-token reuse is **blocked upstream**
(Electron doesn't expose the ScreenCast token; field stays reserved).

### Release metadata ✅ (partial)

`release.yml` now syncs `apps/desktop/package.json` to the released
version before packaging, and the owner gate accepts both candidate orgs
(`grok-insider`, `anomalyco`). Still open: actually settling the
canonical org (see below).

## Current / Next

Ordered by expected value; none are started unless noted.

- [x] **On-demand goal solving (ADR-0015, shipped):** trained policies
  are no longer corpus-bounded — `Engine.recommend` analytically solves
  any cache-missing `(goal, item-class)` in the worker (sub-second) and
  caches the exact policy pair; league/plugin-dispatch changes clear the
  cache. The corpus artefact is now an optional **warm start** (operator
  action, ~30 s): `cargo run --release --bin train-advisor -- --corpus
  pipeline/data/training_goals.toml --bundle <0.5 bundle> --out …`
  (default `--model analytic`; the Monte Carlo path survives as
  `--model mc` for cross-validation), copy to
  `apps/web/public/trained-models.json`. Since the training-quality pass
  (artefact schema v2), all 42 trainable corpus goals converge to a
  finite `V_path(s0)`; 9 goals are **audit-dropped** with per-spec reasons
  (Block on BodyArmour/Helmet/Staff suffixes, the `Armour` concept missing
  entirely, Sceptre minion / Amulet attack / Ring caster concepts absent
  per class) — these are upstream taxonomy gaps, tracked with the data
  gaps below.
- [ ] **Alloys in the solver action set:** add goal-relevant **Verisium
  Alloys** to `training::solve::enumerate_solver_actions` (they're
  runtime candidates already; Swift Alloy is the only CastSpeed-on-Gloves
  source, so `gloves-cast-speed-es` stays audit-dropped until then).
  Consider a CI/release lane that publishes the warm-start artefact
  alongside releases.
- [ ] **Plugin phase 3 (ADR-0014):** recommendation emitters — needs a
  candidate-source hook in the planner (`PlanInput` only carries
  predicate dispatch today), then the JS host wires
  `emit_recommendations`; plus a capability-manifest approval UI.
- [ ] **Remaining data gaps** (blocked on upstream data + curation — not
  guessable): "Thrud's Might" weapon mechanic; Preserved Vertebrae
  (waystone desecration — the waystone pool now exists, the bone path
  doesn't); Breach Ring quality caps (40/45); Essence of the Abyss
  granting only one of its two Mark mods per class; Vaal Catalysing
  Infuser; the 5 Expedition Saga omens; Expedition Logbooks (21 — no
  craftable pool in RePoE's export yet). Waystone/Tablet/Relic/Flask/
  Charm/Ultimatum pools shipped (see above).
- [ ] **Canonical GitHub org decision** (`anomalyco` vs `grok-insider`):
  the release workflow now tolerates both; once decided, align Cargo
  metadata, docs, and tighten the gate back to one owner.

## Deferred / out of scope (unchanged decisions)

- Cachix binary cache; Hardcore/SSF support; macOS support; self-hosted
  data pipeline; empirical weight derivation from trade samples; MCTS
  advisor upgrade; real Wayland layer-shell overlay (ADR-0009); GGG
  `/trade2` OAuth; plugin component-model migration + marketplace
  (ADR-0008 future work); beam-search memoization (bench margins are
  huge); Genesis birth simulation (explicit scope decision).
