//! Example plugin: custom predicate `ilvl_at_least`.
//!
//! Returns `true` when the item's `ilvl` is >= the `min` value
//! supplied via the predicate's `args` JSON object.
//!
//! ## Build
//!
//! ```bash
//! rustup target add wasm32-unknown-unknown
//! cargo build --release --target wasm32-unknown-unknown -p predicate-ilvl-min
//! cp target/wasm32-unknown-unknown/release/predicate_ilvl_min.wasm \
//!    ~/.config/poc2/plugins/predicate-ilvl-min/plugin.wasm
//! cp poc2-plugin.toml ~/.config/poc2/plugins/predicate-ilvl-min/
//! ```
//!
//! ## Use
//!
//! Reference from a strategy / rule TOML:
//!
//! ```toml
//! [[rule]]
//! id = "R-Custom-ilvl-82"
//! category = "base_selection"
//! when = { custom = { plugin_id = "predicate-ilvl-min", name = "ilvl_at_least", args = { min = 82 } } }
//! ...
//! ```

#![allow(unsafe_code)]

use poc2_plugin_sdk::serde_json::Value;
use poc2_plugin_sdk::{declare_predicate, declare_rules, serde_json};

declare_predicate!(ilvl_at_least, |item: &Value, args: &Value| {
    let ilvl = item.get("ilvl").and_then(serde_json::Value::as_u64).unwrap_or(0);
    let min = args.get("min").and_then(serde_json::Value::as_u64).unwrap_or(82);
    ilvl >= min
});

// Phase 1 + 2 combined demo: the plugin ALSO ships a rule that is gated
// by its own custom predicate. When loaded in the app
// (Settings → Plugins), fresh Normal ilvl-82+ items get an extra
// plugin-sourced suggestion — visible proof that both the rule emission
// (phase 1) and the live predicate dispatch (phase 2) work end-to-end.
declare_rules!(
    r#"
[[rule]]
id = "R-PLUGIN-ilvl-min-transmute"
category = "base_selection"
explanation = "Plugin demo: ilvl_at_least(82) gate satisfied - this base is endgame-viable."
source = "predicate-ilvl-min example plugin"
confidence = "experimental"

[rule.when]
all = [
    { rarity = "normal" },
    { custom = { plugin_id = "predicate-ilvl-min", name = "ilvl_at_least", args = { min = 82 } } },
]

[[rule.then]]
note = "predicate-ilvl-min: ilvl >= 82 confirmed by the plugin predicate - a Greater Transmute keeps the mod level floor at 44+."
priority = 60
[rule.then.action]
kind = "apply_currency"
currency = "GreaterOrbOfTransmutation"
omens = []
"#,
);
