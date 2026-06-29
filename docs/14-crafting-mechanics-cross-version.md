# Crafting Mechanics — Cross-Version Reference (0.3 / 0.4 / 0.5)

> Most crafting *logic* is identical across 0.3, 0.4, and 0.5; what changes
> is the **item set** and a handful of **gates** (which currency/omen is
> legal, what floor a variant enforces, what's disabled in trade league).
> This document is the single source of truth for those deltas. The engine
> resolves each gate from `(PatchVersion, League)` carried on `ApplyContext`.
>
> The invariant mechanics (weights, ilvl pools, tiers, Min-Modifier-Level
> floors, essence/desecration models) are documented in
> `docs/11-game-mechanics.md`. This file only covers what *differs* by
> version.

## Patch identity

| Patch | Expansion name | League | Released | Engine const |
|---|---|---|---|---|
| 0.3.0 | The Third Edict | Rise of the Abyssal | 2025-08-?? | — |
| 0.4.0 | The Last of the Druids | Fate of the Vaal | 2025-12-12 | `PATCH_0_4_0` |
| 0.5.0 | Return of the Ancients | Runes of Aldur | 2026-05-29 | `PATCH_0_5_0` |

## League model

`League` is carried on `ApplyContext` alongside `PatchVersion` because some
items function in **Standard** but not in the **current trade/challenge
league**. The default is the current challenge league (Runes of Aldur for
0.5).

| League | Meaning |
|---|---|
| `Standard` | Permanent league; legacy items still function. |
| `Challenge` | Current temporary league (Runes of Aldur in 0.5). Trade-relevant. |
| `HardcoreStandard` / `HardcoreChallenge` | HC variants; same crafting rules. |

## What changed in 0.3 "The Third Edict"

Crafting overhaul. Baseline for everything the engine models.

- **Essence rework.** 4 tiers: Lesser / Normal / Greater / Perfect.
  Lesser/Normal/Greater upgrade Magic→Rare adding a *specific* mod (not a
  tag-random one). Perfect (and Corrupted) remove a random mod then add a
  specific one on Rare. +7 new essence types; Essence of Torment → Abrasion.
- **Greater / Perfect orb tiers** added for Transmute/Aug/Regal/Chaos/Exalt
  — same effect, but constrain the *added* mod to a Minimum Modifier Level.
- **Desecration / Abyssal Bones.** Add a hidden Desecrated mod; reveal at
  the Well of Souls (choose 1 of 3). Bone size = ilvl gate (Gnawed ≤ ilvl64,
  Preserved any, Ancient Min-Mod-Level 40). Lord omens (Liege/Sovereign/
  Blackblooded) on Weapons/Jewellery only.
- **Hinekora's Lock** added (preview = commit).
- **Omen of Sanctification** added (rolls 80–120%, then locks the item).
- **Exceptional bases** (quality > 20%, extra sockets).
- **Removed omens:** Greater Annulment, Dextral Alchemy, Sinistral Alchemy,
  Dextral Coronation, Sinistral Coronation. → gate `PatchRange::until(0.2.x)`.

## What changed in 0.4 "Fate of the Vaal"

- **Disabled (legacy stockpile only):** Omen of Homogenising Exaltation,
  Omen of Homogenising Coronation. → `PatchRange::until(0.3.x)`; not consumed
  in 0.4+. (Already modeled.)
- **New crafting items:** Architect's Orb (itemized double-corrupt, 50%
  destroy), Crystallised Corruption (gems), Vaal Cultivation Orb (reroll mods
  on corrupted Vaal uniques).
- **Vocabulary:** Rune Socket → Augment Socket; old Augment Talisman → Idol.
- **Essence rebalances** (Haste→bows/crossbows; Sorcery no longer on Foci;
  Battle nerf; Horror 100%→60%; etc.).

## What changed in 0.5 "Return of the Ancients"

Major crafting overhaul. Default engine league = Runes of Aldur.

### Removed / disabled
- **Recombinator DISABLED** (and Omen of Recombination removed; existing
  copies deleted on login). → gate `PatchRange::until(0.4.x)`; in 0.5 the
  advisor must never recommend it, and `can_apply_to` rejects it in
  Challenge league. (Still works in Standard via legacy, hence the `League`
  gate.)
- **Omen of Corruption, Homogenising Exaltation, Homogenising Coronation:**
  Standard-league only in 0.5. → `League::Standard` gate.
- Expedition disabled on Standard during the league; legacy artifacts
  removed from Currency Exchange.

### Economy / floor shifts (affects valuator + Min-Mod-Level constants)
- Divine Orbs more common; Greater & Perfect currencies rarer; Transmutation
  & Augmentation significantly rarer.
- **Greater Orbs of Transmutation and Augmentation now require lower
  modifier levels** (floor lowered). → P2 must source floors from data, not
  the historical engine constants.

### Omen changes
- **Omen of Chaotic Rarity / Quantity / Monsters inverted** — now *prevent*
  that type instead of guaranteeing it (waystone-scoped).
- **New Omen of Chaotic Effectiveness** (waystone Chaos, excludes Monster
  Effectiveness mods). Up to 3 of these may be active simultaneously.

### New crafting systems (P5 scope)
- **Verisium Runeforging:** Runic Ward defense; **13 Alloy currencies** that
  replace an existing mod with a crafted one (essence-like). Verisium metal
  is the resource. Unique Runeforging upgrades low-level unique bases.
- **Genesis Tree:** consumes Wombgifts/Hiveblood to craft jewellery +
  currency. Adds **6 ring / 4 amulet / 4 belt base types** craftable only via
  the tree, plus new caster/minion mods on rings/belts. **Catalysts no longer
  drop** — Genesis-Tree only; **12 new Jewel catalysts** added.
- **Liquid Emotions** → craft mods on Jewels (greater-essence-like). **Ancient
  Emotions** (+ Potent) → Timelost Jewels.
- New weapon classes present in data: Flail, Claw, Dagger, Warstaff.

### Live data confirmation (RePoE-fork tags, 0.5)
`verisium_{common,uncommon,rare,mythic,crafting}`, `runic_ward`,
`ward_armour`, `runic_core`, `genesis_tree_caster`, `genesis_tree_minion`,
`expedition_faction_runes_of_aldur`. Mod ids like `GenesisTreeRing*Crafted`,
`Verisium*`. Domains include `desecrated`, `crafted`, `instilled`,
`talisman`.

## Gate cheat-sheet (engine resolution)

| Mechanic | Gate |
|---|---|
| Homogenising omens | `PatchRange::until(0.3.x)` |
| Removed 0.3 omens (Greater Annul, Dextral/Sinistral Alchemy/Coronation) | `PatchRange::until(0.2.x)` |
| Recombinator | `PatchRange::until(0.4.x)` OR `League::Standard` in 0.5 |
| Omen of Corruption (0.5) | `League::Standard` |
| Verisium / Genesis / Emotions / new catalysts | `PatchRange::from(0.5.0)` |
| Min-Mod-Level floors (Greater Trans/Aug lowered) | per-`PatchVersion` constant |

## Sources

- 0.3 patch notes: `pathofexile.com/forum/view-thread/3826682`.
- 0.4 patch notes: `pathofexile.com/forum/view-thread/3883495`.
- 0.5 patch notes: `pathofexile.com/forum/view-thread/3932540`;
  `poe2wiki.net/wiki/Version_0.5.0`.
- Weights provenance: `poe2db.tw/weightings`.
- Exalt variants / Min Mod Level: `poe2wiki.net/wiki/Perfect_Exalted_Orb`,
  `.../Greater_Exalted_Orb`.
- Desecration: `poe2wiki.net/wiki/Desecrated_Modifier`,
  `.../Omen_of_the_Liege`, `.../Omen_of_the_Sovereign`.
