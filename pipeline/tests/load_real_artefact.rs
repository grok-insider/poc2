//! Smoke test: load the real production artefact at
//! `~/.config/poc2/cache/trained_models/` and report the cache count.
//!
//! Skipped when the directory doesn't exist (CI / fresh checkouts).
//! Useful for catching schema-drift bugs after a `train-advisor` run
//! against a live bundle.

#[test]
fn load_production_artefact_directory() {
    let Some(dir) = poc2_state_cache_dir() else {
        eprintln!("no $HOME / $XDG_CONFIG_HOME — skipping");
        return;
    };
    let trained_dir = dir.join("trained_models");
    if !trained_dir.exists() {
        eprintln!(
            "no trained_models dir at {}; skipping",
            trained_dir.display()
        );
        return;
    }
    let (cache, loaded, skipped) = poc2_advisor::training::load_cache_from_dir(&trained_dir);
    eprintln!(
        "loaded {} model(s), {} file(s) skipped, cache.len() = {}",
        loaded,
        skipped,
        cache.len()
    );
    assert_eq!(skipped, 0, "no artefact files should fail to parse");
    if loaded > 0 {
        assert!(!cache.is_empty(), "cache must be non-empty when loaded > 0");
    }
}

fn poc2_state_cache_dir() -> Option<std::path::PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".config"))
        })?;
    Some(base.join("poc2").join("cache"))
}
