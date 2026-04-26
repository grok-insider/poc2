# Architecture

> System architecture for Path of Crafting 2.

## Layers

```
                            ┌──────────────────────┐
                            │   ADVISOR (M4)       │
                            │  beam-search planner │◄───── user risk slider
                            │  full re-plan        │       market context
                            └─────────┬────────────┘
              ┌───────────────────────┼─────────────────────────┐
              ▼                       ▼                         ▼
      ┌──────────────┐        ┌──────────────┐         ┌─────────────────┐
      │  STRATEGY    │        │  RULE        │         │  PROBABILITY    │
      │  LIBRARY     │        │  ENGINE      │         │  & EV LAYER     │
      │  (M3)        │        │  (M3)        │         │  (M5)           │
      └──────┬───────┘        └──────┬───────┘         └────────┬────────┘
             └───────────────────────┼──────────────────────────┘
                                     ▼
                         ┌──────────────────────┐
                         │  ENGINE CORE (M2)    │
                         │  apply(currency)     │
                         │  sub-ms hot path     │
                         └──────────┬───────────┘
                                    ▼
                  ┌─────────────────────────────────┐
                  │  DATA BUNDLE (M2)               │
                  │  patch-versioned, hot-swappable │
                  └────────────┬────────────────────┘
                               │
          ┌────────────────────┼────────────────────┐
          ▼                    ▼                    ▼
   ┌─────────────┐      ┌────────────┐      ┌──────────────┐
   │  RePoE-fork │      │  poe2db.tw │      │  Craft of    │
   │  mods/bases │      │  omens     │      │  Exile       │
   │             │      │  essences  │      │  weights     │
   └─────────────┘      └────────────┘      └──────────────┘

  Live channels (M7+):
   • poe.ninja PoE2 → meta-build awareness, prices
   • poe2scout       → currency exchange snapshots
   • GGG /trade2     → live trade integration (OAuth)
   • Clipboard       → in-game item capture (wl-clipboard)
   • Client.txt      → zone awareness (inotify)
   • Layer-shell     → Hyprland overlay window
```

## Process model

The desktop app is a single Tauri 2 process with three logical compartments:

1. **Frontend** (WebKit + Svelte 5) — renders UI, dispatches commands via IPC.
2. **Tauri IPC bridge** (`apps/desktop/src-tauri`) — exposes Rust functions to JS, owns plugins.
3. **Workspace crates** (`crates/*`) — the engine, advisor, data, market, etc. Imported by the IPC bridge.

The advisor's beam search runs in a Tokio worker on the Rust side. The frontend subscribes to a stream of `Recommendation` events; new events arrive as the search deepens.

## Patch versioning

Every entity carries `patch_min` / `patch_max`:

```rust
struct PatchRange { min: Option<PatchVersion>, max: Option<PatchVersion> }
```

- Mods, currencies, omens, essences, bones, catalysts — versioned at the data-bundle level.
- Strategies and rules — versioned in TOML (`patch_min = "0.4.0"`).
- The bundle declares its `game_patch`. Loaders filter entities to those whose `PatchRange` contains it.

This is the mechanism by which 0.5 (May 29 2026) lands as a config swap rather than a rebuild.

## Sub-millisecond `apply()`

The advisor's beam search runs tens of thousands of `apply(currency, item, omens)` calls during a re-plan. Constraints:

- No allocations in the hot path. `Item` is small (`SmallVec` for mod slots, fixed-size arrays for fractures).
- Mod pools precomputed at bundle load: `(BaseType, ilvl, AffixType) → &[ModDefinition]`.
- State memoization: canonicalize an `Item` to a `u64` hash, cache score.
- Beam width / depth are user-configurable (default w=5, d=8).
- Search runs in a `tokio::task::spawn_blocking` so the runtime stays responsive.
- Cancellation: a new state arriving aborts the in-flight search.

## Data bundle

A bundle is a single JSON or compressed JSON document containing the entire dataset the engine needs. Schema sketch (full schema in `21-bundle-schema.json` once M2 lands):

```jsonc
{
  "schema_version": 1,
  "game_patch": "0.4.0",
  "built_at": "2026-04-26T12:00:00Z",
  "built_by": "pipeline@<git-sha>",
  "mods":              [...],
  "base_items":        [...],
  "item_classes":      [...],
  "tags":              [...],
  "currencies":        [...],
  "omens":             [...],
  "essences":          [...],
  "bones":             [...],
  "catalysts":         [...],
  "stat_translations": {...},
  "weights":           [...],   // CoE primary, poe2db cross-check
  "synergy_overrides": [...],   // hand-curated edge cases
  "concept_map":       {...}    // stat-id → concept (for hybrid analysis)
}
```

Bundles are produced by `pipeline/` and published as GitHub Releases. The desktop app:

- Ships with one baseline bundle embedded
- Checks for newer bundles on launch (configurable interval)
- Caches the latest bundle in `$XDG_DATA_HOME/poc2/bundles/`
- Is fully usable offline

## Synergy graph

Hybrid auto-derive + hand-override:

- Currencies declare `affected_by: Set<OmenId>`
- Omens declare `targets: CurrencyId` and `effect: EffectFn`
- The graph is computed at bundle load: edges are `(currency, omen) → effect`
- `synergy_overrides.toml` covers state-dependent or wildcard cases:
  - `HinekorasLock` applies wildcard
  - `OmenOfCorruption` modifies the *outcome distribution* of `VaalOrb`, not the orb itself
  - `OmenOfLight` applies to `OrbOfAnnulment` only when desecrated mods exist

## Hybrid mods (concept-based matching)

A "hybrid" mod is a single-affix mod producing multiple distinct concepts (e.g., `+X% ES AND +Y Life`). The engine handles them via a **concept map**:

1. RePoE-fork's `mods.json` lists each mod's `stats: [{id, min, max}, ...]`
2. The pipeline computes a `Concept` per `stat-id` (atomic semantic group)
3. Each mod is annotated with `concept_set: Set<Concept>`
4. Targets are concept-based: `{ concept: "EnergyShield", min_tier: 1 }` matches any mod whose `concept_set` contains `EnergyShield`
5. A hybrid `ES + Life` mod simultaneously satisfies `EnergyShield` and `Life` targets

This is required for the canonical "Triple T1 ES Body Armour" test fixture, where the user accepts hybrid ES mods alongside flat ES mods.

## NixOS / Hyprland specifics

- **Wayland-only** — no X11 fallback. `wl-clipboard` for clipboard, `wlr-layer-shell` for overlay.
- **flake.nix** — declarative dev shell. Includes Rust toolchain, Node, pnpm, Tauri system deps (webkit2gtk-4.1, libsoup-3, gtk3, gdk, etc.), Wayland deps.
- **Hyprland overlay** — implemented as a layer-shell surface, not a regular window. Hyprland window rules (`windowrulev2 = float, ...`) configure positioning.
- **PoE2 runs under Proton/Wine** — clipboard works (`wl-clipboard`), `Client.txt` lives in the Wine prefix, monitored via `inotify`.
