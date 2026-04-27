# Path of Crafting 2 (`poc2`)

A native desktop crafting **advisor** for Path of Exile 2 — beam-search
optimal-path planning with full re-plan on every state change,
branching recovery flows, live market awareness, and a Wasm plugin
SDK for community extensions.

> Status: **v1.0 release-candidate**. All 8 milestones (M1-M8) +
> Phases A-G of the v1 execution plan have shipped. See
> [`docs/72-v1-execution-plan.md`](docs/72-v1-execution-plan.md).

## What it does

- **Import an in-game item** via `Ctrl+C` (clipboard parser → engine
  state). Or build it manually in the UI.
- **Declare your target mods + budget** via the Target panel
  (concept-aware, hybrid-mod aware).
- The advisor **proposes the optimal next currency / omen** to use,
  with explanation, EV math, confidence band (Monte Carlo ± stderr),
  and full traceability back to the source rule / strategy /
  heuristic.
- You apply it; the app sees the new state via clipboard and
  **re-plans automatically** with progressively-deeper streaming
  results (depth 1 → 3 → final).
- If the outcome was a failure (bricked roll, bad reveal), **recovery
  branches are surfaced first** via the Recovery panel.
- **Live market prices** from poe2scout drive cost ranking; the
  off-meta finder surfaces niche crafting goals based on
  poe.ninja PoE2 build popularity.
- **Wasm plugin SDK** lets the community ship custom predicates,
  strategies, rules, and recommendation emitters; the advisor
  threads plugin dispatch into the planner's beam search.

## Platform

**v1 supports NixOS + Hyprland only.** See
[`docs/adr/0002-platform-nixos-only.md`](docs/adr/0002-platform-nixos-only.md)
+ [`docs/adr/0009-defer-wayland-layer-shell-to-v1-1.md`](docs/adr/0009-defer-wayland-layer-shell-to-v1-1.md).

## Quick start

```bash
# Run from the flake directly:
nix run github:anomalyco/poc2

# Or for development:
nix develop
cargo build --workspace
cargo test --workspace
cd apps/desktop && pnpm install && pnpm tauri:dev
```

To clone the reference repos (~1.5 GB):

```bash
./scripts/clone-example-repos.sh
```

### First-run setup

1. **Build a data bundle** (one-shot, ~3 minutes; needs network):
   ```bash
   cargo run --release -p poc2-pipeline -- build \
     --out ~/.config/poc2/bundles/poc2.bundle.json.gz \
     --patch 0.4.0
   ```
   The bundle contains every mod, base, omen, essence, bone,
   catalyst, and weight observation the advisor reads. ~211 KB
   gzipped covering 2 740 bases, 2 123 mods, 81 essences, 44 omens,
   12 catalysts, 10 bones, 2 951 weights.

2. **Hyprland always-on-top** (optional but recommended):
   ```bash
   # ~/.config/hypr/hyprland.conf
   source = /path/to/poc2/examples/hyprland/poc2-windowrules.conf
   ```
   See
   [`examples/hyprland/`](examples/hyprland/) for the NixOS module
   variant + per-rule explanations.

3. **Live prices** (optional): click "Refresh prices" in Settings
   to pull from poe2scout, or set the auto-refresh interval.

## Architecture

```
ADVISOR  (beam-search + Monte Carlo + streaming)
   │
   ├── STRATEGY LIBRARY  (23 codified TOML recipes)
   ├── RULE ENGINE       (113 forward-chained rules across 14 sections)
   ├── PROBABILITY       (Monte Carlo + Wilson score intervals)
   └── PLUGINS           (Wasm Component-Model + capability gates)
   │
   ▼
ENGINE CORE  (sub-µs apply(currency, item, omens))
   │
   ▼
DATA BUNDLE  (patch-versioned, hot-swappable)
   │
   ├── RePoE-fork    (mods, bases, tags)
   ├── Craft of Exile (essences, catalysts, weights ≥80% join)
   ├── poe2db.tw     (omens, bones)
   └── poe2scout     (live currency prices)
```

See [`docs/40-architecture.md`](docs/40-architecture.md) for the full
picture; [`docs/35-advisor-architecture.md`](docs/35-advisor-architecture.md)
for the planner internals; [`docs/36-decision-engine.md`](docs/36-decision-engine.md)
for the rule-priority pipeline.

## v1.0 feature matrix

| Phase | Deliverable | Status |
|---|---|---|
| M1 | Foundation (flake, workspace, CI) | ✅ |
| M2 | Engine core + data pipeline | ✅ |
| M3 | Strategy DSL + rule engine | ✅ |
| M4 | Beam-search advisor | ✅ |
| M5 | Probability + market | ✅ |
| M6 | UI v1 | ✅ |
| M7 | Clipboard import + Client.txt watcher | ✅ |
| M8 | Polish + release | ✅ |
| A.1 | PredicateContext threaded through advisor + rules | ✅ |
| A.2 | DSL action extensions (ActivateOmen, Recombine, Reveal floors) | ✅ |
| A.3 | CoE→engine mod-id join refinement | ✅ |
| A.4 | 23 strategies encoded (full /docs/33 coverage) | ✅ |
| A.5 | 113 rules across 14 section files (full /docs/34 coverage) | ✅ |
| A.6 | Bundle hot-swap via reload_bundle | ✅ |
| A.7 | docs/32, 36, 37, 51 | ✅ |
| B.1-B.4 | Target / Recovery / Settings / Recipe panels | ✅ |
| C.1-C.3 | Monte Carlo + streaming + simulation runner | ✅ |
| D.1-D.3 | Client.txt watcher + Hyprland + trade URL | ✅ |
| E.1-E.2 | poe.ninja meta + off-meta finder | ✅ |
| F.1-F.8 | Wasm Plugin SDK | ✅ |
| G.1-G.2 | Perf pass + release | ✅ |

## Performance

Per [`docs/35-advisor-architecture.md`](docs/35-advisor-architecture.md)
(i7-class laptop):

| Operation | Time | Budget | Margin |
|---|---|---|---|
| `plan_depth_1_top_3` | 46 µs | 1 ms | ×21 |
| `plan_depth_3_top_3_mc50` | 139 µs | 5 ms | ×35 |
| `plan_depth_5_width_8` | 151 µs | 500 ms | ×3311 |

## Documentation

- [`00-overview.md`](docs/00-overview.md) — vision, scope, decisions
- [`11-game-mechanics.md`](docs/11-game-mechanics.md) — PoE2 0.4 crafting mechanics reference
- [`13-patch-0.4-changes.md`](docs/13-patch-0.4-changes.md) — Fate of the Vaal patch deltas
- [`30-domain-model.md`](docs/30-domain-model.md) — Item / Mod / Omen / Strategy / Rule types
- [`31-engine-algorithms.md`](docs/31-engine-algorithms.md) — apply_currency contract
- [`32-probability-math.md`](docs/32-probability-math.md) — geometric / MC / Wilson math
- [`33-strategy-library.md`](docs/33-strategy-library.md) — 23 codified strategies
- [`34-heuristics-rulebook.md`](docs/34-heuristics-rulebook.md) — 120-rule catalogue
- [`35-advisor-architecture.md`](docs/35-advisor-architecture.md) — beam search + scoring
- [`36-decision-engine.md`](docs/36-decision-engine.md) — production-rule synthesis
- [`37-recovery-flows.md`](docs/37-recovery-flows.md) — recovery DAGs
- [`40-architecture.md`](docs/40-architecture.md) — system architecture
- [`41-ui-flows.md`](docs/41-ui-flows.md) — UI components + IPC commands
- [`51-market-meta.md`](docs/51-market-meta.md) — poe.ninja meta aggregator
- [`70-roadmap.md`](docs/70-roadmap.md) — milestones M1-M9+
- [`72-v1-execution-plan.md`](docs/72-v1-execution-plan.md) — Phases A-G
- [`adr/`](docs/adr/) — 9 architecture decision records

## Plugins

The Wasm plugin SDK (`crates/plugin-sdk`) lets the community extend
the advisor with custom predicates, strategies, rules, and
recommendation emitters. See
[`examples/plugins/`](examples/plugins/) for a working example +
build/install instructions.

```toml
# Reference a plugin custom predicate from a rule:
[[rule]]
when = { custom = { plugin_id = "predicate-meta-build-match", name = "matches_meta_build", args = { ascendancy = "Stormweaver" } } }
```

## License

[MIT](LICENSE). Game data and weights are credited to their sources;
see [`docs/adr/0003-data-sources.md`](docs/adr/0003-data-sources.md)
and [`docs/adr/0005-license-mit.md`](docs/adr/0005-license-mit.md).

## Related projects

- [pathofcrafting.net](https://pathofcrafting.net/) — web crafting simulator (separate brand, not this project)
- [Craft of Exile](https://craftofexile.com/?game=poe2) — community gold-standard simulator
- [POE2HTC](https://github.com/Dboire9/POE2_HTC) (AGPL-3) — reference for beam-search
- [pyoe2-craftpath](https://github.com/WladHD/pyoe2-craftpath) (MIT) — reference + potential dependency
- [XileHUD](https://github.com/XileHUD/poe_overlay) (GPL-3) — reference for clipboard / overlay
