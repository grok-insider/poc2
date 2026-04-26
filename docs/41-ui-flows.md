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

## Future M6 Polish

- **Target panel** for editing the `Goal` (currently bound to the
  hard-coded worked-example goal in `lib/fixtures.ts`).
- **Recovery panel** that surfaces strategy step `recovery` hints when
  the last action's outcome was a failure.
- **Simulation runner** — a button that runs N Monte Carlo trials of a
  candidate plan and renders cost / probability histograms.
- **Recipe library** — save / load / share strategies + goals to
  `$XDG_CONFIG_HOME/poc2/recipes/`.
- **Settings** page for risk slider persistence, valuator overrides,
  trade league selection, and bundle update controls.
