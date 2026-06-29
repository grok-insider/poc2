# ADR-0012: Automated upstream data-refresh loop

- **Status:** accepted (2026-06-29, explicit user decision)
- **Complements:** [ADR-0003](0003-data-sources.md) (data sources & licensing),
  [ADR-0006](0006-patch-versioning.md) (patch-versioned entities).
- **Context:** PoE2 ships point patches frequently (0.5.0 → 0.5.4 within weeks),
  each adding/altering mods, bases, and uniques. The bundle's primary source —
  RePoE-fork's published `*.min.json` at `repoe-fork.github.io/poe2/` — is
  **unversioned**: every `build` silently tracks whatever upstream last
  published, with no signal for *when* a new patch's data actually landed and no
  record of *which* upstream snapshot a built bundle corresponds to. The user
  wants the project to notice new game data and surface it for ingestion without
  manual polling.

## The PoE2 data-mining ecosystem (what we evaluated)

Data flows through a layered open-source stack, raw → cooked:

| Layer | Project | Role for us |
|------|---------|-------------|
| GGG **patch CDN** | (GGG) | ultimate source of truth (`.datc64` + bundles) |
| Version pointer | `repoe-fork.github.io/poe2/version.txt` | **the trigger signal** (`4.5.4.x` = patch 0.5.4) |
| Schema | `poe-tool-dev/dat-schema` | needed only for direct-CDN extraction |
| Extractor | `SnosMe/poe-dat-viewer` → `pathofexile-dat` (npm) | reads `.datc64` straight from the CDN (future fallback tier) |
| **Cooked JSON** | **`repoe-fork/repoe`** (PyPoE) | **← our primary source, already consumed** |

Key finding: the project already sits on the best practical primary source
(RePoE-fork, actively maintained, last commit days old). What was missing is not
a new extractor but a **change-detection + diff + reporting loop** around the
existing pipeline. Note: `poe-tool-dev/latest-patch-version` is **PoE1-only**
(its `latest.txt` is the `3.28.x` line); the PoE2 version is published by
RePoE-fork itself at `poe2/version.txt`, which is ideal because it is the game
version the data we ingest was *generated from* (tightly correlated with our
content hashes).

## Decision

Add an in-repo, in-Rust **detect → report → PR** loop. Three additive pieces,
all confined to `pipeline/` + `.github/workflows/` (no engine/web/runtime
changes):

1. **`poc2-pipeline watch`** (`pipeline/src/watch.rs`) — fetches the PoE2
   patch pointer + SHA-256 of the three consumed RePoE-fork files, compares
   against the committed `pipeline/data/upstream_state.json`, and exits `0`
   (no change) or `10` (change) so CI can branch. `--write` persists the new
   state; `--report` emits a JSON verdict.

2. **`poc2-pipeline diff-bundle`** (`pipeline/src/diff.rs`) — semantic diff of
   two bundles (added/removed/changed mods, bases, tags, and section entries),
   keyed on stable ids, rendered to a markdown changelog for the PR body.

3. **`.github/workflows/data-watch.yml`** — a cron (every 6h) + manual
   `workflow_dispatch` workflow that runs `watch`; on change it rebuilds the
   `0.5.0` bundle, diffs it against the previous build (restored from CI cache),
   runs the `audit-matrix` sanity sweep, and opens a **draft PR** carrying the
   regenerated `upstream_state.json` + the "what's new" report.

Provenance: every built bundle now stamps the live PoE2 patch pointer into its
header `sources` (`poe2.patch_pointer`), so any bundle is traceable to the game
version it corresponds to.

## Why not (alternatives considered)

- **Direct GGPK / `.datc64` extraction as primary** — rejected for now. It
  removes the RePoE-fork timing dependency but means owning schema drift, table
  selection, and a Node extraction step. RePoE-fork is current and sufficient; a
  thin direct-CDN **fallback** tier (for the handful of mods RePoE doesn't yet
  export — e.g. the 5 Ancient-emotion targets) is noted as **future work**.
- **Fully automatic rebuild + commit/deploy** — rejected. The curated fixtures
  (`alloys.json`, `emotions.json`, `desecrated_mods.json`, Genesis
  `brequel_tree.json` / `genesis_meta.toml`) require hand-curated
  `engine_mod_id` joins that cannot be auto-generated reliably, and repo policy
  is that the user performs all merges. Hence: draft PR + human review.
- **Local-only Rust bin / systemd timer** — rejected as the *primary* host (ties
  detection to the dev machine being on), but the logic lives in a reusable
  library module, so a local `cargo run -p poc2-pipeline -- watch` works
  identically for ad-hoc checks.

## Consequences

- New patch data is noticed within ~6h and lands as a reviewable PR with a
  human-readable content diff, instead of requiring manual polling.
- The curated-fixture curation step stays explicitly human; the PR flags when
  new mod ids likely need hand joins.
- `pipeline/data/upstream_state.json` is a committed text file; its history is
  itself an audit log of when upstream moved.
- Future: promote `diff-bundle` output into `CHANGELOG.md` automation, and add
  the direct-CDN fallback extractor for un-exported tables.
