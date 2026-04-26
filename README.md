# Path of Crafting 2 (`poc2`)

A native desktop crafting **advisor** for Path of Exile 2 — beam-search optimal-path planning with full re-plan on every state change, branching recovery flows, and live market awareness.

> Status: **M1 (foundation) complete**. Engine core, data pipeline, advisor, and UI land in M2-M6. See [`docs/70-roadmap.md`](docs/70-roadmap.md).

## What it does (when finished)

- You import an in-game item via `Ctrl+C`.
- You declare your target mods and budget.
- The advisor proposes the optimal next currency / omen to use, with explanation, EV math, and confidence band.
- You apply it; the app sees the new state via clipboard and re-plans automatically.
- If the outcome was a failure (bricked roll, bad reveal, etc.), recovery branches are surfaced first.
- Live market prices from poe2scout / poe.ninja drive cost ranking.

## Platform

**v1 supports NixOS + Hyprland only.** See [`docs/adr/0002-platform-nixos-only.md`](docs/adr/0002-platform-nixos-only.md).

## Quick start

```bash
# direnv (recommended)
direnv allow

# OR enter the dev shell manually
nix develop

# Inside the shell
cargo build --workspace
cargo test --workspace

# Frontend
cd apps/desktop
pnpm install
pnpm tauri:dev
```

To clone the reference repos (~1.5 GB):

```bash
./scripts/clone-example-repos.sh
```

## Architecture

```
ADVISOR  (beam-search + Monte Carlo)
   │
   ├── STRATEGY LIBRARY  (23+ codified TOML recipes)
   ├── RULE ENGINE       (~120 forward-chained rules)
   └── PROBABILITY       (Monte Carlo + EV)
   │
   ▼
ENGINE CORE  (sub-ms apply(currency, item, omens))
   │
   ▼
DATA BUNDLE  (patch-versioned, hot-swappable)
   │
   ├── RePoE-fork    (mods, bases, tags)
   ├── Craft of Exile (weights)
   └── poe2db.tw     (omens, essences, bones)
```

See [`docs/40-architecture.md`](docs/40-architecture.md) for the full picture.

## Documentation

- [`00-overview.md`](docs/00-overview.md) — vision, scope, decisions
- [`33-strategy-library.md`](docs/33-strategy-library.md) — 23 codified strategies (research seed)
- [`34-heuristics-rulebook.md`](docs/34-heuristics-rulebook.md) — ~120 production rules (research seed)
- [`40-architecture.md`](docs/40-architecture.md) — system architecture
- [`70-roadmap.md`](docs/70-roadmap.md) — milestones M1-M9+
- [`adr/`](docs/adr/) — architecture decision records

## License

[MIT](LICENSE). Game data and weights are credited to their sources; see [`docs/adr/0003-data-sources.md`](docs/adr/0003-data-sources.md) and [`docs/adr/0005-license-mit.md`](docs/adr/0005-license-mit.md).

## Related projects

- [pathofcrafting.net](https://pathofcrafting.net/) — web crafting simulator (separate brand, not this project)
- [Craft of Exile](https://craftofexile.com/?game=poe2) — community gold-standard simulator
- [POE2HTC](https://github.com/Dboire9/POE2_HTC) (AGPL-3) — reference for beam-search
- [pyoe2-craftpath](https://github.com/WladHD/pyoe2-craftpath) (MIT) — reference + potential dependency
- [XileHUD](https://github.com/XileHUD/poe_overlay) (GPL-3) — reference for clipboard / overlay
