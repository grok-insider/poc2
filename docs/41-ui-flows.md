# UI Flows (M6 v1)

> Describes the desktop frontend in `apps/desktop/src/`. Svelte 5 with
> runes, served by Tauri 2 inside a webkit2gtk webview.

## Layout

```
┌─────────────────────────────────────────────────────────┐
│ Path of Crafting 2                                      │
│ PoE2 crafting advisor — M6 advisor IPC                  │
├─────────────────────────────────────────────────────────┤
│  ┌─────────────────────┐  ┌────────────────────────────┐│
│  │ Import item         │  │ Advisor                    ││
│  │  [Read clipboard]   │  │  Risk: 0.50 ──────●────    ││
│  │  ▾ Or paste text    │  │  Depth: 3   ─●──────────   ││
│  ├─────────────────────┤  │  [Re-plan]                 ││
│  │ Item                │  │  patch 0.4.0 · 25 rules ·  ││
│  │  Base: BodyArmour   │  │  3 strategies · 2123 mods  ││
│  │  ilvl: 82           │  │  · bundle: ~/.config/...   ││
│  │  [Normal][Magic]... │  │                            ││
│  │  ☐ corrupted        │  │  ┌──────────────────────┐  ││
│  │  Prefixes (0/3)     │  │  │ PerfectOrbOfTrans... │  ││
│  │  Suffixes (0/3)     │  │  │ score: 4.123        │  ││
│  └─────────────────────┘  │  │ free · P=87% · d=1   │  ││
│  [Reset to fresh BA]      │  │ Normal ilvl 82 → ... │  ││
│                           │  │ rule R001 (verified) │  ││
│                           │  ├──────────────────────┤  ││
│                           │  │ ...                  │  ││
│                           │  └──────────────────────┘  ││
│                           └────────────────────────────┘│
│  ┌──────────────────────────────────────────────────┐   │
│  │ Health check                                     │   │
│  │  [Ping Tauri backend]   poc2 v0.1.0 ready ...    │   │
│  └──────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────┘
```

## Components

### `App.svelte`

Top-level layout. Two-column flex above 720px (item builder left,
advisor right); single column below. Keeps shared state (`item`,
`goal`) and propagates updates via callback props.

### `ClipboardImport.svelte`

- **Read clipboard** button calls `read_clipboard_item` over IPC.
  Tauri's `tauri-plugin-clipboard-manager` reads the system clipboard;
  the Rust handler runs `parse_clipboard_text` + `lower_to_item` and
  returns `ParseClipboardResponse { parsed, item, unresolved }`.
- **Manual paste** textarea calls `parse_item_text` for the same flow
  on user-typed input.
- Surfaces unresolved mod lines under a `<details>` so the user can
  see which mods didn't match the loaded bundle's registry.

### `ItemBuilder.svelte`

Manual-edit fallback for when the user wants to construct an item
without copy-pasting. Inputs:

- Base name (text)
- ilvl (number 1-100)
- Rarity buttons (Normal / Magic / Rare / Unique)
- Corrupted / Sanctified checkboxes
- Slot summary (read-only — manual mod entry comes in M6 polish)
- Hidden-desecrated / Hinekora-Lock indicators

Every change calls `onUpdate(item)` to bubble up to `App.svelte`.

### `AdvisorPanel.svelte`

The advisor's live re-plan. Two sliders (risk + depth) plus a manual
Re-plan button. Uses Svelte 5's `$effect` to call `recommend(args)`
whenever the item, goal, risk, or depth changes.

Each `Recommendation` renders as:

```
PerfectOrbOfTransmutation                       score 4.123
free · P(reach) ≈ 87% · depth 1
Normal ilvl 82 base. Perfect Transmute guarantees a required-level >= 70 mod.
rule R001-perfect-transmute-on-normal (verified)
```

The meta strip shows patch, rule count, strategy count, mod count, and
the loaded bundle path (or a highlighted "no bundle loaded" warning).

## IPC Commands

| Command | Args | Returns |
|---------|------|---------|
| `ping` | — | `String` health line |
| `recommend` | `RecommendArgs { item, goal, stash?, risk?, top_n?, depth? }` | `RecommendResponse { recommendations, patch, rule_count, strategy_count, mod_count, bundle_path }` |
| `parse_item_text` | `text: String` | `ParseClipboardResponse { parsed, item, unresolved }` |
| `read_clipboard_item` | — | same as `parse_item_text` |

## Data Bundle Loading

At startup `AdvisorState::build()` searches for a bundle in:

1. `$POC2_BUNDLE` (env var, must be an absolute path to `*.bundle.json{,.gz}`)
2. `$XDG_CONFIG_HOME/poc2/bundles/` or `~/.config/poc2/bundles/`
3. `$XDG_DATA_HOME/poc2/bundles/` or `~/.local/share/poc2/bundles/`

The newest `*.bundle.json{,.gz}` in each directory wins. Validation
failures are warned-and-skipped so a single bad bundle doesn't block
startup.

To produce a bundle:

```bash
cargo run --release -p poc2-pipeline -- build --out /tmp/poc2.bundle.json.gz --patch 0.4.0
mkdir -p ~/.config/poc2/bundles
cp /tmp/poc2.bundle.json.gz ~/.config/poc2/bundles/
```

## Strategy Loading

In addition to the 3 seed strategies bundled into the binary
(canonical 3xT1 ES + Apprentice Blueprint + Whittling Cleanup),
`AdvisorState::build()` walks
`$XDG_CONFIG_HOME/poc2/strategies/*.toml` and loads every strategy
that parses + validates. Per-file failures are warned-and-skipped.

## Phase B Polish (shipped)

- **Target panel** (`TargetPanel.svelte`) edits the `Goal`
  interactively; persists via `save_state` to
  `$XDG_CONFIG_HOME/poc2/state.toml`.
- **Recovery panel** (`RecoveryPanel.svelte`) surfaces strategy step
  `recovery` hints when the user toggles "Last action failed".
- **Settings panel** (`SettingsPanel.svelte`): bundle hot-swap,
  league dropdown, prices auto-refresh interval, Client.txt watcher
  toggle.
- **Recipe library** (`RecipeLibrary.svelte`) — save / load / share
  recipes via `$XDG_CONFIG_HOME/poc2/recipes/<name>.toml`.
- **Simulation runner** (`SimulationRunner.svelte`) — runs N Monte
  Carlo trials of a candidate action; renders an inline-SVG
  change-count histogram.

## Hyprland integration (Phase D.2)

Per ADR-0009, the v1 always-on-top behaviour is implemented as a
documented Hyprland configuration recipe rather than a custom Wayland
layer-shell surface.

Add to `~/.config/hypr/hyprland.conf` (or your NixOS Hyprland module):

```hyprlang
windowrulev2 = float, class:^(ai\.anomaly\.poc2)$
windowrulev2 = pin, class:^(ai\.anomaly\.poc2)$
windowrulev2 = noborder, class:^(ai\.anomaly\.poc2)$
windowrulev2 = size 480 720, class:^(ai\.anomaly\.poc2)$
windowrulev2 = move 100% 0, class:^(ai\.anomaly\.poc2)$
windowrulev2 = opacity 0.95, class:^(ai\.anomaly\.poc2)$
```

`pin` keeps the window on top across workspaces; `float` keeps it
out of the tiling stack; `move 100% 0` docks it to the right edge.
Run PoE2 in borderless windowed mode (not exclusive fullscreen) for
the overlay to actually float on top.

Clipboard reads use `wl-clipboard` via
`tauri-plugin-clipboard-manager` and work unchanged on Wayland.

## Live integration (Phase D)

| Subsystem | Tauri command | Event topic |
|---|---|---|
| Client.txt watcher (D.1) | `start_client_log` / `stop_client_log` / `client_log_status` | `client-log://event` |
| Always-on-top (D.2) | (Hyprland windowrulev2; no Tauri command) | — |
| Trade-search URL (D.3) | `trade_search` | — (opens browser) |
