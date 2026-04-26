# PoE2 Crafting Mechanics Reference

> Quick reference for the mechanics the engine models. Patch baseline: **0.4 "Fate of the Vaal"** (released Dec 12 2025). See [`13-patch-0.4-changes.md`](13-patch-0.4-changes.md) for what changed in 0.4 and [`12-poe2-vs-poe1.md`](12-poe2-vs-poe1.md) for the PoE1→PoE2 diff.
>
> The deeper research catalogue lives in [`33-strategy-library.md`](33-strategy-library.md) (codified strategies) and [`34-heuristics-rulebook.md`](34-heuristics-rulebook.md) (~120 expert rules).

## Item rarities and slot caps

| Rarity | Max prefixes | Max suffixes |
|---|---|---|
| Normal | 0 | 0 |
| Magic | 1 | 1 |
| Rare | 3 (per item-class) | 3 (per item-class) |
| Unique | varies by unique | varies |

There is **no Orb of Scouring in PoE2** — once promoted past Normal, an item cannot be reverted.

## Affix types

- **Prefix** / **Suffix** — the rollable explicit slots
- **Implicit** — intrinsic to the base, untouched by most currencies
- **Enchantment** — added by runes / soul cores / Vaal corruption / certain omens

Desecrated mods occupy a Prefix or Suffix slot but carry `ModKind::Desecrated` and originate from a desecration bone.

## Currencies the engine models

### Basic orbs (rarity changers / mod modifiers)

| Currency | Input rarity | Effect |
|---|---|---|
| Orb of Transmutation | Normal | → Magic + 1 random mod |
| Orb of Augmentation | Magic with 1 mod | Add 1 mod (fills empty slot) |
| Orb of Alchemy | Normal | → Rare + up to 4 random mods |
| Regal Orb | Magic | → Rare + 1 random mod (existing preserved) |
| Exalted Orb | Rare with empty slot | Add 1 random mod |
| Chaos Orb (PoE2) | Rare | Remove 1 random non-fractured + add 1 random |
| Orb of Annulment | Magic / Rare | Remove 1 random non-fractured mod |
| Divine Orb | Magic / Rare / Unique | Reroll values of all non-fractured explicit mods |
| Vaal Orb | non-corrupted | Corrupt with 1 of 6 random outcomes |

### Greater + Perfect variants

`Greater {Transmute, Aug, Regal, Exalt, Chaos}` and `Perfect {Transmute, Aug, Regal, Exalt, Chaos}` behave as their base variants but constrain the *added* mod to `required_level >= MIN`:

| Variant | Min mod-level (engine constants) |
|---|---|
| Greater Transmutation | 35 |
| Greater Augmentation | 55 |
| Greater Regal | 50 |
| Greater Exalted | 50 |
| Greater Chaos | 50 |
| Perfect (any) | 70 |

### Specialty currencies

| Currency | Effect |
|---|---|
| **Fracturing Orb** | Locks one visible non-fractured mod immutably. Requires ≥ 4 explicit mods (hidden desecrated counts). Cannot target hidden mods. |
| **Hinekora's Lock** | Binds the next operation's RNG to a stored seed. Preview matches commit byte-for-byte. Refuses on corrupted/sanctified/mirrored items. |
| **Bone** (Gnawed/Preserved/Ancient × Jawbone/Rib/Cranium/Collarbone) | Adds a hidden desecrated mod slot to a Rare item. Reveal at the Well of Souls. |
| **Essence** (Lesser/Normal/Greater/Perfect/Corrupted × 19 types) | Adds a guaranteed specific mod. Lesser/Normal/Greater promote Magic→Rare; Perfect/Corrupted remove+add on Rare. |
| Catalysts (M2.5b — pending) | Tag-targeting quality on rings/amulets |
| Recombinator (M2.5c — pending) | 2-item combine |

## Omens (22 modeled)

The engine implements every crafting-relevant omen as one of seven [`OmenEffect`] variants:

| Effect | Omens |
|---|---|
| `AffixOnly(Prefix\|Suffix)` | Sinistral/Dextral × {Exaltation, Annulment, Erasure, Crystallisation, Necromancy} |
| `GreaterExaltation` | Greater Exaltation |
| `Whittling` | Whittling |
| `Light` | Light |
| `AbyssalEchoes` | Abyssal Echoes |
| `PreventNoChange` | Corruption |
| `Sanctification` / `Blessed` | Sanctification, the Blessed |
| `LordTarget(Kurgal\|Amanamu\|Ulaman)` | Blackblooded, Liege, Sovereign |
| `CatalystingExaltation` | Catalysing Exaltation |
| `HomogenisingTagMatch` | Homogenising Exaltation, Coronation (**disabled in 0.4** — `patch_max = 0.3.x`) |

Omens are added to the [`OmenSet`] and consumed one-shot by a compatible currency. The engine enforces patch-versioning: omens out of `patch_range` are silently NOT consumed (legacy stockpile semantics).

## Critical engine invariants

These are encoded as unit tests in `crates/engine/src/{item,currency}/*.rs`:

1. **Hidden desecrated mods count toward Fracturing Orb's 4-mod requirement** but are never the fracture target. → `Item::fracturing_eligibility_count()` includes hidden; `Item::fracture_targets()` excludes hidden.
2. **Fractured mods are immutable** — Annul cannot remove them, Chaos cannot remove them, Divine cannot reroll their values.
3. **Mod-group exclusivity** — at most one mod per `ModGroup` per item. Hybrid mods sit in their own group, distinct from singleton siblings, so a hybrid `ES + Life` does NOT lock out a singleton `Life` mod.
4. **Hybrid mods produce multiple `ConceptId` outputs from one affix slot.** Concept-based target matching means `target = { concept: "EnergyShield", min_tier: 1 }` accepts both pure-ES mods and ES-Life hybrids.
5. **Corrupted/Sanctified/Mirrored items reject most operations.** Vaal corruption is a one-way door; double-corruption only via Architect's Orb (M2.5d — pending).
6. **Hinekora's Lock + preview = commit.** With a lock active, `preview_currency` and `apply_currency` produce byte-identical results from the same seed. Lock is consumed on successful commit; preserved on failure.

## Worked-example reference flow

The user's "Triple T1 Energy Shield Body Armour Isolation" craft, in 10 engine-supported steps:

1. ilvl 82 Normal int/dexint base
2. Perfect Transmutation (target: any T1 ES)
3. Perfect Augmentation retry on miss
4. Perfect Regal; on bad outcome: 2× Annul + Chaos spam
5. Perfect Exalted Orb loop until 2× T1 ES prefixes
6. Perfect Exalted Orb for first suffix, then Preserved Rib + Omen of Dextral Necromancy for the hidden suffix
7. Optional Divine Orb for value polish, then Fracturing Orb (2/3 chance to lock a T1 ES prefix)
8. Reveal at Well of Souls; pair with Omen of Abyssal Echoes for a 3+3 choice
9. Perfect Essence of Seeking + Omen of Dextral Crystallisation (suffix swap)
10. Vaal Orb finish, optionally with Omen of Corruption to remove the no-op outcome

Each step is unit-tested in `crates/engine/tests/worked_example_es_body_armour.rs`.
