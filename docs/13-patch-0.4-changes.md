# Patch 0.4 — "Fate of the Vaal" / "The Last of the Druids"

> Distilled from the research pass kicked off in the planning phase. Authoritative sources: official patch notes thread `pathofexile.com/forum/view-thread/3883495` plus subpatches 0.4.0a through 0.4.0j.
>
> Released: **2025-12-12**. Latest known subpatch: **0.4.0j** (~2026-04-20).

## What 0.4 IS called

Two names refer to the same patch:
- **Expansion** (permanent content): `0.4.0 — The Last of the Druids`
- **Challenge League** (~3-4 month temporary): `Fate of the Vaal`

The official patch-notes thread title is "Content Update 0.4.0 — Path of Exile 2: The Last of the Druids". Streamers and most community sites refer to it as "Fate of the Vaal".

## Disabled in 0.4 (legacy stockpile only)

The single most consequential change for crafting strategy:

- **Omen of Homogenising Exaltation** — drop disabled
- **Omen of Homogenising Coronation** — drop disabled

Existing copies in player stashes still function in trade league. The engine encodes this via `patch_range = PatchRange::until(PatchVersion::new(0, 3, 255))` on `Omen::homogenising_exaltation()` and `Omen::homogenising_coronation()`. `OmenSet::consume_homogenising(0.4.0)` returns `None` even when the omen is in the active set; `OmenSet::consume_homogenising(0.3.0)` returns `Some(...)` and removes it.

This kills the dominant 0.3-era "5T1 guaranteed via tag-matching Exalts/Regals" pipeline. Belton's Four-T1 Rubric (December 2025 video) is the canonical replacement; it's encoded in `/docs/33-strategy-library.md`.

## New crafting items

| Item | What it does |
|---|---|
| **Architect's Orb** | Itemized double-corrupt for equipment / jewels (was Atziri's Temple Sacrificial Chamber Tier 3 only). 50% chance to destroy the item. Currency Exchange tradeable. |
| **Crystallised Corruption** | Same idea for skill gems. |
| **Vaal Cultivation Orb** | Reroll mods on a corrupted unique. On Vaal-themed uniques, samples from a special pool (incl. unique-specific Vaal mods). |

Engine status (M2 baseline): not yet modeled. The data structure exists (`item.corrupted` is the gate) but Architect's Orb specifically requires double-corrupt outcomes that need data not yet in the bundle.

## New weapon class: Talisman

Two-handed shapeshifting martial weapon, Druid-flavored. Recombinator-compatible (per 0.4.0b). Engine status: not yet modeled (no data class for shapeshift forms / form-specific basic skills).

## Limit: 1 across most named runes / soul cores / idols

Stacking-the-same-rune strategies are dead in 0.4. Engine status: tracked via `Item.sockets` only; the Limit: 1 enforcement happens at the data layer (each rune mod's tags include the relevant restriction) and isn't yet checked by the engine.

## Renames (vocabulary)

The engine and pipeline use the new names:
- `Rune Socket` → **Augment Socket**
- `Augment Talisman` (the old socketable) → **Idol** (frees "Talisman" for the new weapon class)

The `Item.sockets[].augment` field uses the post-rename `AugmentSlot::{Rune, SoulCore, Idol}` enum.

## Essence rebalances

Per the 0.4.0 patch-notes Item Changes section:
- **Lesser/Essence/Greater Essence of Haste** — now adds attack speed to **bows + crossbows** (previously melee-only)
- **Perfect Essence of Sorcery** — can no longer apply to Foci (only Wands and Staves)
- **Lesser/Essence/Greater Essence of Sorcery** — significantly buffed spell damage values
- **Perfect Essence of Battle** — nerfed from `+4 levels` to `+3` on 1H/bows; `+6` to `+5` on 2H/crossbows
- **Essence of Horror** (gloves/boots) — `100% increased effect of Socketed Items` → `60%`

Engine status: when the poe2db pipeline pass lands, these per-essence values will be ingested. Today the engine's `Essence` currency takes a target_mod ModId; the actual mod's value range is whatever the bundle has.

## Bow arrow-count overhaul

The old `Bow Attacks fire X additional Arrows` mod is **disabled**. Replaced by a "Surpassing Chance" tier system:
- "of Surplus": 25-50%
- "of Splintering": 75-100%
- "of Shards": 125-150%
- "of Many": 175-200%

Surpassing-chance > 100% always fires +1 arrow, with extra chance for +2.

Engine status: this is data-driven; comes online when the bundle ships these mods.

## Foci nerf

Top 2 tiers of spell damage and certain other mods can no longer roll on Foci. Engine status: data-driven; bundle's `allowed_item_classes` already filters this once the data lands.

## Other mod changes

Many staff spell-skill +level mods stepped down by 1 level:
- "of Coals" +2 → +1, "of Cinders" +3 → +2, "of Flames" +4 → +3 / +4
- "of the Mage" +2 → +1 (level requirement raised 5 → 10)
- "of the Enchanter" +3 → +2, "of the Evoker" +4 → +3 / +4

Various unique nerfs (Indigon, Effigy of Cruelty, Rathpith Globe, Tailwind, Perfidy) and one buff (The Anvil block ranges).

## Subpatch index (chronological)

| Patch | ~Date | One-line summary |
|---|---|---|
| 0.4.0   | Dec 12 2025 | Main release |
| 0.4.0 Hotfix 2 | Dec 13 2025 | Crash fixes |
| 0.4.0b  | Dec 15 2025 | Buffed Vaal Unique mods, Talisman recombinate fix |
| 0.4.0c  | Dec 19 2025 | Major Temple rebalance, new Medallions |
| 0.4.0d  | ~Jan 14 2026 | Death-during-Architect respawn fix |
| 0.4.0e  | Feb 09 2026 | Temple disconnection fix |
| 0.4.0f  | Feb 26 2026 | Map portal "(Complete)" markers |
| 0.4.0g  | Mar 06 2026 | Server maintenance + account fixes |
| 0.4.0h  | Mar 16 2026 | Controlled Metamorphosis ring fix |
| 0.4.0i  | ~Mar 26 2026 | Race event support |
| 0.4.0j  | ~Apr 20 2026 | Async Trade price-mismatch warning fix |

The pipeline records source revisions per fetch (`SourceRevisions` in the bundle header), so consumers can pin to a specific RePoE-fork commit if needed.

## What this means for the advisor

When 0.5 ships (announced for May 29 2026):
1. The pipeline rebuilds against the new RePoE-fork data.
2. New entities appear (their `patch_min = 0.5.0`).
3. Removed-from-pool entities — like Homogenising omens did in 0.4 — get a `patch_max` set on their data row.
4. The desktop app downloads the new bundle. No code changes required for the engine.
5. Strategies in the strategy library (M3) carry their own `patch_min` / `patch_max`; the advisor filters out 0.3-era strategies that depended on Homogenising omens when targeting 0.4+ crafts.

The point of the patch-versioning architecture (ADR-0006) is that 0.5 is a config swap, not a rebuild.
