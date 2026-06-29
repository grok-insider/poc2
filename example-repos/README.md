# example-repos/

Read-only reference clones of upstream projects we study but do not depend on at the source-code level.

These directories are **not tracked by git** (see `.gitignore`). They are recreated by running `scripts/clone-example-repos.sh` from the project root.

## Repos

| Directory | Upstream | License | Why we keep it |
|---|---|---|---|
| `repoe-fork/` | https://github.com/repoe-fork/repoe | NOASSERTION (data: GGG) | Source of the canonical PoE2 JSON dump (mods, bases, tags). Published to `repoe-fork.github.io/poe2/`. |
| `poe-tool-dev-dat-schema/` | https://github.com/poe-tool-dev/dat-schema | MIT | Source-of-truth schema for raw `.datc64` tables. |
| `pyoe2-craftpath/` | https://github.com/WladHD/pyoe2-craftpath | MIT | Reference Rust crate for path-finding crafting; potential dependency. |
| `POE2_HTC/` | https://github.com/Dboire9/POE2_HTC | AGPL-3.0 | Reference only — best example of beam-search optimal-path crafting. **Cannot copy code (license incompatibility with our MIT)**. |
| `XileHUD-poe_overlay/` | https://github.com/XileHUD/poe_overlay | GPL-3.0 | Reference for clipboard parsing, Client.txt monitoring, local data layout. **Cannot copy code (license incompatibility)**. |
| `Exiled-Exchange-2/` | https://github.com/Kvan7/Exiled-Exchange-2 | MIT-ish | Reference for trade API integration patterns. |
| `awakened-poe-trade/` | https://github.com/SnosMe/awakened-poe-trade | MIT | Reference for overlay item capture (Ctrl+C→clipboard), price-check UI/flow, trade-API stat matching. |
| `ggpk-explorer/` | https://github.com/juddisjudd/ggpk-explorer | GPL-3.0 | Emergency self-extraction tool if RePoE-fork breaks. **Cannot copy code (license incompatibility)**. |
| `poe-dat-viewer/` | https://github.com/SnosMe/poe-dat-viewer | (open) | Reference for `.datc64` parsing in TypeScript. |
| `LocalIdentity-poe2-data/` | https://github.com/LocalIdentity/poe2-data | (data: GGG) | Raw `.dat` dumps as cross-check against RePoE-fork. **~1.1 GB**. |

## Clone all

```bash
./scripts/clone-example-repos.sh
```

The script clones each repo with `--depth 1`. Total disk: ~1.5 GB.

## License notes

- Code copied from MIT/Apache repos into our codebase must preserve their copyright notice.
- **Code from GPL/AGPL repos must NOT be copied** into our MIT codebase. We may study, learn from, and re-implement — but never copy verbatim.
- Game data extracted from GGPK belongs to GGG; we redistribute only what RePoE-fork already has under their disclaimer.
