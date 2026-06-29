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

// Meta pollers + on-disk snapshot cache are gated behind the `net` feature
// (reqwest/tokio) so the crate builds in non-networked targets (the WASM
// advisor). `prices` builds everywhere: only its `fetch_snapshot` HTTP path
// is net-gated — the snapshot types + `apply_feed_to_valuator` +
// `default_id_mapping` are needed wasm-side to apply browser-fetched feeds.
#[cfg(feature = "net")]
pub mod cache;
#[cfg(feature = "net")]
pub mod meta;
pub mod name_match;
pub mod prices;
pub mod valuator;

#[cfg(feature = "net")]
pub use cache::{
    cache_file_for_league, default_cache_dir, store as cache_store, try_load as cache_try_load,
    CacheError, CachedSnapshot, DEFAULT_TTL,
};
#[cfg(feature = "net")]
pub use meta::{
    fetch_meta_snapshot, off_meta, MetaBuild, MetaError, MetaSnapshot, NicheTarget,
    POE_NINJA_BUILDS_BASE_URL, POE_NINJA_DEFAULT_LEAGUE,
};
pub use name_match::{
    bundled_translator, LocaleFile, NameIndex, NameMatch, NameTranslator, BUNDLED_LOCALES,
};
pub use prices::{
    apply_feed_to_valuator, apply_ninja_to_valuator, default_id_mapping, NinjaExchangeSnapshot,
    NinjaPriceEntry, PoeScoutCurrencyEntry, PoeScoutLeague, PoeScoutSnapshot, PriceError,
    POE2SCOUT_BASE_URL, POE2SCOUT_DEFAULT_CATEGORIES, POE2SCOUT_DEFAULT_LEAGUE, POE2SCOUT_REALM,
    POE_NINJA_EXCHANGE_BASE, POE_NINJA_EXCHANGE_TYPES,
};
#[cfg(feature = "net")]
pub use prices::{fetch_ninja_exchange, fetch_snapshot};
pub use valuator::{DivEquiv, Valuator};
