# Path of Crafting 2 Agent Notes

## Project State

- Path of Crafting 2 (`poc2`) is a crafting advisor for Path of Exile 2. The advisor is a Rust engine that runs **in the browser via WebAssembly**, driven by a Next.js 16 + React 19 web app (`apps/web`) and optionally wrapped by an **Electron desktop shell** (`apps/desktop`). Current game target is patch **0.5 "Return of the Ancients"** (league: Runes of Aldur, live since 2026-05-29); the shipped bundle is **schema v3 / patch 0.5.0**.
- **History in one line:** v1.0 (patch 0.4, all milestones M1–M8 + Phases A–G) → the Tauri 2 + Svelte 5 desktop app was replaced by the web/WASM app → **v2** crafter-helper pass (`docs/80`, shipped) → **v3** engine-training pass (`docs/81`, shipped) → **crafting-mechanics fidelity + 0.5 content** pass (`docs/83` P0–P6 + P5, shipped) → **Electron desktop shell + capture + price checking** (ADR-0010, shipped) → **OCR price overlay + desktop price cache** (ADR-0013, shipped).
- Work integrates on **`dev`** and ships from **`master`** (see Branch & Release Flow). The `v1.0.0` tag is local at `afa7a04`; no git remote is currently configured on this machine.
- The crafting-fidelity pass added: item-level-dependent pools, inclusive higher-tier weighting (`ModRegistry::inclusive_weight_for`), the keep-≥1-tier Min-Mod-Level exception, patch-versioned floors (`MinModLevelVariant`), explicit `ModDefinition.tier` ordinals, tag-intersection weighting (`weight_for_on_base`), desecration bone-size/lord-omen fidelity, a `League` ruleset on `ApplyContext` (threaded through `PlanInput.league` **and** user-switchable from Settings via the WASM `setLeague`), and cross-version gating (`recombinator_available`, Corruption-omen Standard-only). **Bundle schema is v3.**
- The 0.5 content pass added: `ModKind::Crafted` + the 0.5 1-crafted/1-desecrated mod caps; patch-gated Vaal/Sanctification multiply-instead-of-reroll; **Verisium Alloys end-to-end** (13 alloys × 132 class-targets from `pipeline/data/alloys.json`, advisor candidates via `CurrencyResolver::alloys()`); **Distilled Emotions** (26 emotions × 96 jewel-base targets, `pipeline/data/emotions.json`, base-targeted `Alloy::with_base_targets`); the jewel mod pool (`ModDomain::Jewel`); and the **Genesis Tree panel** (`apps/web/components/GenesisPanel.tsx` — real tree nodes from the committed `pipeline/data/brequel_tree.json` + curated `genesis_meta.toml` presets, served by WASM `genesisTree`; UI-only, no birth simulation). Genesis art lives in `apps/web/public/genesis-icons/` (regenerate with `cargo run -p poc2-pipeline --bin fetch-genesis-assets`).
- **Desktop (ADR-0010 + ADR-0013, shipped):** a cross-platform **Electron windowed app** (`apps/desktop`) wraps the web export over a privileged `app://` scheme — a normal window like Discord, NOT an overlay (ADR-0009's layer-shell deferral stands). It adds: item capture (global hotkey → the game's own Ctrl+C → clipboard → import; per-platform backends, APT semantics; `--capture`/`--scan`/`--recalibrate` second-instance flags for Wayland compositor binds), trade2 price checking (main-process proxy, header-driven rate limiting, pipeline-generated mod→trade-stat-id table `apps/web/public/trade-stats.json`), an hourly **poe2scout price cache** (node:sqlite, poe.ninja fallback rows), and the capability-gated **screen-region OCR price overlay** (`/overlay` + `/calibrate` routes; full click-through window on win32/X11/GNOME-KDE-probe-pass, in-app degraded panel on Hyprland/wlroots). Targets: Linux, NixOS, Windows 11 (macOS out of scope). The plain browser app remains fully supported — desktop features detect the bridge and degrade gracefully. There is also a standalone Hyprland capture daemon (`crates/capture`, ADR-0011) the web app reaches over `ws://127.0.0.1:17771/ws`.
- Source-of-truth docs: `README.md` (user-facing), `CHANGELOG.md`, `docs/70-roadmap.md` (current/next work), and the ADRs. `docs/72`, `docs/80`, `docs/81`, and `docs/90` are **historical** plan documents.
- **UI design system: `apps/web/DESIGN.md`** — the web app mimics the PoE2 *in-game* interface (Fontin fonts, black/bronze/gold panels, game-exact rarity colors, `.poe-pop` item popups, Breach-style Genesis tooltips). All new UI must follow it; GGG art is fetched (regenerable, gitignored) via `fetch-genesis-assets` / `fetch-base-icons`, never committed.
- **Known gaps (the honest list — also mirrored in the roadmap):**
  - Trained models are **wired** (worker fetches the optional `/trained-models.json`; `Engine.loadTrainedModels` → `PlanInput.trained_models`; ⚛ topbar chip), but the **production-scale retrain** (`--samples 100000`) on the 0.5 bundle is pending — the artefact is an operator asset, never committed (smoke-quality artefacts exist locally under `~/.config/poc2/cache/trained_models/`).
  - Plugins: **phase 1 wired** (ADR-0014 browser-side JS host — Settings → Plugins, strategy/rule emission via `Engine.setPluginContent`). **Phase 2 pending**: live custom predicates + recommendation emitters (`plugin_dispatch` is still `None` during planning; `ItemPredicate::Custom` evaluates to false).
  - No progressive/streaming recommendations: `replan()` is a single `recommend` call with stale-token discard.
  - 5 Ancient-emotion targets are display-only until RePoE exports their mods (emotion advisor candidates themselves shipped — pinned by `live_bundle_proposes_emotion_on_matching_jewel_base`).
  - Genesis birth simulation stays intentionally out of scope.

## Stack

- Rust workspace, edition 2021, minimum Rust `1.82`.
- Web app in `apps/web`: Next.js 16 + React 19, TypeScript, static export (`output: 'export'`), plain CSS modules (no Tailwind), Zustand store, IndexedDB persistence (`idb-keyval`), `tesseract.js` for OCR.
- WASM boundary in `crates/poc2-wasm` (wasm-bindgen, `cdylib`): an `Engine` exposing `recommend`, `parse`, `eligibleMods`, `checkCanApply`, `recordOutcome`, `rerollableMods`, `runNTrials`, `recoveryHints`, `listBases`, `listDatabaseEntries`, `databaseEntryDetail`, `league`/`setLeague`, `applyPrices`, `applyNinjaPrices`, `resolveName`, `genesisTree`, `loadTrainedModels`/`trainedModelCount`, `setPluginContent`. Built by `bun run wasm` (`scripts/build-wasm.mjs`: cargo wasm32 → wasm-bindgen `--target web` → `wasm-opt -Oz`) into `apps/web/lib/wasm` + `apps/web/public/wasm`.
- Desktop shell in `apps/desktop`: Electron main + preload, TypeScript, electron-builder packaging (AppImage/deb + NSIS). Bridge contract: `window.poc2Desktop` (`apps/web/lib/desktop.ts` ⇄ `apps/desktop/src/preload.ts`).
- `crates/market` networking is behind a `net` feature (off by default at the workspace dep level) so the engine stays WASM-clean; enable with `--features net` for native price fetches.
- **Bun** is the web package manager + script runner. The repo root is a **Bun workspace** (root `package.json` + single root `bun.lock`) listing `apps/web` **and** `apps/desktop`, so `bun install` and `bun run <dev|build|typecheck|lint|wasm|test:web|test:desktop|desktop:*>` all run **from the repo root**. Node is kept for tooling compatibility.
- Nix flake development; prefer `nix develop` before running toolchains. The flake provides the Rust toolchain (+ `wasm32-unknown-unknown`), `wasm-pack`/`wasm-bindgen-cli`/`binaryen`, `bun`/`nodejs_22`, and `electron`.
- Core crates: `engine`, `data`, `strategies`, `rules`, `probability`, `market`, `advisor`, `parser`, `plugin-host`, `plugin-sdk`, `poc2-wasm`, `capture` (ADR-0011 Hyprland item-capture daemon; binary, excluded from `default-members`), and `pipeline`.

## Platform Assumptions

- The web app runs in any modern browser (WebAssembly + Web Workers); development uses the Nix flake. Platform-specific code (`process.platform`, `cfg(target_os)`) is allowed **only** in `apps/desktop` and explicitly-marked operator tooling — engine/data/advisor/parser/market/wasm/web stay platform-free.
- Cross-platform rules (ADR-0010): never resolve paths via `HOME` alone — use the `XDG_CONFIG_HOME → HOME → USERPROFILE/APPDATA` chain (see `crates/market/src/cache.rs`); build scripts that end users or CI need must run on Windows (Bun scripts, not bash — `scripts/build-wasm.mjs`); `.gitattributes` enforces LF so `include_str!` bytes match across OSes.
- Trade integration: browser sessions use URL deep links (`window.open`); the desktop shell proxies `trade2` search/fetch through the Electron main process with header-driven rate limiting. GGG OAuth stays out of scope (public trade2 endpoints need no session).
- **Browser-side state** persists in IndexedDB (`apps/web/lib/persist.ts`, via `idb-keyval`): the craft item/goal/history, notes, league + engine league, last item text, and saved recipes.
- **Build/operator artifacts** live under `~/.config/poc2/` on the dev machine: data bundles (`bundles/`) and trained-model caches (`cache/trained_models/`). The web app ships a bundle as a static asset (`apps/web/public/poc2.bundle.json.gz`).
- Dropped with the Tauri app (no browser equivalent): the Client.txt live watcher. Live item **capture** is the Electron shell (hotkey → Ctrl+C → clipboard) or the ADR-0011 capture daemon, not Client.txt.

## Product Baseline

- Advisor: beam-search planner with Monte Carlo aggregation and `prob_stderr` confidence intervals. A re-plan is a single `recommend` call answered off the UI thread (no progressive streaming); the store token-discards superseded plans.
- Data: patch-versioned hot-swappable bundle (schema v3) from RePoE-fork, Craft of Exile, poe2db.tw, and curated fixtures; poe2scout + poe.ninja provide live prices at runtime.
- Strategies: **43 TOML strategies** (`crates/strategies/strategies/`).
- Rules: **142 TOML production rules across 16 section files** (`crates/rules/seed_rules/`).
- UI (web, `apps/web/components/`): item editor (paste/capture/manual + screenshot OCR), target editor (concept palette + archetypes), guide (hero recommendation + alternatives + success band + recovery hints), eligible-mods inspector, history (with undo), database browser (bases + materials), **price check panel** (trade2 stat filters + live listings/deep link), **Regex panel** (in-game search-string generator: goal / item-mods / vendor tabs — see below), **Genesis Tree panel**, tools (simulation runner + recipe library), settings (league, engine league, prices, capture, notes, data/reset), and the outcome dialog (add/remove/reroll/rarity). Extra routes: `/overlay` (OCR price overlay) and `/calibrate` (region calibration) for the desktop shell.
- **Regex generator** (`apps/web/lib/regex/` + `RegexPanel*`; inspired by poe2.re, which is UNLICENSED — clean-room reimplementation, never copy its code/JSON): pure-TS, bun-tested. `numberRegex` (≥N / [min,max] digit patterns, exhaustively tested), `shortestUnique` (shortest fragment unique within the eligible pool's `text_template` lines; digit-free, `#`-roll-safe, `^`/`$` anchors), `modTerms` (goal-spec + mod terms; tier floors become per-mod-group roll floors; precision-first — unidentifiable mods are skipped with warnings), `searchString` (quoted AND / `|` OR / `!` NOT assembly, 250-char budget), `vendor` (hand-authored shopping patterns + class-prefix computation), `state` (non-persisted zustand UI store). Corpus comes from the store's cached bare-item `eligibleMods` response — never hardcode mod text.
- Item import: paste in-game-copied text into the Item panel (`navigator.clipboard` → Rust `parse`), desktop capture hotkey, capture-daemon push, or screenshot OCR.
- Market: poe2scout live prices (browser fetch or desktop proxy) + poe.ninja exchange as a parallel source, applied to the valuator via `applyPrices`/`applyNinjaPrices`; the desktop keeps an hourly sqlite price cache with poe.ninja fallback. All soft-fail; planning never depends on them.

## Feature And Docs Map

- Product overview and status: `README.md`, `CHANGELOG.md`, `docs/70-roadmap.md`.
- Architecture overview: `docs/40-architecture.md`; domain model: `docs/30-domain-model.md`; engine algorithms: `docs/31-engine-algorithms.md`; cross-version gates: `docs/14-crafting-mechanics-cross-version.md`.
- Advisor planning, scoring, and Monte Carlo behavior: `docs/35-advisor-architecture.md`, `docs/32-probability-math.md`, `crates/advisor/`, `crates/probability/`.
- Rules and decision synthesis: `docs/34-heuristics-rulebook.md`, `docs/36-decision-engine.md`, `crates/rules/`, `crates/rules/seed_rules/`.
- Strategy DSL and strategy library: `docs/33-strategy-library.md`, `crates/strategies/`.
- Recovery flows: `docs/37-recovery-flows.md`, strategy TOML recovery hints, `apps/web/components/GuidePanel.tsx` (engine reach: `recoveryHints`).
- UI flows and React components: `docs/41-ui-flows.md`, `apps/web/components/` (one panel per workflow: `ItemEditor`, `TargetEditor`, `GuidePanel`, `EligibleTab`, `HistoryTab`, `DatabasePanel`, `PricePanel`, `RegexPanel` (+ `RegexGoalTab`/`RegexModsTab`/`RegexVendorTab`/`RegexResult`), `GenesisPanel`, `ToolsPanel`, `SettingsPanel`, `OutcomeDialog`), the `Console` shell, and the Zustand store `apps/web/lib/store.ts`.
- Engine boundary and RPC: `crates/poc2-wasm/src/lib.rs` + `crates/poc2-wasm/src/commands/` (the pure-compute command ports), the Web Worker host `apps/web/lib/engine/engine.worker.ts`, and the typed client `apps/web/lib/engine/client.ts`.
- Desktop shell: `apps/desktop/src/` (`main.ts`, `preload.ts`, `ipc.ts`, `serve.ts`, `capture/`, `trade/`, `prices/`), contract mirror `apps/web/lib/desktop.ts`, docs in `apps/desktop/README.md` + ADR-0010/0011/0013.
- Data bundles and source joins: `pipeline/`, `pipeline/scripts/`, `crates/data/`; bundle output normally lives under `~/.config/poc2/bundles/`. Automated refresh: `.github/workflows/data-watch.yml` (ADR-0012).
- Market integrations: `docs/51-market-meta.md`, `crates/market/`, `apps/desktop/src/prices/`.
- Plugin system: `crates/plugin-sdk/` (guest macros), `crates/plugin-host/` (native/test host + ABI reference), the browser host `apps/web/lib/plugins/` (ADR-0014 phase 1: emission only), `examples/plugins/`, ADR-0008 + ADR-0014.
- Hyprland integration: `examples/hyprland/`, ADR-0009; do not add non-Hyprland compositor branches unless requested — the GlobalShortcuts portal path is the generic one.
- Roadmap and scope decisions: `docs/70-roadmap.md`, ADRs in `docs/adr/` (14 records).

## Future Work And Deferred Scope

- Use `docs/70-roadmap.md`'s "Current / Next" section as the source of truth for unfinished work. Checked boxes in `docs/72` and earlier roadmap milestones are historical.
- Current candidates (see roadmap): production-scale advisor retrain (`--samples 100000`, operator compute run — the wiring shipped); plugin **phase 2** (live custom predicates + recommendation emitters per ADR-0014); remaining data gaps (Waystones/Tablets/Relics/Flasks/Charms pools, "Thrud's Might", Vertebrae, Breach Ring quality caps, Essence of the Abyss dual-mod — blocked on upstream data + curation, which also gates the Regex panel's Waystone/Tablet tabs); canonical GitHub org decision.
- Do not treat these as current tasks unless the user asks: Cachix binary cache, Hardcore/SSF support, macOS support, self-hosted data pipeline, empirical weight derivation from trade samples, MCTS advisor upgrade, real Wayland layer-shell overlay (deferred by ADR-0009), GGG `/trade2` OAuth, plugin component-model migration, beam-search memoization.
- Cross-platform scope was changed by the user on 2026-06-10: Linux + NixOS + Windows 11 are supported targets (ADR-0010 supersedes ADR-0002). macOS remains out of scope.
- Plugin marketplace/signature verification, UI panel plugins, and currency plugins are future work in ADR-0008.

## App Flow Pointers

- Item state comes from the manual item editor, pasted item text, the desktop capture hotkey, the capture daemon, or screenshot OCR; the craft state (item/goal/history/notes/leagues) persists to IndexedDB and restores on load.
- The target editor defines desired concepts, tiers, hybrid behavior, and budget; any item/goal/risk/depth change triggers a re-plan (`apps/web/lib/store.ts`, token-guarded).
- Advisor recommendations combine strategy steps, rule outputs, market prices, and probability simulation; the worker answers each `recommend` call off the UI thread.
- Recovery hints appear for strategy-sourced recommendations and come from strategy step recovery metadata (`recoveryHints`).
- Recipes save/load `(item, goal)` pairs in IndexedDB (`apps/web/lib/persist.ts`).
- Settings handles league selection (market league + engine League ruleset), best-effort price refresh (re-plans on success), desktop price-cache status, capture diagnostics, and per-project notes.
- Trade search opens PoE2 trade deep links via `window.open` in browsers; the desktop shell runs the query through the trade2 proxy and renders listings.

## Plugin SDK

- Runtime is `wasmtime` in `crates/plugin-host` (raw-Wasm `(ptr, len)` ABI — the Component Model is future work), with capability gating, per-plugin sandboxing/fuel, and a predicate dispatch cache.
- SDK macros live in `crates/plugin-sdk`: `declare_predicate!`, `declare_strategies!`, `declare_rules!`, `declare_recommendation_emitter!`. Example plugin: `examples/plugins/predicate-ilvl-min/`.
- `ItemPredicate::Custom` can be referenced from rule and strategy TOML; without a live dispatch it evaluates to false.
- **App wiring (ADR-0014)**: phase 1 runs a browser-side JS host (`apps/web/lib/plugins/` — sandboxed `WebAssembly.instantiate`, no imports; emission exports → `Engine.setPluginContent`, set semantics). During planning the WASM engine still passes `plugin_dispatch: None` — live predicates/emitters are phase 2.
- Important known debugging note: do not enable `epoch_interruption(true)` without setting deadlines; this caused immediate Wasmtime traps in plugin tests and was disabled.

## Common Commands

- Build Rust workspace: `cargo build --workspace`.
- Test Rust workspace: `cargo test --workspace`.
- Format Rust: `cargo fmt --all --check`.
- Lint Rust: `cargo clippy --workspace --all-targets -- -D warnings`.
- Run advisor benchmarks: `cargo bench --bench advisor_plan`.
- Build the WASM engine: `bun run wasm` (`scripts/build-wasm.mjs` — cargo wasm32 → wasm-bindgen `--target web` → `wasm-opt`, warn-and-skip when absent); outputs to `apps/web/lib/wasm` + `apps/web/public/wasm`. Re-run after any change to `crates/poc2-wasm` or the crates it re-exports.
- Install web deps (from repo root): `bun install`.
- Typecheck web (from root): `bun run typecheck` (`tsc --noEmit`, strict).
- Run web dev server (from root): `bun run dev` (real browser at :3000; pass args through, e.g. `bun run dev --port 4000`).
- Build web / static export (from root): `bun run build` (→ `apps/web/out/`).
- Lint web (from root): `bun run lint` (ESLint 9 flat config — `next lint` was removed in Next 16).
- Web unit tests (from root): `bun run test:web` (bun test, `apps/web/lib/__tests__/`).
- Desktop app (from root): `bun run test:desktop`, `bun run desktop:typecheck`, `bun run desktop:dev` (Electron against the dev server — start `bun run dev` first), `bun run desktop:start` (Electron serving `apps/web/out` over `app://` — run `bun run build` first). On NixOS the launcher picks the devshell's `electron`; packaging (`dist:linux`/`dist:win` in `apps/desktop`) runs in CI on FHS runners, not on NixOS.
- Windows dev (no Nix): rustup honors `rust-toolchain.toml` (incl. wasm32 target); install Bun + `wasm-bindgen-cli@0.2.117` (must match Cargo.lock), then the same `bun run` commands work.
- Build data bundle: `cargo run --release -p poc2-pipeline -- build --out ~/.config/poc2/bundles/poc2.bundle.json.gz --patch 0.5.0` (0.5.0 is also the CLI default).
- Check for new upstream game data (ADR-0012): `cargo run -p poc2-pipeline -- watch` (exit `0` = no change, `10` = change; `--write` persists `pipeline/data/upstream_state.json`). The live PoE2 game version is `4.5.x` = patch `0.5.x`. Diff two bundles: `cargo run -p poc2-pipeline -- diff-bundle <old.gz> <new.gz> --out diff.md`. Automated by `.github/workflows/data-watch.yml` (cron → rebuild → diff → draft PR against `dev`).
- Audit the crafting surface against a bundle: `cargo run -p poc2-pipeline --bin audit-matrix -- --bundle <bundle.gz> --out audit-matrix.json`.
- Train advisor models (smoke ~10 min): `cargo run --release --bin train-advisor -- --corpus pipeline/data/training_goals.toml --bundle ~/.config/poc2/bundles/poc2.bundle.json.gz --out ~/.config/poc2/cache/trained_models/poc2-trained-models-0.5.0.json --samples 10000 --verbose`. The training patch follows the bundle's header.
- Train advisor models (production ~hours): same command with `--samples 100000`. Artefacts land in `~/.config/poc2/cache/trained_models/`; the planner consults them via `PlanInput.trained_models` (native/tests only — the WASM engine does not load them yet). **Always pass `--bundle`** — without it the binary trains against an empty synthetic registry and every goal's `V_path(s0)` degenerates to the value-iteration floor (`-1000`). Add `--strict-audit` in CI to fail-fast when corpus goals reference concepts the bundle's mod taxonomy doesn't carry.

## Verification Expectations

- For Rust-only changes, run the narrow crate tests first, then `cargo test --workspace` when feasible.
- For web `lib/` logic changes, run `bun run test:web` in addition to typecheck.
- For desktop (`apps/desktop`) changes, run its typecheck + unit tests, then a launch smoke (window opens, renderer loads, bridge responds) before claiming done.
- Windows CI lane runs without Nix (rustup + Bun); don't introduce build steps that only work inside `nix develop` for anything end users or CI need.
- For changes to `crates/poc2-wasm` (or crates it re-exports), also rebuild the WASM (`bun run wasm`) and confirm `cargo build -p poc2-wasm --target wasm32-unknown-unknown --release` is clean.
- For web changes, run `bun run typecheck` (from root); run `bun run build` (static export) when changing bundled UI behavior.
- For engine-boundary changes (new/changed `Engine` method), update the typed client `apps/web/lib/engine/client.ts` and the `lib/types.ts` contract, then typecheck.
- For release-sensitive changes, run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, and (from root) `bun run typecheck && bun run build`.
- Do not require live network services in ordinary tests; poe2scout/poe.ninja integrations should soft-fail or use fixtures.

## Editing Guidance

- Keep changes small and consistent with the existing crate boundaries.
- Prefer TOML strategy/rule data changes over hard-coding behavior when updating advisor knowledge.
- Preserve scope decisions in the ADRs unless the user explicitly asks to reopen them (ADR-0002's platform scope was reopened by ADR-0010; ADR-0009's overlay deferral still stands — the desktop app is a window, and the ADR-0013 price overlay is a plain Electron window, not layer-shell).
- Linux capture support targets Hyprland first (`hyprctl sendshortcut`, `ydotool` fallback); don't build per-compositor branches for others unless requested — the GlobalShortcuts portal path is the generic one.
- Do not remove or rewrite user state paths under `~/.config/poc2/` without an explicit migration plan.
- Treat generated data bundles and large reference repos as artifacts; avoid committing new large data unless intentionally part of the task.

## Architecture Conventions (from the 2026-06 readiness audit)

- **Boundary layers hold no crafting logic.** `crates/poc2-wasm/src/commands/*` are thin ports: deserialize → call engine/advisor → serialize. Capacity, eligibility, and outcome semantics always come from the engine (the hardcoded-3/3-slots bug in `outcome.rs` is the cautionary tale).
- **Currency mechanics share one kernel.** Sampling/removal helpers live in `crates/engine/src/currency/common.rs`; new mechanics import from there — never copy helpers out of `basic.rs` or from each other.
- **Item-class resolution has exactly one path:** `BaseRegistry::resolve_item_class` (handles real `BaseTypeId` metadata paths AND legacy class-id placeholders). Never `ItemClassId::from(item.base)` directly — captured items carry real base ids and will silently misclassify.
- **`Currency::apply` is atomic on failure** — the orchestrator (`apply_currency_with_bases`) snapshots and restores the item on `Err`. Preserve that invariant when adding apply paths or callers.
- **Parser must carry rolled values** through both formats (basic Ctrl+C and advanced); tier resolution picks the tier whose range contains the rolls. Capture and price checking depend on this.
- **Web state:** persistence is middleware in the store (subscribe-based), not manual `persist()` calls. External item text enters through `ingestExternalItemText` (the capture seam); the desktop bridge contract is `apps/web/lib/desktop.ts` (`window.poc2Desktop`) and the web app never imports Electron.
- **Web runtime asset URLs stay origin-relative** (`/wasm/...`, `/base-icons/...`, `/ocr/...`): the desktop shell serves the export over a privileged `app://` scheme. Never assume `http://localhost` or hardcode origins.
- **Web tests run under `bun test`** (`apps/web/lib/__tests__/`). New pure logic in `apps/web/lib/` gets tests; new engine-boundary methods get a typed `client.ts` + `types.ts` mirror and a typecheck run.
- **Desktop app (`apps/desktop`)**: Electron main + preload only — capture, trade2 proxy, price cache, overlay/calibration windows, window/tray. No crafting logic, no React. Per-platform code branches on `process.platform` inside dedicated backend modules (`capture/win32.ts`, `capture/linux.ts`), never inline.
- **`example-repos/` licensing**: MIT repos (Exiled-Exchange-2, awakened-poe-trade) may inform ports with attribution; GPL/AGPL repos (XileHUD, POE2_HTC, ggpk-explorer) are read-only references — never copy code.
- **File-size guidance**: a module pushing past ~700 lines of non-test code or hosting a second responsibility gets split (see `GenesisPanel.tsx`, `OutcomeDialog.tsx` as known offenders to split when next touched).

## Release Notes

- v1.0 performance baselines from `CHANGELOG.md`: `plan_depth_3_top_3_mc50` around 139 µs and `plan_depth_5_width_8` around 151 µs on the recorded benchmark machine.
- Current verification baseline: `cargo test --workspace` green plus, from the repo root, `bun run typecheck` / `bun run lint` / `bun run test:web` / `bun run test:desktop` / `bun run build` clean.
- Known metadata wrinkle: `[workspace.package].version` is `1.0.0` (release-plz owns it), `apps/desktop/package.json` is versioned independently for packaging, and Cargo repo metadata points at `github.com/anomalyco/poc2` while `release.yml` gates on the `grok-insider` org — align these when the canonical remote is decided.

## Branch & Release Flow

- **Branches:** `master` is the released branch; `dev` is the integration branch; all work happens on typed feature branches — `feat/…`, `fix/…`, `ci/…`, `docs/…`, `release/…` — cut from `dev`. **Never push to `master` directly.** Flow: feature branch → PR into `dev` → a single `dev → master` PR ships. `.github/workflows/guard-master.yml` enforces "only `dev` (or `release-plz-*`) may PR into `master`". The data-watch auto-refresh PRs target `dev` too.
- **Conventional Commits are required** (`feat:` → minor, `fix:` → patch, `feat!:`/`BREAKING CHANGE:` → major; `docs/refactor/perf/test/chore/ci` don't trigger a release). The commit history drives automated versioning + the changelog — see `CONTRIBUTING.md`.
- **Releases are automated via release-plz** (`release-plz.toml` + `.github/workflows/release.yml`). On every push to `master`, release-plz keeps a release PR open (bumps the single `[workspace.package].version`, refreshes `Cargo.lock`, regenerates `CHANGELOG.md`); the `grok-insider/release-changelog-action` rewrites that PR's notes with AI-written prose. **Merging the release PR ships it** — tags `vX.Y.Z` (anchored on `poc2-engine`), creates the GitHub Release, and attaches the Electron desktop packages (Windows NSIS + Linux AppImage/deb). **Do not hand-bump versions or hand-edit the CHANGELOG `[Unreleased]` block** once release-plz owns it; let the release PR do it.
- One-time GitHub setup is documented in `CONTRIBUTING.md` (secrets `OPENROUTER_API_KEY` + `RELEASE_PLZ_TOKEN`, the "allow Actions to create PRs" toggle, and `master` branch protection).
