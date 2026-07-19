# Path of Crafting 2 — Overview

> Project vision, scope, and locked decisions. This doc is the entry point
> for new contributors. Current as of the 0.5 "Return of the Ancients"
> passes (see `CHANGELOG.md` + `docs/70-roadmap.md`).

## Vision

A **crafting advisor** for Path of Exile 2 that doesn't just simulate — it
*plans*. Given the user's current item, target mods, and budget, the app
proposes the optimal next action with full re-plan on every state change,
branching recovery flows when steps fail, and live market awareness so
cost rankings reflect today's prices.

The advisor is a Rust engine compiled to **WebAssembly** running entirely
client-side in a browser (Next.js web app), optionally wrapped by an
**Electron desktop shell** that adds in-game item capture and live trade
price checking.

## What sets this apart

Existing PoE2 crafting tools cluster into three buckets:

1. **Simulators** (Craft of Exile, pathofcrafting.net) — show probabilities for a fixed plan, no planning, no in-game integration.
2. **Optimal-path tools** (POE2HTC, pyoe2-craftpath) — algorithmic, but headless / rough UX.
3. **Trade companions** (Awakened PoE Trade, Exiled Exchange 2, XileHUD) — in-game capture + pricing, no probability/sim engine.

Path of Crafting 2 fuses all three:

- Beam-search optimal-path planner over a curated **strategy library**
  (43 TOML recipes) + **heuristic rule engine** (142 rules), with Monte
  Carlo confidence bands and an offline-trained Q-policy layer for long
  spam-loop crafts.
- High-fidelity engine: ilvl-dependent pools, inclusive tier weighting,
  patch-versioned Min-Mod-Level floors, cross-version (0.3/0.4/0.5) and
  league (Standard/Challenge) gating, and the full 0.5 systems
  (Verisium Alloys, Distilled Emotions, Genesis Tree data).
- Live state tracking via **item capture** (desktop hotkey → the game's
  own Ctrl+C → import) or clipboard paste.
- Market-aware cost ranking (poe2scout + poe.ninja) and real **trade2
  price checking** (desktop-proxied official API).

## Scope

| In scope | Out of scope |
|---|---|
| Crafting simulator + planner | Hardcore/SSF rule variants |
| Strategy library + rule engine | Build planner (use PoB-PoE2) |
| Beam-search advisor + trained policy | Stash analyzer |
| Recovery flows | Bot/automation (forbidden by GGG ToS) |
| Item capture (hotkey / clipboard / OCR) | Trade automation / currency flipping |
| trade2 price checking (public API) | GGG OAuth |
| poe2scout / poe.ninja live prices | Genesis birth simulation |
| Hot-swappable patch-versioned data bundles | macOS support |
| Linux + NixOS + Windows 11 desktop app | Wayland layer-shell overlay (deferred, ADR-0009) |

## Locked decisions

| # | Decision | Choice | ADR |
|---|---|---|---|
| 1 | Project name | "Path of Crafting 2" — collision with `pathofcrafting.net` documented and accepted | — |
| 2 | Tech stack | **Rust engine as WASM** in a Next.js 16 + React 19 web app; **Electron** desktop shell for native features | [ADR-0001](adr/0001-tech-stack.md) (amended), [ADR-0010](adr/0010-desktop-shell-electron-cross-platform.md) |
| 3 | Platform | **Linux + NixOS + Windows 11** for the desktop app; any modern browser for the web app; macOS out | [ADR-0010](adr/0010-desktop-shell-electron-cross-platform.md) (supersedes [ADR-0002](adr/0002-platform-nixos-only.md)) |
| 4 | Overlay | Normal window, NOT an in-game overlay; layer-shell deferred; the ADR-0013 price overlay is a plain Electron window | [ADR-0009](adr/0009-defer-wayland-layer-shell-to-v1-1.md), [ADR-0013](adr/0013-item-capture-ocr-overlay.md) |
| 5 | License | **MIT** | [ADR-0005](adr/0005-license-mit.md) |
| 6 | Data model | Offline baseline bundle + automated upstream refresh via `poc2-pipeline` | [ADR-0012](adr/0012-automated-data-refresh.md) |
| 7 | Mod weights | Craft of Exile primary + poe2db cross-check, confidence-flagged | [ADR-0004](adr/0004-weight-strategy.md) |
| 8 | Patch versioning | Every entity carries a `PatchRange`; gates resolve from `(PatchVersion, League)` | [ADR-0006](adr/0006-patch-versioning.md), `docs/14` |
| 9 | Advisor sophistication | Beam-search optimal-path + Monte Carlo ranking + risk slider + trained-policy uplift | [ADR-0007](adr/0007-advisor-beam-search.md) |
| 10 | Recovery handling | Full re-plan on every state change | — |
| 11 | Live craft tracking | Item capture (hotkey Ctrl+C) + clipboard paste; Client.txt watching dropped | ADR-0010/0011 |
| 12 | League modes | Trade league (Challenge) default; Standard switchable in Settings | `docs/14` |
| 13 | Plugins | Wasm SDK + wasmtime host built and tested; **not currently wired into the browser/Electron planning path** (roadmap) | [ADR-0008](adr/0008-plugin-system-deferred.md) |

## Brand collision note

`pathofcrafting.net` is an existing branded "Path of Crafting" web tool.
We accept the name collision. Mitigations: binary / package names use
`poc2` (`poc2-desktop`, `crates/poc2-engine`), the Electron appId is
`ai.anomaly.poc2`, and a rename stays a config change, not a code change.

## Repository layout

```
poc2/
├── Cargo.toml                 # Rust workspace root
├── package.json               # Bun workspace root (apps/web + apps/desktop)
├── flake.nix                  # Nix dev shell (Rust + wasm toolchain + Bun + electron)
├── rust-toolchain.toml        # pinned Rust channel (+ wasm32 target)
├── apps/
│   ├── web/                   # Next.js 16 + React 19 web app (static export)
│   │   ├── app/               # routes: / (console), /overlay, /calibrate
│   │   ├── components/        # one panel per workflow + Console shell
│   │   ├── lib/               # store, engine RPC client/worker, trade, OCR, persist
│   │   └── public/            # wasm, data bundle, trade-stats.json, fetched art
│   └── desktop/               # Electron shell: capture, trade2 proxy, price cache, overlay windows
├── crates/
│   ├── engine/                # domain model + apply() + currency mechanics
│   ├── data/                  # bundle loader + schema (v3)
│   ├── strategies/            # strategy DSL + 43 TOML strategies
│   ├── rules/                 # rule engine + 142 seed rules (16 files)
│   ├── probability/           # Monte Carlo + EV math
│   ├── market/                # valuator + poe2scout/poe.ninja (net feature-gated)
│   ├── advisor/               # beam-search planner + training consumption
│   ├── parser/                # in-game item text parser
│   ├── plugin-host/           # wasmtime plugin runtime (not app-wired yet)
│   ├── plugin-sdk/            # guest-side plugin macros
│   ├── poc2-wasm/             # wasm-bindgen Engine boundary
│   └── capture/               # ADR-0011 Hyprland capture daemon (binary)
├── pipeline/                  # data bundle builder + train-advisor + audit-matrix + fetchers
├── docs/                      # this directory (+ adr/)
├── example-repos/             # reference clones (gitignored)
└── .github/workflows/         # ci, release (release-plz), data-watch, guard-master
```

## Status

See [`docs/70-roadmap.md`](70-roadmap.md). Everything through the 0.5
content pass, the Electron desktop shell (ADR-0010), and the OCR price
overlay (ADR-0013) has shipped; the roadmap's "Current / Next" section
lists what's open (trained-model wiring, emotion candidates, plugin
re-wiring, remaining data pools).

## Quick start (development)

```bash
# 1. Direnv recommended
direnv allow                          # uses flake automatically
# OR manual
nix develop

# 2. Rust
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings

# 3. Web app (Bun workspace at the repo root — engine runs as WebAssembly)
bun install
bun run wasm                          # build crates/poc2-wasm → apps/web/{lib,public}/wasm
bun run dev                           # http://localhost:3000

# 4. Desktop shell (optional)
bun run desktop:dev                   # against the dev server

# 5. Clone reference repos (one-time, ~1.5 GB)
./scripts/clone-example-repos.sh
```

## License

MIT. Data sources have their own terms — see
[`adr/0003-data-sources.md`](adr/0003-data-sources.md) and
[`adr/0005-license-mit.md`](adr/0005-license-mit.md).
