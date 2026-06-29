# Path of Crafting 2 Agent Notes

## Project State

- Path of Crafting 2 (`poc2`) is a crafting advisor for Path of Exile 2. The advisor is a Rust engine that runs **in the browser via WebAssembly**, driven by a Next.js 16 + React 19 web app (`apps/web`). v1.0 targeted patch 0.4 "Fate of the Vaal"; post-v1 work is migrating it toward **0.5 "Return of the Ancients"** (live since 2026-05-29).
- **UI rebuild (current):** the original Tauri 2 + Svelte 5 desktop app (`apps/desktop`) has been **replaced** by a Next.js web app (`apps/web`) that hosts the engine as WASM (`crates/poc2-wasm`) in a Web Worker. Reason: the desktop webview was effectively unclickable under the target compositor; a real browser fixes it and removes all native/Wayland coupling from the UI itself. Full feature parity was reached before the desktop app was removed. (ADR-0010 later reintroduced deliberate, *isolated* native coupling — capture and trade proxying — confined to the new Electron shell `apps/desktop`.) Implementation plan: `docs/90-ui-redesign.md` (the "Forge" design) + the gleaming-watching-lightning plan.
- `main` is **well past the `v1.0.0` tag** (local tag at `4861ad1`, may be unpushed). Since v1: the **v2** crafter-helper pass (`docs/80`, shipped), the **v3** engine-training pass (`docs/81`, shipped), and an in-progress **crafting-mechanics fidelity + 0.5** pass (`docs/83`, `docs/14`).
- The crafting-fidelity pass (P0–P6 shipped) added: item-level-dependent pools, inclusive higher-tier weighting (`ModRegistry::inclusive_weight_for`), the keep-≥1-tier Min-Mod-Level exception, patch-versioned floors (`MinModLevelVariant`), explicit `ModDefinition.tier` ordinals, tag-intersection weighting (`weight_for_on_base`), desecration bone-size/lord-omen fidelity, a `League` ruleset on `ApplyContext` (threaded through `PlanInput.league` **and** user-switchable from Settings via the WASM `setLeague`), and cross-version gating (`recombinator_available`, Corruption-omen Standard-only). **Bundle schema is now v3.**
- The 0.5 content pass (shipped): `ModKind::Crafted` + the 0.5 1-crafted/1-desecrated mod caps; patch-gated Vaal/Sanctification multiply-instead-of-reroll; **Verisium Alloys end-to-end** (13 alloys × 132 class-targets from `pipeline/data/alloys.json`, advisor candidates via `CurrencyResolver::alloys()`); **Distilled Emotions** (26 emotions × 96 jewel-base targets, `pipeline/data/emotions.json`, base-targeted `Alloy::with_base_targets`); the jewel mod pool (371 mods, `ModDomain::Jewel`); and the **Genesis Tree panel** (`apps/web/components/GenesisPanel.tsx` — 248 real tree nodes from the committed `pipeline/data/brequel_tree.json` + curated `genesis_meta.toml` presets, served by WASM `genesisTree`; UI-only, no birth simulation). Genesis art lives in `apps/web/public/genesis-icons/` (regenerate with `cargo run -p poc2-pipeline --bin fetch-genesis-assets`).
- **Desktop direction (ADR-0010, 2026-06-10, explicit user decision):** a cross-platform **Electron windowed desktop app** (`apps/desktop`) wraps the web app — a normal window like Discord, NOT an overlay. It adds: item capture (global hotkey → the game's own Ctrl+C → clipboard → import; per-platform backends, APT semantics), price checking (trade2 API proxied through Electron main; pipeline-generated mod→trade-stat-id table), and live price snapshots into the planner (`applyPrices`). Supported targets: Linux, NixOS, Windows 11 (macOS out of scope). The plain browser app remains fully supported — desktop features detect the bridge and degrade gracefully.
- Source-of-truth docs: `README.md` (user-facing), `CHANGELOG.md` (the `[Unreleased]` section tracks the post-v1 work), and the plan docs above. The v1 execution-plan boxes in `docs/72` are historical.
- **UI design system: `apps/web/DESIGN.md`** — the web app mimics the PoE2 *in-game* interface (Fontin fonts, black/bronze/gold panels, game-exact rarity colors, `.poe-pop` item popups, Breach-style Genesis tooltips). All new UI must follow it; GGG art is fetched (regenerable, gitignored) via `fetch-genesis-assets`, never committed.
- Remaining fidelity work: production-scale advisor retrain (`--samples 100000`) on the current 0.5 bundle; advisor candidate proposals for Emotions (engine apply works, but candidates need base-level item fidelity); the 5 Ancient-emotion targets whose mods RePoE doesn't export yet (display-only until upstream ships them); Genesis birth simulation stays intentionally out of scope.

## Stack

- Rust workspace, edition 2021, minimum Rust `1.82`.
- Web app in `apps/web`: Next.js 16 + React 19, TypeScript, static export (`output: 'export'`), plain CSS modules (no Tailwind), Zustand store, IndexedDB persistence (`idb-keyval`).
- WASM boundary in `crates/poc2-wasm` (wasm-bindgen, `cdylib`): an `Engine` exposing `recommend`, `parse`, `eligibleMods`, `checkCanApply`, `recordOutcome`, `rerollableMods`, `runNTrials`, `recoveryHints`, `listBases`, `listDatabaseEntries`, `databaseEntryDetail`. Built by `scripts/build-wasm.sh` (cargo wasm32 → wasm-bindgen `--target web` → `wasm-opt -Oz`) into `apps/web/lib/wasm` + `apps/web/public/wasm`.
- `crates/market` networking is behind a `net` feature (off by default at the workspace dep level) so the engine stays WASM-clean; enable with `--features net` for native price fetches.
- **Bun** is the web package manager + script runner. The repo root is a **Bun workspace** (root `package.json` + single root `bun.lock`) listing `apps/web`, so `bun install` and `bun run <dev|build|typecheck|lint|wasm>` all run **from the repo root** (each web script delegates into `apps/web`). Node is kept for tooling compatibility.
- Nix flake development; prefer `nix develop` before running toolchains. The flake provides the Rust toolchain (+ `wasm32-unknown-unknown`), `wasm-pack`/`wasm-bindgen-cli`/`binaryen`, and `bun`/`nodejs_22`.
- Core crates include `engine`, `data`, `strategies`, `rules`, `probability`, `market`, `advisor`, `parser`, `plugin-host`, `plugin-sdk`, `poc2-wasm`, `capture` (ADR-0011 Hyprland item-capture daemon; binary, excluded from `default-members`), and `pipeline`.

## Platform Assumptions

- The web app runs in any modern browser (WebAssembly + Web Workers); development uses the Nix flake. Platform-specific code (`process.platform`, `cfg(target_os)`) is allowed **only** in `apps/desktop` and explicitly-marked operator tooling — engine/data/advisor/parser/market/wasm/web stay platform-free.
- Cross-platform rules (ADR-0010): never resolve paths via `HOME` alone — use the `XDG_CONFIG_HOME → HOME → USERPROFILE/APPDATA` chain (see `crates/market/src/cache.rs`); build scripts that end users or CI need must run on Windows (Bun scripts, not bash — `scripts/build-wasm.mjs`); `.gitattributes` enforces LF so `include_str!` bytes match across OSes.
- Trade integration: browser sessions use URL deep links (`window.open`); the desktop shell proxies `trade2` search/fetch through the Electron main process with header-driven rate limiting. GGG OAuth stays out of scope (public trade2 endpoints need no session).
- **Browser-side state** persists in IndexedDB (`apps/web/lib/persist.ts`, via `idb-keyval`): the craft item/goal/history, notes, league, and saved recipes — the replacement for the desktop's `~/.config/poc2/state.toml` + `recipes/*.toml`.
- **Build/operator artifacts** still live under `~/.config/poc2/` on the dev machine: data bundles (`bundles/`) and trained-model caches (`cache/trained_models/`). The web app ships a bundle as a static asset (`apps/web/public/poc2.bundle.json.gz`).
- Dropped with the Tauri app (no browser equivalent): the Client.txt live watcher and the in-process Wasm plugin host. Live item **capture** returns via the Electron shell (hotkey → Ctrl+C → clipboard), not via Client.txt.

## Product Baseline

- Advisor: beam-search planner with Monte Carlo aggregation, `prob_stderr` confidence intervals, and streaming depth 1, depth 3, and final recommendations.
- Data: patch-versioned hot-swappable bundle from RePoE-fork, Craft of Exile, poe2db.tw, poe2scout, and poe.ninja where applicable.
- Strategies: 23 TOML strategies with full `docs/33-strategy-library.md` coverage.
- Rules: 113 TOML production rules across 14 section files with full `docs/34-heuristics-rulebook.md` coverage.
- UI (web, `apps/web/components/`): item editor, target editor, guide (hero recommendation + alternatives + success band), eligible-mods inspector, history (with undo), recovery panel, database browser (bases + materials), tools (simulation runner + recipe library), settings (league, notes, prices), and the outcome dialog (add/remove/reroll/rarity). Trade search opens PoE2 trade deep links via `window.open`.
- Item import: paste in-game-copied text into the Item panel (`navigator.clipboard` → Rust `parse`).
- Market: poe2scout live prices and poe.ninja PoE2 meta/off-meta helper with soft-fail behavior; browser price fetches are best-effort (CORS may block) and planning never depends on them.

## Feature And Docs Map

- Product overview and feature matrix: `README.md`, `CHANGELOG.md`, `docs/72-v1-execution-plan.md`.
- Architecture overview: `docs/40-architecture.md`; domain model: `docs/30-domain-model.md`; engine algorithms: `docs/31-engine-algorithms.md`.
- Advisor planning, scoring, streaming, and Monte Carlo behavior: `docs/35-advisor-architecture.md`, `docs/32-probability-math.md`, `crates/advisor/`, `crates/probability/`.
- Rules and decision synthesis: `docs/34-heuristics-rulebook.md`, `docs/36-decision-engine.md`, `crates/rules/`, `crates/rules/seed_rules/`.
- Strategy DSL and strategy library: `docs/33-strategy-library.md`, `crates/strategies/`.
- Recovery flows: `docs/37-recovery-flows.md`, strategy TOML recovery hints, `apps/web/components/RecoveryPanel.tsx` (engine reach: `recoveryHints`).
- UI flows and React components: `docs/41-ui-flows.md`, `apps/web/components/` (one panel per workflow: `ItemEditor`, `TargetEditor`, `GuidePanel`, `EligibleTab`, `HistoryTab`, `DatabasePanel`, `ToolsPanel`, `SettingsPanel`, `OutcomeDialog`), the `Console` shell, and the Zustand store `apps/web/lib/store.ts`.
- Engine boundary and RPC: `crates/poc2-wasm/src/lib.rs` + `crates/poc2-wasm/src/commands/` (the pure-compute command ports), the Web Worker host `apps/web/lib/engine/engine.worker.ts`, and the typed client `apps/web/lib/engine/client.ts`. Clipboard import is `navigator.clipboard` → `parse`; recipe persistence + state are IndexedDB (`apps/web/lib/persist.ts`).
- Data bundles and source joins: `pipeline/`, `pipeline/scripts/`, `crates/data/`; bundle output normally lives under `~/.config/poc2/bundles/`.
- Market integrations: `docs/51-market-meta.md`, `crates/market/`; poe2scout is used for live prices and leagues, poe.ninja PoE2 data is used for meta/off-meta hints.
- Plugin system: `crates/plugin-host/`, `crates/plugin-sdk/`, `examples/plugins/`, ADR-0008; installed plugins live under `~/.config/poc2/plugins/`.
- Hyprland integration: `examples/hyprland/`, ADR-0009; do not add non-Hyprland compositor support for v1 unless requested.
- Roadmap and scope decisions: `docs/70-roadmap.md`, ADRs in `docs/adr/`.

## Future Work And Deferred Scope

- Use the `M9+ Post-v1` section of `docs/70-roadmap.md` as the source of truth for unfinished/future work. Earlier unchecked boxes in `docs/70-roadmap.md` and `docs/72-v1-execution-plan.md` can be historical and may have been completed by the v1 Phase A-G pass.
- Do not treat these as current tasks unless the user asks to work on them: Cachix binary cache, Hardcore/SSF support, macOS support, self-hosted data pipeline, empirical weight derivation from trade samples, MCTS advisor upgrade, real Wayland layer-shell overlay, GGG `/trade2` OAuth, plugin component-model migration, and beam-search memoization for heavier plugin workloads. (Windows 11 support, the Electron desktop shell, and price checking moved INTO scope on 2026-06-10 — see ADR-0010 and roadmap M10.)
- Real Wayland layer-shell overlay is deferred by ADR-0009; v1 uses Hyprland `windowrulev2` recipes instead.
- GGG `/trade2` OAuth is not implemented in v1; v1 uses URL-only trade deep links.
- Cross-platform scope WAS changed by the user on 2026-06-10: Linux + NixOS + Windows 11 are supported targets (ADR-0010 supersedes ADR-0002). macOS remains out of scope.
- Plugin marketplace/signature verification, UI panel plugins, and currency plugins are post-v1 future work in ADR-0008.
- Beam-search memoization was intentionally deferred because v1 benchmark numbers are well under budget; revisit only if real workloads or plugin expansion make planning slow.

## App Flow Pointers

- Item state comes from the manual item editor or pasting in-game-copied item text; the craft state (item/goal/history/notes/league) persists to IndexedDB and restores on load.
- The target editor defines desired concepts, tiers, hybrid behavior, and budget; any item/goal/risk/depth change debounce-triggers a re-plan (`apps/web/lib/store.ts`).
- Advisor recommendations combine strategy steps, rule outputs, market prices, and probability simulation; the worker answers each `recommend` call off the UI thread.
- Recovery hints appear for strategy-sourced recommendations and come from strategy step recovery metadata (`recoveryHints`).
- Recipes save/load `(item, goal)` pairs in IndexedDB (`apps/web/lib/persist.ts`).
- Settings handles league selection, best-effort price refresh, and per-project notes.
- Trade search opens PoE2 trade deep links via `window.open`.

## Plugin SDK

- The Wasm plugin SDK ships in v1.
- Runtime is `wasmtime` in `crates/plugin-host`, with capability gating, per-plugin sandboxing/fuel, and predicate dispatch cache.
- SDK macros live in `crates/plugin-sdk`, including `declare_predicate!`, `declare_strategies!`, `declare_rules!`, and `declare_recommendation_emitter!`.
- `ItemPredicate::Custom` can be referenced from rule and strategy TOML.
- Example plugin: `examples/plugins/predicate-ilvl-min/`.
- Important known debugging note: do not enable `epoch_interruption(true)` without setting deadlines; this caused immediate Wasmtime traps in plugin tests and was disabled for v1.

## Common Commands

- Build Rust workspace: `cargo build --workspace`.
- Test Rust workspace: `cargo test --workspace`.
- Format Rust: `cargo fmt --all --check`.
- Lint Rust: `cargo clippy --workspace --all-targets -- -D warnings`.
- Run advisor benchmarks: `cargo bench --bench advisor_plan`.
- Build the WASM engine: `bun run wasm` (or `bash scripts/build-wasm.sh`) — cargo wasm32 → wasm-bindgen `--target web` → `wasm-opt`; outputs to `apps/web/lib/wasm` + `apps/web/public/wasm`. Re-run after any change to `crates/poc2-wasm` or the crates it re-exports.
- Install web deps (from repo root): `bun install`.
- Typecheck web (from root): `bun run typecheck` (`tsc --noEmit`, strict).
- Run web dev server (from root): `bun run dev` (real browser at :3000; pass args through, e.g. `bun run dev --port 4000`).
- Build web / static export (from root): `bun run build` (→ `apps/web/out/`).
- Lint web (from root): `bun run lint` (ESLint 9 flat config — `next lint` was removed in Next 16).
- Web unit tests (from root): `bun run test:web` (bun test, `apps/web/lib/__tests__/`).
- Desktop app (from root): `bun run test:desktop`, `bun run desktop:typecheck`, `bun run desktop:dev` (Electron against the dev server — start `bun run dev` first), `bun run desktop:start` (Electron serving `apps/web/out` over `app://` — run `bun run build` first). On NixOS the launcher picks the devshell's `electron`; packaging (`dist:linux`/`dist:win` in `apps/desktop`) runs in CI on FHS runners, not on NixOS.
- Windows dev (no Nix): rustup honors `rust-toolchain.toml` (incl. wasm32 target); install Bun + `wasm-bindgen-cli@0.2.117` (must match Cargo.lock), then the same `bun run` commands work — `scripts/build-wasm.mjs` is cross-platform (wasm-opt optional, warn-and-skip).
- Build data bundle: `cargo run --release -p poc2-pipeline -- build --out ~/.config/poc2/bundles/poc2.bundle.json.gz --patch 0.4.0`.
- Check for new upstream game data (ADR-0012): `cargo run -p poc2-pipeline -- watch` (exit `0` = no change, `10` = change; `--write` persists `pipeline/data/upstream_state.json`). Detects new PoE2 patches via RePoE-fork's `poe2/version.txt` + the SHAs of the three consumed RePoE files. The live PoE2 game version is `4.5.x` = patch `0.5.x` (e.g. `4.5.4.x` ↔ 0.5.4). Diff two bundles for a "what's new" changelog: `cargo run -p poc2-pipeline -- diff-bundle <old.gz> <new.gz> --out diff.md`. Automated by `.github/workflows/data-watch.yml` (cron → rebuild → diff → draft PR; curated fixtures still need human curation).
- Train advisor models (smoke ~10 min): `cargo run --release --bin train-advisor -- --corpus pipeline/data/training_goals.toml --bundle ~/.config/poc2/bundles/poc2.bundle.json.gz --out ~/.config/poc2/cache/trained_models/poc2-trained-models-0.4.0.json --samples 10000 --verbose`.
- Train advisor models (production ~hours): same command with `--samples 100000`. Artefacts land in `~/.config/poc2/cache/trained_models/`; the planner consults them via `PlanInput.trained_models`. **Always pass `--bundle`** — without it the binary trains against an empty synthetic registry and every goal's `V_path(s0)` degenerates to the value-iteration floor (`-1000`). Add `--strict-audit` in CI to fail-fast when corpus goals reference concepts the bundle's mod taxonomy doesn't carry.

## Verification Expectations

- For Rust-only changes, run the narrow crate tests first, then `cargo test --workspace` when feasible.
- For web `lib/` logic changes, run `bun test` (from `apps/web`) in addition to typecheck.
- For desktop (`apps/desktop`) changes, run its typecheck + unit tests, then a launch smoke (window opens, renderer loads, bridge responds) before claiming done.
- Windows CI lane runs without Nix (rustup + Bun); don't introduce build steps that only work inside `nix develop` for anything end users or CI need.
- For changes to `crates/poc2-wasm` (or crates it re-exports), also rebuild the WASM (`bash scripts/build-wasm.sh`) and confirm `cargo build -p poc2-wasm --target wasm32-unknown-unknown --release` is clean.
- For web changes, run `bun run typecheck` (from root); run `bun run build` (static export) when changing bundled UI behavior.
- For engine-boundary changes (new/changed `Engine` method), update the typed client `apps/web/lib/engine/client.ts` and the `lib/types.ts` contract, then typecheck.
- For release-sensitive changes, run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, and (from root) `bun run typecheck && bun run build`.
- Do not require live network services in ordinary tests; poe2scout/poe.ninja integrations should soft-fail or use fixtures.

## Editing Guidance

- Keep changes small and consistent with the existing crate boundaries.
- Prefer TOML strategy/rule data changes over hard-coding behavior when updating advisor knowledge.
- Preserve scope decisions in the ADRs unless the user explicitly asks to reopen them (ADR-0002's platform scope was reopened by ADR-0010; ADR-0009's overlay deferral still stands — the desktop app is a window, not an overlay).
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
- **Web runtime asset URLs stay origin-relative** (`/wasm/...`, `/base-icons/...`): the desktop shell serves the export over a privileged `app://` scheme. Never assume `http://localhost` or hardcode origins.
- **Web tests run under `bun test`** (`apps/web/lib/__tests__/`). New pure logic in `apps/web/lib/` gets tests; new engine-boundary methods get a typed `client.ts` + `types.ts` mirror and a typecheck run.
- **Desktop app (`apps/desktop`)**: Electron main + preload only — capture, trade2 proxy, window/tray, auto-update later. No crafting logic, no React. Per-platform code branches on `process.platform` inside dedicated backend modules (`capture/win32.ts`, `capture/linux.ts`), never inline.
- **`example-repos/` licensing**: MIT repos (Exiled-Exchange-2, awakened-poe-trade) may inform ports with attribution; GPL/AGPL repos (XileHUD, POE2_HTC, ggpk-explorer) are read-only references — never copy code.
- **File-size guidance**: a module pushing past ~700 lines of non-test code or hosting a second responsibility gets split (see `GenesisPanel.tsx`, `OutcomeDialog.tsx` as known offenders to split when next touched).

## Release Notes

- v1.0 performance baselines from `CHANGELOG.md`: `plan_depth_3_top_3_mc50` around 139 us and `plan_depth_5_width_8` around 151 us on the recorded benchmark machine.
- v1.0 verification baseline (historical, desktop era): 317 tests passing across 11 crates plus the (now-removed) Tauri desktop app, the old `pnpm check` frontend clean, `cargo fmt` clean, and clippy clean with `-D warnings`. Current baseline: `cargo test --workspace` green plus, from the repo root, `bun run typecheck` / `bun run lint` / `bun run build` clean.

## Branch & Release Flow

- **Branches (mirrors `grok-insider/open-media`):** `master` is the released branch; `dev` is the integration branch; all work happens on typed feature branches — `feat/…`, `fix/…`, `ci/…`, `docs/…`, `release/…` — cut from `dev`. **Never push to `master` directly.** Flow: feature branch → PR into `dev` → a single `dev → master` PR ships. `.github/workflows/guard-master.yml` enforces "only `dev` (or `release-plz-*`) may PR into `master`". The data-watch auto-refresh PRs target `dev` too.
- **Conventional Commits are required** (`feat:` → minor, `fix:` → patch, `feat!:`/`BREAKING CHANGE:` → major; `docs/refactor/perf/test/chore/ci` don't trigger a release). The commit history drives automated versioning + the changelog — see `CONTRIBUTING.md`.
- **Releases are automated via release-plz** (`release-plz.toml` + `.github/workflows/release.yml`). On every push to `master`, release-plz keeps a release PR open (bumps the single `[workspace.package].version`, refreshes `Cargo.lock`, regenerates `CHANGELOG.md`); the `grok-insider/release-changelog-action` rewrites that PR's notes with AI-written prose. **Merging the release PR ships it** — tags `vX.Y.Z` (anchored on `poc2-engine`), creates the GitHub Release, and attaches the Electron desktop packages (Windows NSIS + Linux AppImage/deb). **Do not hand-bump versions or hand-edit the CHANGELOG `[Unreleased]` block** once release-plz owns it; let the release PR do it.
- One-time GitHub setup is documented in `CONTRIBUTING.md` (secrets `OPENROUTER_API_KEY` + `RELEASE_PLZ_TOKEN`, the "allow Actions to create PRs" toggle, and `master` branch protection).
