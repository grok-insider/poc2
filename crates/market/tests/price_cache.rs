//! Phase F integration tests for the on-disk price cache.
//!
//! These tests cover the cache surface poc2-desktop uses at startup:
//! a fresh write, a read-back within TTL, the stale-window detection,
//! and the missing-cache fallback path. None of these tests touch the
//! network — they exercise the `crate::cache` module directly with
//! synthesized `PoeScoutSnapshot` payloads. Live poe2scout integration
//! is covered by the unit tests inside `crates/market/src/prices.rs`.

// The cache module only exists with the `net` feature.
#![cfg(feature = "net")]

use std::collections::HashMap;
use std::time::Duration;

use poc2_engine::ids::CurrencyId;
use poc2_market::{
    apply_feed_to_valuator, cache_file_for_league, cache_store, cache_try_load, default_id_mapping,
    CachedSnapshot, PoeScoutCurrencyEntry, PoeScoutSnapshot, Valuator, DEFAULT_TTL,
};
use tempfile::TempDir;

fn fake_snapshot(league: &str) -> PoeScoutSnapshot {
    let mut entries = HashMap::new();
    entries.insert(
        "divine".to_string(),
        PoeScoutCurrencyEntry {
            currency_item_id: 1,
            item_id: 1,
            currency_category_id: 21,
            api_id: "divine".into(),
            text: "Divine Orb".into(),
            category_api_id: "currency".into(),
            icon_url: None,
            current_price: Some(200.0),
            current_quantity: None,
        },
    );
    entries.insert(
        "omen-of-whittling".to_string(),
        PoeScoutCurrencyEntry {
            currency_item_id: 2,
            item_id: 2,
            currency_category_id: 41,
            api_id: "omen-of-whittling".into(),
            text: "Omen of Whittling".into(),
            category_api_id: "ritual".into(),
            icon_url: None,
            current_price: Some(60.0),
            current_quantity: None,
        },
    );
    entries.insert(
        "perfect-essence-of-the-body".to_string(),
        PoeScoutCurrencyEntry {
            currency_item_id: 3,
            item_id: 3,
            currency_category_id: 11,
            api_id: "perfect-essence-of-the-body".into(),
            text: "Perfect Essence of the Body".into(),
            category_api_id: "essences".into(),
            icon_url: None,
            current_price: Some(40.0),
            current_quantity: None,
        },
    );
    PoeScoutSnapshot {
        league: league.into(),
        divine_price_in_exalts: 200.0,
        chaos_per_divine: 30.0,
        entries,
        fetched_at: "2026-04-26T12:00:00Z".into(),
    }
}

#[test]
fn store_then_load_round_trips_via_disk() {
    let dir = TempDir::new().unwrap();
    let league = "Fate of the Vaal";
    let cached = CachedSnapshot::now(fake_snapshot(league));
    cache_store(dir.path(), league, &cached).expect("store");

    let path = cache_file_for_league(dir.path(), league);
    assert!(path.exists(), "cache file must be created");

    let loaded = cache_try_load(dir.path(), league).expect("load").unwrap();
    assert_eq!(loaded.snapshot.league, league);
    assert_eq!(loaded.snapshot.divine_price_in_exalts, 200.0);
    assert_eq!(loaded.snapshot.entries.len(), 3);
    assert!(!loaded.is_stale(DEFAULT_TTL));
}

#[test]
fn missing_cache_returns_none_so_caller_can_fall_back_to_fetch() {
    let dir = TempDir::new().unwrap();
    let loaded = cache_try_load(dir.path(), "Standard").unwrap();
    assert!(loaded.is_none());
}

#[test]
fn cache_entries_become_stale_past_ttl() {
    let mut cached = CachedSnapshot::now(fake_snapshot("Standard"));
    // Pretend the cache was written 2h ago.
    cached.cached_at_unix = cached.cached_at_unix.saturating_sub(60 * 60 * 2);
    assert!(cached.is_stale(DEFAULT_TTL));
    assert!(!cached.is_stale(Duration::from_secs(60 * 60 * 6)));
}

#[test]
fn cached_snapshot_feeds_valuator_via_apply_feed() {
    // A canonical "load cached snapshot, hand it to the valuator" flow —
    // this is exactly what the Tauri startup path does when poe2scout
    // is unreachable but the user has a recent cached refresh on disk.
    let dir = TempDir::new().unwrap();
    let league = "Fate of the Vaal";
    let cached = CachedSnapshot::now(fake_snapshot(league));
    cache_store(dir.path(), league, &cached).unwrap();
    let loaded = cache_try_load(dir.path(), league).unwrap().unwrap();

    let mut valuator = Valuator::default();
    let mapping = default_id_mapping();
    let applied = apply_feed_to_valuator(&mut valuator, &loaded.snapshot, &mapping);
    assert_eq!(
        applied, 3,
        "all three known slugs (divine, omen-of-whittling, perfect-essence-of-the-body) feed through"
    );

    // Spot-check a few of the prices. Divine maps to itself (1 div).
    let div = valuator.get(&CurrencyId::from("DivineOrb")).unwrap();
    assert!((div.expected - 1.0).abs() < 1e-9);

    // Omen of Whittling: 60 exalts -> 60/200 = 0.3 div.
    let om = valuator.get(&CurrencyId::from("OmenOfWhittling")).unwrap();
    assert!((om.expected - 0.3).abs() < 1e-9, "got {om:?}");

    // Perfect Essence of the Body: 40 exalts -> 0.2 div.
    let ess = valuator
        .get(&CurrencyId::from("PerfectEssenceOfTheBody"))
        .unwrap();
    assert!((ess.expected - 0.2).abs() < 1e-9, "got {ess:?}");
}

#[test]
fn league_subdir_collisions_separate_per_league_files() {
    // Two leagues, two cache files, no cross-talk.
    let dir = TempDir::new().unwrap();
    let a = CachedSnapshot::now(fake_snapshot("Fate of the Vaal"));
    let b = CachedSnapshot::now(fake_snapshot("Standard"));
    cache_store(dir.path(), "Fate of the Vaal", &a).unwrap();
    cache_store(dir.path(), "Standard", &b).unwrap();

    let loaded_a = cache_try_load(dir.path(), "Fate of the Vaal")
        .unwrap()
        .unwrap();
    let loaded_b = cache_try_load(dir.path(), "Standard").unwrap().unwrap();
    assert_eq!(loaded_a.snapshot.league, "Fate of the Vaal");
    assert_eq!(loaded_b.snapshot.league, "Standard");
}

#[test]
fn store_overwrites_previous_cache_atomically() {
    // Two writes for the same league must result in the latter sticking,
    // and the loader must never observe a half-written file even mid-
    // overwrite (rename-based atomicity guarantees this on POSIX).
    let dir = TempDir::new().unwrap();
    let league = "Standard";
    let mut snap = fake_snapshot(league);
    snap.divine_price_in_exalts = 200.0;
    cache_store(dir.path(), league, &CachedSnapshot::now(snap)).unwrap();

    let mut snap2 = fake_snapshot(league);
    snap2.divine_price_in_exalts = 250.0;
    cache_store(dir.path(), league, &CachedSnapshot::now(snap2)).unwrap();

    let loaded = cache_try_load(dir.path(), league).unwrap().unwrap();
    assert!(
        (loaded.snapshot.divine_price_in_exalts - 250.0).abs() < 1e-9,
        "second write must win"
    );
}
