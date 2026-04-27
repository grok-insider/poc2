# Example plugins (Phase F.7)

Working sample plugins demonstrating the
[`poc2-plugin-sdk`](../../crates/plugin-sdk/) macros.

## Building

Plugins compile to `wasm32-unknown-unknown`. Add the target once:

```bash
rustup target add wasm32-unknown-unknown
```

Then per plugin:

```bash
cd examples/plugins/<plugin-name>
cargo build --release --target wasm32-unknown-unknown
```

The artifact lands at
`target/wasm32-unknown-unknown/release/<crate_name>.wasm`. Copy that
plus the `poc2-plugin.toml` manifest into
`~/.config/poc2/plugins/<plugin-id>/`.

## Plugins

### `predicate-ilvl-min`

Custom predicate `ilvl_at_least`. Returns `true` when the item's
ilvl is >= the `min` value supplied via the predicate's `args`.

Manifest declares `read_engine` + `register_predicate` capabilities.

Reference from a TOML rule:

```toml
[[rule]]
id = "R-Custom-ilvl-82"
category = "base_selection"
when = { custom = { plugin_id = "predicate-ilvl-min", name = "ilvl_at_least", args = { min = 82 } } }
explanation = "Item must be ilvl 82 or higher per build requirements."
source = "user-defined"
confidence = "verified"

[[rule.then]]
note = "ilvl gate satisfied"
priority = 50
[rule.then.action]
kind = "guidance"
```

## Adding a new plugin

1. Copy `predicate-ilvl-min/` to `<your-plugin-name>/`.
2. Edit `Cargo.toml` (rename) + `src/lib.rs` (your closure).
3. Edit `poc2-plugin.toml` to set the right id + capabilities.
4. `cargo build --release --target wasm32-unknown-unknown`.
5. Drop the resulting `.wasm` + `poc2-plugin.toml` in
   `~/.config/poc2/plugins/<your-plugin-id>/`.
6. Restart the desktop app (or call the `reload_bundle` Tauri
   command — the host walks the plugins dir on every reload).

## Capabilities cheat sheet

| Capability | Allows the plugin to… |
|---|---|
| `read_engine` | Read the current Item state |
| `read_market` | Read live valuator prices |
| `read_advisor_state` | Read goal + cost-spent + recs-so-far |
| `register_predicate` | Export `eval_predicate` |
| `emit_strategies` | Export `list_strategies()` |
| `emit_rules` | Export `list_rules()` |
| `emit_recommendations` | Export `emit_recommendations(state)` |

The host refuses to load plugins declaring capabilities the user
hasn't approved; manage approvals in the Settings → Plugins UI
(Phase F.6 ships in v1).
