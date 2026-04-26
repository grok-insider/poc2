# Market Meta Aggregator (M5+ / Phase E)

> Companion to [`poc2-market`](../crates/market) and the upcoming
> Phase E work. Designs the poe.ninja PoE2 builds aggregator and the
> off-meta-finder that consume it.

## Goals

1. **Surface meta builds** — show the user which crafts the rest of
   the league is doing, so they can either (a) supply demand or (b)
   intentionally chase off-meta niches with low competition.
2. **Off-meta opportunity finder** — combine build popularity with
   live trade prices to rank niche crafting goals where price-per-
   demand is high.
3. **Local cache** — the user's per-craft `recommend` IPC must stay
   sub-100ms. Network calls happen out-of-band and feed a local cache
   that the cost / EV scorer reads from.

## Data Sources

### poe.ninja PoE2 builds endpoint

The base URL pattern (mirroring the PoE1 endpoints):

```
https://poe.ninja/api/data/poe2/builds?league=Fate%20of%20the%20Vaal
```

We expect the response shape to mirror PoE1's:

```json
{
  "skills":   [...],
  "uniques":  [...],
  "builds":   [
    {
      "name":  "Stormbringer",
      "ascendancy": "Stormweaver",
      "hp":    7200,
      "es":    8400,
      "main_skill_name": "Lightning Strike",
      "items": [{ "name": "...", "frame_type": 2, "modifiers": [...] }, ...],
      "popularity": 1234   // # of profiles using this build
    },
    ...
  ]
}
```

The exact field names are TBD until we hit the live endpoint. The
crate's deserializer is intentionally permissive (`#[serde(default)]`
on every optional field).

### poe2scout currency feed (already integrated, M5)

`poc2_market::prices::fetch_snapshot` returns the per-currency snapshot
from poe2scout. This is the price input to the EV scorer.

The meta aggregator consumes both feeds together:

```
poe.ninja /builds  ──┐
                     ├──► MetaSnapshot { builds, prices, fetched_at }
poe2scout /currencies ┘            │
                                    ▼
                      ┌────────────────────────┐
                      │  off_meta(builds, prices) │
                      └────────────────────────┘
                                    │
                                    ▼
                          Vec<NicheTarget>
```

## Crate Layout

`crates/market/src/meta.rs` (new in Phase E.1):

```rust
pub struct MetaBuild {
    pub id:           String,        // url-safe slug
    pub name:         String,
    pub ascendancy:   String,
    pub popularity:   u32,           // # profiles
    pub key_mods:     Vec<ConceptId>, // concept-aware mod fingerprint
    pub base_choices: Vec<ItemClassId>,
}

pub struct MetaSnapshot {
    pub builds:    Vec<MetaBuild>,
    pub fetched_at: String,
    pub league:    String,
    pub source_revisions: SourceRevisions,
}

pub async fn fetch_meta_builds(
    client: &Client,
    league: &str,
) -> Result<MetaSnapshot, MarketError>;
```

`crates/market/src/meta::off_meta` (Phase E.2):

```rust
pub struct NicheTarget {
    pub craft:       CraftDescription,   // mods + base + slot
    pub demand:      f64,                // 0..1, share of off-meta builds
    pub competition: f64,                // 0..1, # crafters listing
    pub gross_price: DivEquiv,           // current trade price
    pub net_ev:      DivEquiv,           // gross_price - expected_craft_cost
    pub rationale:   String,
}

pub fn off_meta(
    builds: &[MetaBuild],
    prices: &PriceSnapshot,
    crafter_cost_estimator: &dyn CrafterCostEstimator,
) -> Vec<NicheTarget>;
```

`CrafterCostEstimator` is implemented by the advisor — it knows how
to estimate the EV of crafting a target item given current valuator
prices.

## Caching Strategy

- Builds snapshot: 30-min TTL, cached at
  `$XDG_CACHE_HOME/poc2/meta_builds.json.gz`.
- Currency snapshot: 5-min TTL (already in M5 implementation).
- Off-meta computation: pure function over (builds, prices); cached
  in-memory for the duration of an Advisor session.

## UI Surface (Settings panel — Phase B.3)

A "What to craft right now" card surfaces the top-3 niche targets,
ranked by `net_ev / sqrt(competition + 1)` (penalize crowded niches):

```
┌──────────────────────────────────────────────┐
│ What to craft right now                  ⟳  │
│ ────────────────────────                     │
│                                              │
│ 1. T1 Cold Spell Skills + T1 Crit          │
│    on Heavy Belt (ilvl 82)                 │
│    Demand: ●●●○○  Competition: ●○○○○        │
│    Trade price: 18–35 div  EV: +12 div     │
│                                              │
│ 2. +4 Minion Skill Levels Sceptre          │
│    (ilvl 78)                               │
│    Demand: ●●○○○  Competition: ●○○○○        │
│    Trade price: 25–60 div  EV: +18 div     │
│                                              │
│ 3. Dual T1 Phys% + T1 Cold Adds Bow         │
│    (ilvl 81 Gemini)                         │
│    Demand: ●●○○○  Competition: ●●○○○        │
│    Trade price: 10–25 div  EV: +6 div      │
└──────────────────────────────────────────────┘
```

## Refresh Cadence

- Background poller runs every 30 minutes by default; configurable in
  Settings (off / 30min / 1hr / 4hr / manual only).
- Manual refresh via the same button as `refresh_prices` (already
  exists in M6).
- On startup: load cached snapshot if < 30 min old; otherwise queue
  a background refresh.

## Privacy / Telemetry

- All requests carry the user-agent
  `poc2-desktop/<version> (+contact: github issues)`.
- No request includes the user's account name, character names, or
  crafted-item details.
- Cached snapshots are local only; never re-uploaded.

## Failure Modes

- poe.ninja endpoint down: serve stale cache + log a warning, no
  fatal error.
- Schema drift (new field, missing field): permissive deserializer
  ignores unknowns; missing fields default; partial snapshots
  surface what they can.
- Rate limiting (429): exponential backoff; surface a non-blocking
  banner in Settings.

## Future Work

- **Trade-search integration**: clicking a NicheTarget opens the trade
  URL pre-filtered to that mod combo (Phase D.3 trade-search adapter).
- **Build diff alerting**: when the meta shifts (new build climbs
  past a threshold popularity), surface a "Meta shift" tip in the
  Settings sidebar.
- **Plugin-emitted niches** (Phase F.4): community plugins can
  register their own niche detectors (e.g. specific build-archetype
  targeting) that feed into the same ranking pipeline.
