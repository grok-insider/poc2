# ADR-0005 — License: MIT

- Status: Accepted
- Date: 2026-04-26

## Decision

Path of Crafting 2 source code is licensed under **MIT**.

## Rationale

- **Permissive** — anyone can use, fork, modify, redistribute.
- **Compatible with Tauri** (Apache-2.0/MIT) and `pyoe2-craftpath` (MIT) — our two largest potential dependencies.
- **Compatible with the Rust ecosystem** — most crates are MIT/Apache.
- **Allows commercial use** if the project ever needs that flexibility.

## Compatibility constraints

| Reference repo | License | Allowed action |
|---|---|---|
| `repoe-fork/repoe` | NOASSERTION (data: GGG) | Read & consume the JSON output, attribute |
| `poe-tool-dev/dat-schema` | MIT | Copy with attribution |
| `pyoe2-craftpath` | MIT | Depend on, copy with attribution |
| `Dboire9/POE2_HTC` | **AGPL-3.0** | **Reference only — no code copy** |
| `XileHUD/poe_overlay` | **GPL-3.0** | **Reference only — no code copy** |
| `Kvan7/Exiled-Exchange-2` | MIT-ish | Copy with attribution |
| `juddisjudd/ggpk-explorer` | **GPL-3.0** | **Reference only — no code copy** |
| `SnosMe/poe-dat-viewer` | (open) | Reference; case-by-case for code copy |
| `LocalIdentity/poe2-data` | NOASSERTION (data: GGG) | Read & consume |

## Hard rule

**Never copy code from GPL/AGPL repos into our codebase.** We may study, learn, and re-implement — but never copy verbatim. If we accidentally do, we fix it before the next public release.

CI enforcement: M3+ adds a license-scanning step that fails on accidental GPL'd file inclusion.

## Data licensing (separate concern)

Game data we redistribute carries GGG's terms. Aggregation sources have their own licenses:

- **poe2db.tw** is **CC BY-NC-SA** — non-commercial. If the project ever monetizes, we either replace those datapoints, obtain permission, or move to a fully self-extracted pipeline.
- **Craft of Exile** `poec_data.json` is informally permissive. Attribution required in `docs/60-licensing.md`.

See [ADR-0003](0003-data-sources.md) for full data-source licensing.
