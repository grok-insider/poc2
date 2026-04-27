# Path of Crafting 2 Agent Notes

## Project State

- Path of Crafting 2 (`poc2`) is a v1.0-complete native desktop crafting advisor for Path of Exile 2 patch 0.4, "Fate of the Vaal".
- Current release tag: local `v1.0.0` on `main` at `4861ad1`; the tag may not have been pushed.
- The project status in `README.md` is the user-facing source of truth. `CHANGELOG.md` records the v1.0 release contents.
- v1.0 ships all phases A-G from `docs/72-v1-execution-plan.md`, although that execution-plan document may still contain historical unchecked boxes.

## Stack

- Rust workspace, edition 2021, minimum Rust `1.82`.
- Tauri 2 desktop app in `apps/desktop/src-tauri`.
- Svelte 5 + Vite + TypeScript frontend in `apps/desktop`.
- Nix flake development and release flow; prefer `nix develop` before running toolchains on a clean machine.
- Core crates include `engine`, `data`, `strategies`, `rules`, `probability`, `market`, `advisor`, `parser`, `plugin-host`, `plugin-sdk`, and `pipeline`.

## Platform Assumptions

- v1 supports NixOS + Hyprland only.
- Always-on-top behavior is handled by Hyprland `windowrulev2` examples in `examples/hyprland/`.
- Wayland layer-shell is intentionally deferred to v1.1 by ADR-0009.
- Trade integration is URL-only for v1; OAuth is not part of v1.
- User state persists under `~/.config/poc2/`, including `state.toml`, recipes, bundles, and plugins.

## Product Baseline

- Advisor: beam-search planner with Monte Carlo aggregation, `prob_stderr` confidence intervals, and streaming depth 1, depth 3, and final recommendations.
- Data: patch-versioned hot-swappable bundle from RePoE-fork, Craft of Exile, poe2db.tw, poe2scout, and poe.ninja where applicable.
- Strategies: 23 TOML strategies with full `docs/33-strategy-library.md` coverage.
- Rules: 113 TOML production rules across 14 section files with full `docs/34-heuristics-rulebook.md` coverage.
- UI: item builder, target panel, recovery panel, settings panel, recipe library, simulation runner, plugin manager, bundle reload, league dropdown, price refresh, and trade URL search.
- Live integration: clipboard import and Client.txt watcher using `notify`.
- Market: poe2scout live prices and poe.ninja PoE2 meta/off-meta helper with soft-fail behavior.

## Feature And Docs Map

- Product overview and feature matrix: `README.md`, `CHANGELOG.md`, `docs/72-v1-execution-plan.md`.
- Architecture overview: `docs/40-architecture.md`; domain model: `docs/30-domain-model.md`; engine algorithms: `docs/31-engine-algorithms.md`.
- Advisor planning, scoring, streaming, and Monte Carlo behavior: `docs/35-advisor-architecture.md`, `docs/32-probability-math.md`, `crates/advisor/`, `crates/probability/`.
- Rules and decision synthesis: `docs/34-heuristics-rulebook.md`, `docs/36-decision-engine.md`, `crates/rules/`, `crates/rules/seed_rules/`.
- Strategy DSL and strategy library: `docs/33-strategy-library.md`, `crates/strategies/`.
- Recovery flows: `docs/37-recovery-flows.md`, strategy TOML recovery hints, `apps/desktop/src/lib/RecoveryPanel.svelte`.
- UI flows and Svelte components: `docs/41-ui-flows.md`, `apps/desktop/src/lib/`, `apps/desktop/src/App.svelte`.
- Tauri commands and desktop runtime: `apps/desktop/src-tauri/src/`; use this area for clipboard import, Client.txt watcher, recipe persistence, bundle reload, plugin reload, and trade URL commands.
- Data bundles and source joins: `pipeline/`, `pipeline/scripts/`, `crates/data/`; bundle output normally lives under `~/.config/poc2/bundles/`.
- Market integrations: `docs/51-market-meta.md`, `crates/market/`; poe2scout is used for live prices and leagues, poe.ninja PoE2 data is used for meta/off-meta hints.
- Plugin system: `crates/plugin-host/`, `crates/plugin-sdk/`, `examples/plugins/`, ADR-0008; installed plugins live under `~/.config/poc2/plugins/`.
- Hyprland integration: `examples/hyprland/`, ADR-0009; do not add non-Hyprland compositor support for v1 unless requested.
- Roadmap and scope decisions: `docs/70-roadmap.md`, ADRs in `docs/adr/`.

## Future Work And Deferred Scope

- Use the `M9+ Post-v1` section of `docs/70-roadmap.md` as the source of truth for unfinished/future work. Earlier unchecked boxes in `docs/70-roadmap.md` and `docs/72-v1-execution-plan.md` can be historical and may have been completed by the v1 Phase A-G pass.
- Do not treat these as current tasks unless the user asks to work on them: Cachix binary cache, Hardcore/SSF support, Windows/macOS support, self-hosted data pipeline, empirical weight derivation from trade samples, MCTS advisor upgrade, real Wayland layer-shell overlay, GGG `/trade2` OAuth, plugin component-model migration, `tauri-plugin-opener` migration, and beam-search memoization for heavier plugin workloads.
- Real Wayland layer-shell overlay is deferred by ADR-0009; v1 uses Hyprland `windowrulev2` recipes instead.
- GGG `/trade2` OAuth is not implemented in v1; v1 uses URL-only trade deep links.
- Cross-platform support is out of v1 scope; v1 remains NixOS + Hyprland only unless the user explicitly changes the scope.
- Plugin marketplace/signature verification, UI panel plugins, and currency plugins are post-v1 future work in ADR-0008.
- Beam-search memoization was intentionally deferred because v1 benchmark numbers are well under budget; revisit only if real workloads or plugin expansion make planning slow.

## App Flow Pointers

- Item state can come from manual item builder input or clipboard import of in-game copied item text.
- The target panel defines desired concepts, tiers, hybrid behavior, and budget; persisted state is under `~/.config/poc2/state.toml`.
- Advisor recommendations combine strategy steps, rule outputs, market prices, and probability simulation, then stream shallow and deeper results back to the UI.
- Recovery hints appear when the last outcome is a failure and come from strategy step recovery metadata.
- Recipes save/load/share `(item, goal)` TOML under `~/.config/poc2/recipes/`.
- Settings handles bundle reload, league selection, price refresh, Client.txt watcher configuration, plugin manager, and off-meta craft hints.
- Trade search is URL-only and opens Path of Exile trade deep links in the default browser.

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
- Install frontend deps: `cd apps/desktop && pnpm install`.
- Check frontend: `cd apps/desktop && pnpm check`.
- Build frontend: `cd apps/desktop && pnpm build`.
- Run desktop dev app: `cd apps/desktop && pnpm tauri:dev`.
- Build data bundle: `cargo run --release -p poc2-pipeline -- build --out ~/.config/poc2/bundles/poc2.bundle.json.gz --patch 0.4.0`.
- Train advisor models (smoke ~1 min): `cargo run --release --bin train-advisor -- --corpus pipeline/data/training_goals.toml --out ~/.config/poc2/cache/trained_models/poc2-trained-models-0.4.0.json --samples 1000`.
- Train advisor models (production ~hours): same command with `--samples 100000`. The desktop loader picks up artefacts from `~/.config/poc2/cache/trained_models/` on the next bundle reload; the planner consults them via `PlanInput.trained_models`.
- Inspect trained-model cache status: call the `trained_model_status` Tauri command from the desktop UI (or via `cargo run --bin poc2-desktop` then DevTools).

## Verification Expectations

- For Rust-only changes, run the narrow crate tests first, then `cargo test --workspace` when feasible.
- For frontend changes, run `pnpm check` in `apps/desktop`; run `pnpm build` when changing bundled UI behavior.
- For Tauri command or Rust/frontend boundary changes, run both relevant Rust tests and `pnpm check`.
- For release-sensitive changes, run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, and `cd apps/desktop && pnpm check`.
- Do not require live network services in ordinary tests; poe2scout/poe.ninja integrations should soft-fail or use fixtures.

## Editing Guidance

- Keep changes small and consistent with the existing crate boundaries.
- Prefer TOML strategy/rule data changes over hard-coding behavior when updating advisor knowledge.
- Preserve v1 scope decisions unless the user explicitly asks to reopen them.
- Do not add support for non-Hyprland compositors in v1 docs or code unless requested.
- Do not remove or rewrite user state paths under `~/.config/poc2/` without an explicit migration plan.
- Treat generated data bundles and large reference repos as artifacts; avoid committing new large data unless intentionally part of the task.

## Release Notes

- v1.0 performance baselines from `CHANGELOG.md`: `plan_depth_3_top_3_mc50` around 139 us and `plan_depth_5_width_8` around 151 us on the recorded benchmark machine.
- v1.0 verification baseline: 317 tests passing across 11 crates plus the desktop app, frontend `pnpm check` clean, `cargo fmt` clean, and clippy clean with `-D warnings`.
- Push command when the user is ready: `git push origin main v1.0.0`.
