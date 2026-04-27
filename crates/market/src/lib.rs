//! # poc2-market
//!
//! Currency valuator, live price feeds, and meta-build awareness.
//!
//! ## Modules
//!
//! - [`valuator`] — `DivEquiv(min, expected, max)` and cross-currency conversion graph.
//!   Conservative fallback ranges (per planning):
//!   `1 div = 50-180 ex`, `1 div = 3-30 chaos`, `1 mirror = 1500-6000 div`.
//!   Live data from poe2scout / poe.ninja overrides within 30s of online connection.
//! - [`prices`] — pollers for poe2scout, poe.ninja PoE2.
//! - [`meta`] — meta-build aggregator (poe.ninja PoE2 builds page) + off-meta finder.
//!
//! Stub for M1; real implementation in M5/M6.

#![forbid(unsafe_code)]
#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::doc_markdown)]

pub mod cache;
pub mod meta;
pub mod prices;
pub mod valuator;

pub use cache::{
    cache_file_for_league, default_cache_dir, store as cache_store, try_load as cache_try_load,
    CacheError, CachedSnapshot, DEFAULT_TTL,
};
pub use meta::{
    fetch_meta_snapshot, off_meta, MetaBuild, MetaError, MetaSnapshot, NicheTarget,
    POE_NINJA_BUILDS_BASE_URL, POE_NINJA_DEFAULT_LEAGUE,
};
pub use prices::{
    apply_feed_to_valuator, default_id_mapping, fetch_snapshot, PoeScoutCurrencyEntry,
    PoeScoutLeague, PoeScoutSnapshot, PriceError, POE2SCOUT_BASE_URL, POE2SCOUT_DEFAULT_CATEGORIES,
    POE2SCOUT_DEFAULT_LEAGUE, POE2SCOUT_REALM,
};
pub use valuator::{DivEquiv, Valuator};
