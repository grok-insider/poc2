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

use poc2_plugin_sdk::{declare_predicate, serde_json};

declare_predicate!(ilvl_at_least, |item, args| {
    let ilvl = item.get("ilvl").and_then(serde_json::Value::as_u64).unwrap_or(0);
    let min = args.get("min").and_then(serde_json::Value::as_u64).unwrap_or(82);
    ilvl >= min
});
