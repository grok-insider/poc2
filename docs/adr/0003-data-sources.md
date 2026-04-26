# ADR-0003 — Data sources strategy

- Status: Accepted
- Date: 2026-04-26

## Context

We need PoE2 game data: mods, base items, currencies, omens, essences, bones, catalysts, stat translations, mod weights, and live prices. No single source has all of it.

GGG's developer docs explicitly state they don't provide PoE2 data exports.

## Source matrix

| Need | Primary | Secondary | License |
|---|---|---|---|
| Mods, base items, item classes, tags, mods-by-base joins, stat translations | RePoE-fork (`repoe-fork.github.io/poe2/`) | LocalIdentity/poe2-data raw dat dumps | NOASSERTION (data: GGG) |
| Mod spawn weights (numeric) | Craft of Exile (`craftofexile.com/json/poe2/main/poec_data.json`) | poe2db.tw `Modifiers` "Weight" column | unfixed; community |
| Currencies, omens, essences, bones, catalysts | poe2db.tw scrape (`/us/Crafting`, `/us/Omen`, `/us/Essence`, ...) | RePoE-fork (when present) | CC BY-NC-SA |
| Stat-id ↔ trade hash mapping | GGG `/trade/data/stats` | — | GGG ToS, OAuth |
| Live currency / item prices | poe2scout currency exchange | poe.ninja PoE2 | free |
| Patch-day schema regen | ggpk-explorer + `poe-tool-dev/dat-schema` | — | GPL-3 + MIT |

## Decision

The `pipeline/` crate fetches all of the above on a scheduled cadence, normalizes into a single versioned bundle, and publishes as a GitHub Release.

### Cadence

| Source | Cadence | Notes |
|---|---|---|
| RePoE-fork | Hourly | Auto-rebuilds on patch via their CI |
| Craft of Exile poec_data | Daily | Stable URL, polite |
| poe2db.tw scrape | Daily | Small set (~30 pages), polite User-Agent, rate-limited |
| GGG `/trade/data/stats` | Daily | OAuth required; cached |
| poe2scout exchange | Hourly | Lightweight |
| poe.ninja PoE2 builds | Daily | For meta-build aggregation |

### Cross-validation

For mod weights, we hold both Craft of Exile's number AND poe2db.tw's number. The bundle records both with a `confidence` annotation:

- `verified` — both sources agree within ±5%
- `community` — sources within ±25%
- `experimental` — sources differ >25%, or only one source has it

The advisor surfaces the confidence in the UI ("this probability has ±X% uncertainty").

## Licensing implications

- **poe2db.tw is CC BY-NC-SA**. We use it for personal/non-commercial. If we ever monetize, we either replace those datapoints or obtain permission from poe2db's maintainers.
- **Craft of Exile**'s `poec_data.json` is publicly fetched without explicit license; community convention is permissive use with attribution. We attribute in `60-licensing.md`.
- **GGG ToS** forbids reverse-engineering undocumented endpoints. We use only OAuth-registered `/trade/data/*` and `/trade2/*` paths once approved.
- All data ultimately belongs to GGG; we redistribute only what RePoE-fork already publishes under their disclaimer.

## Cross-source schema reconciliation

Different sources name things differently. Examples:
- "Hinekora's Lock" vs "Hinekora's Lock currency" vs internal id "MetadataItemsCurrencyHinekorasLock"
- Item class names: poe2db says "Body Armours"; RePoE-fork says "BodyArmour"

The pipeline normalizes everything to a canonical id space declared in the bundle schema (`docs/21-bundle-schema.json` once M2 ships).

## Failure modes

- RePoE-fork is down → pipeline retries; falls back to last cached bundle.
- Craft of Exile changes URL → manual intervention; tracked in `pipeline/sources.toml`.
- poe2db.tw layout change → scraper test breaks; we get an alert.
- GGG OAuth approval delay → pipeline omits stat-id mapping; advisor warns "trade integration disabled".

## Alternative considered: self-extract from GGPK

We could run `ggpk-explorer` on every patch and bypass community sources entirely. Considered, deferred:
- Requires ~150 GB disk for the unpacked GGPK.
- GPL-3 license incompatibility for direct dependency (we can ship a separate self-host extractor binary if needed).
- The community sources are kept fresh by people who already do the extraction work — duplicating effort is wasteful.

We may add a "self-host pipeline" mode in v2 for users who want zero external dependencies.
