# Pipeline scripts

Helpers for refining the `poc2-pipeline` data joins.

## CoE → engine mod-id alias workflow (A.3)

The `pipeline/data/coe_aliases.toml` table lets you teach the pipeline
explicit mappings between Craft of Exile's `name_modifier` strings and
RePoE-fork's `ModId`s. Run the diagnose subcommand to discover which
mods still need aliasing:

```bash
# Build a fresh bundle (network required for live CoE feed).
cargo run --release -p poc2-pipeline -- build \
  --out /tmp/poc2.bundle.json.gz --patch 0.4.0 --skip-validation

# Diagnose: shows the top-N unmatched CoE mod names by frequency.
cargo run --release -p poc2-pipeline -- diagnose-coe /tmp/poc2.bundle.json.gz --limit 100

# Or with a local CoE snapshot (skip the network):
cargo run --release -p poc2-pipeline -- diagnose-coe /tmp/poc2.bundle.json.gz \
  --coe-file ~/poec_data.json --limit 100
```

For each high-frequency unmatched name:

1. Look up the corresponding engine mod via
   <https://poe2db.tw/us/> or the live RePoE-fork `mods.json`.
2. Add an entry to `pipeline/data/coe_aliases.toml`:
   ```toml
   [[alias]]
   coe_name = "Adds # to # Physical Damage to Spells"
   engine_mod_id = "AddedPhysicalDamageToSpells"
   note = "added in 0.4 patch"
   ```
3. Re-run `pipeline build` and `diagnose-coe`. The new alias should
   move that mod from "unmatched" to "via_alias" in the report.

The target join rate is **≥ 80%**. The four-tier strategy
(alias > essence cross-reference > name substring > template tokens)
typically gets the seed alias table to ~70%; hand-curating the long
tail closes the gap.
