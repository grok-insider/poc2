# Path of Crafting 2 — Overview

> Project vision, scope, and locked decisions. This doc is the entry point for new contributors.

## Vision

A native desktop **crafting advisor** for Path of Exile 2 that doesn't just simulate — it *plans*. Given the user's current item, target mods, budget, and stash, the app proposes the optimal next action with full re-plan on every state change, branching recovery flows when steps fail, and live market awareness so cost rankings reflect today's prices.

## What sets this apart

Existing PoE2 crafting tools cluster into three buckets:

1. **Simulators** (Craft of Exile, pathofcrafting.net) — show probabilities for a fixed plan, web-based, no in-game integration.
2. **Optimal-path tools** (POE2HTC, pyoe2-craftpath) — algorithmic, but headless / rough UX.
3. **Overlays** (XileHUD, Exiled Exchange 2) — in-game tools, no probability/sim engine.

Path of Crafting 2 fuses all three:

- Beam-search optimal-path planner over a curated **strategy library** + **heuristic rule engine** (~120 expert rules).
- Live state tracking via **clipboard** capture (Ctrl+C in PoE2 → `Item` struct in <100ms).
- Wayland-native overlay (`wlr-layer-shell`) for Hyprland.
- Market-aware cost ranking (poe2scout / poe.ninja).
- Recovery branches surfaced when an action's outcome was a failure.

## Scope (v1)

| In scope | Out of scope |
|---|---|
| Crafting simulator + planner | Hardcore/SSF (v2) |
| Strategy library + rule engine | Build planner (use PoB-PoE2) |
| Beam-search advisor | Stash analyzer (use poe.ninja) |
| Recovery flows | Bot/automation (forbidden by GGG ToS) |
| Clipboard item capture | Mobile (Tauri mobile is unproven) |
| Wayland overlay (Hyprland) | macOS/Windows ports (v2+) |
| poe.ninja meta-build awareness | Trade automation |
| Trade integration (OAuth) | Currency flipping |
| Hot-swappable patch-versioned data bundles | Custom stash UI |

## Locked decisions

| # | Decision | Choice | ADR |
|---|---|---|---|
| 1 | Project name | "Path of Crafting 2" — collision with `pathofcrafting.net` documented and accepted | — |
| 2 | Tech stack | **Tauri 2** + **Rust** core + **Svelte 5** UI | [ADR-0001](adr/0001-tech-stack.md) |
| 3 | Platform v1 | **NixOS + Hyprland only**; Wayland-native; flake-packaged | [ADR-0002](adr/0002-platform-nixos-only.md) |
| 4 | v1 scope | Full overlay-style tool: sim + capture + trade + overlay + advisor | — |
| 5 | License | **MIT** | — |
| 6 | Data model | Hybrid offline baseline + online auto-update via `poc2-data` pipeline | — |
| 7 | Mod weights | Craft of Exile primary + poe2db cross-check, confidence-flagged | [ADR-0004](adr/0004-weight-strategy.md) |
| 8 | Reference repos | All 9 cloned to `example-repos/` (gitignored) | — |
| 9 | Patch baseline | **0.4 "Fate of the Vaal"** (released Dec 12 2025); architecture patch-versioned from line 1 to absorb 0.5 (May 29 2026) | [ADR-0006](adr/0006-patch-versioning.md) |
| 10 | Advisor sophistication | **Full beam-search optimal-path** + Monte Carlo ranking + risk slider | [ADR-0007](adr/0007-advisor-beam-search.md) |
| 11 | Recovery handling | Full re-plan on every state change | — |
| 12 | Meta awareness | Full integration (poe.ninja meta builds, league-day phase, off-meta finder, pricing) | — |
| 13 | Live craft tracking | Clipboard polling on Ctrl+C | — |
| 14 | League modes | Trade league only in v1 | — |
| 15 | Strategy authoring | Full plugin system (Wasm-sandboxed); deferred to v1.1 | [ADR-0008](adr/0008-plugin-system-deferred.md) |

## Brand collision note

`pathofcrafting.net` is an existing branded "Path of Crafting" web tool, actively maintained for PoE2. We accept the name collision for now. Mitigations:

- Binary / package names use `poc2` (e.g., `poc2-desktop`, `crates/poc2-engine`)
- The Tauri identifier is `ai.anomaly.poc2`
- Documentation refers to "Path of Crafting 2" but UI title bar can be tuned later
- If the collision becomes an issue, rename is a config change, not a code change

## Repository layout

```
poc2/
├── Cargo.toml                 # workspace root
├── flake.nix                  # NixOS dev shell + package
├── rust-toolchain.toml        # pinned Rust channel
├── apps/
│   └── desktop/               # Tauri 2 app
│       ├── package.json       # Svelte 5 frontend
│       ├── src/               # frontend code
│       └── src-tauri/         # Rust IPC bridge
├── crates/
│   ├── engine/                # M2 — domain model + apply()
│   ├── data/                  # M2 — bundle loader + schema
│   ├── strategies/            # M3 — strategy library DSL
│   ├── rules/                 # M3 — heuristic rule engine
│   ├── probability/           # M5 — Monte Carlo + EV math
│   ├── market/                # M5 — valuator + meta + prices
│   └── advisor/               # M4 — beam-search planner
├── pipeline/                  # M2 — data bundle builder (separate binary)
├── docs/                      # this directory
├── example-repos/             # reference clones (gitignored, see README inside)
├── scripts/
│   └── clone-example-repos.sh
└── .github/workflows/ci.yml
```

## Milestones

See [70-roadmap.md](70-roadmap.md) for the phased build plan.

| Milestone | Focus | Status |
|---|---|---|
| M1 | Foundation (flake, workspace, Tauri skeleton, CI, docs) | ✅ done |
| M2 | Engine core + data pipeline | ⏳ next |
| M3 | Strategy library + rule engine | |
| M4 | Advisor (beam-search) | |
| M5 | Probability + market | |
| M6 | UI v1 | |
| M7 | Live integration / overlay | |
| M8 | Polish + release | |
| M9+ | Plugin system (Wasm) | |

## Quick start (development)

```bash
# 1. Direnv recommended
direnv allow                          # uses flake automatically

# OR manual
nix develop                           # enter dev shell

# 2. Inside the shell
cargo build --workspace               # build all Rust crates
cargo test --workspace                # run tests
cargo clippy --workspace -- -D warnings

# 3. Frontend
cd apps/desktop
pnpm install
pnpm tauri:dev                        # start the app in dev mode

# 4. Clone reference repos (one-time, ~1.5 GB)
./scripts/clone-example-repos.sh
```

## License

MIT. See [60-licensing.md](60-licensing.md) for the per-source license audit (data sources have their own terms; some are non-commercial).
