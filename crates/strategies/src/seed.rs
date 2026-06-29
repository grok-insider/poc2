//! Embedded seed-strategy catalogue.
//!
//! The canonical strategy TOMLs under `crates/strategies/strategies/` are
//! embedded into the binary at compile time so every host — the desktop Tauri
//! app and the WebAssembly web build alike — can populate a default
//! [`crate::StrategyRegistry`] without any filesystem access.

use include_dir::{include_dir, Dir};

use crate::dsl::Strategy;
use crate::loader::load_strategy_str;

static SEED_DIR: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/strategies");

/// Parse every embedded seed-strategy TOML into a [`Strategy`], skipping (with a
/// warning) any that fail to parse. Returned in a stable, file-name-sorted order
/// so the resulting registry is deterministic.
#[must_use]
pub fn seed_strategies() -> Vec<Strategy> {
    let mut files: Vec<_> = SEED_DIR
        .files()
        .filter(|f| {
            f.path()
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("toml"))
        })
        .collect();
    files.sort_by_key(|f| f.path().to_path_buf());
    files
        .into_iter()
        .filter_map(|f| {
            let src = f.contents_utf8()?;
            match load_strategy_str(src) {
                Ok(s) => Some(s),
                Err(e) => {
                    tracing::warn!(
                        file = %f.path().display(),
                        error = %e,
                        "seed strategy failed to parse; skipping"
                    );
                    None
                }
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_strategies_parse_and_are_nonempty() {
        let s = seed_strategies();
        assert!(
            s.len() >= 30,
            "expected the full seed catalogue (40+); got {}",
            s.len()
        );
        // Determinism: same order on repeat.
        let ids1: Vec<_> = s.iter().map(|x| x.id.0.clone()).collect();
        let ids2: Vec<_> = seed_strategies().iter().map(|x| x.id.0.clone()).collect();
        assert_eq!(ids1, ids2);
    }
}
