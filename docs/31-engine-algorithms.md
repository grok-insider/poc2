# Engine Algorithms

> How the apply-paths actually work. Read alongside [`30-domain-model.md`](30-domain-model.md) (types) and [`11-game-mechanics.md`](11-game-mechanics.md) (mechanics).

## Mod sampling: `sample_eligible_mod`

Every "add a mod" currency funnels through this routine in `crates/engine/src/currency/basic.rs`:

```text
function sample_eligible_mod(registry, item, affix, rng, patch, min_level):
    candidates = registry.for_class_affix(item.class, affix)
    occupied_groups = collect_occupied_groups(registry, item)

    eligible = []
    for idx in candidates:
        m = registry.at(idx)
        if m.kind != Explicit:                      continue
        if m.required_level < min_level:            continue
        if m.required_level > item.ilvl:            continue
        if not m.patch_range.contains(patch):       continue
        if occupied_groups.contains(m.mod_group):   continue
        if total_weight_for_item(m, item) == 0:     continue
        eligible.push((idx, total_weight_for_item(m, item)))

    if eligible.empty: return None

    total = sum(weights)
    pick = rng.gen_range(0, total)
    return weighted_pick(eligible, pick)
```

Filters in order:
1. **Kind**: only `Explicit` mods roll via this path (implicits/enchants/desecrated have other paths)
2. **Level gate**: `min_level <= required_level <= item.ilvl`
3. **Patch range**: entity must be valid in current patch
4. **Mod-group exclusivity**: don't roll a mod whose group is already occupied
5. **Spawn weight > 0**: tag-eligibility check

Weighted pick uses cumulative-weight sampling — stable under permutations of the eligible list.

## Mod-group exclusivity

The `occupied_groups` lookup walks `item.prefixes ∪ item.suffixes`, asks the registry for each `ModRoll`'s group, and dedups. Hybrid mods sit in their own group (e.g., `BaseLocalDefencesAndLife`) distinct from singleton groups (`IncreasedLife`, `IncreasedEnergyShield`), so a hybrid `ES+Life` doesn't lock out a singleton `Life`.

## Affix-slot picking

`pick_open_affix(item, rng, max_slots)` returns:
- `Some(Prefix)` if only prefix slot open
- `Some(Suffix)` if only suffix slot open
- `Some(uniformly chosen)` if both open
- `None` if both full

`pick_open_affix_with_omen` first consumes a `Sinistral/Dextral *` omen if present, forcing the affix; otherwise delegates to the unbiased version.

## Hidden desecrated mods + Fracturing Orb

The user's worked-example invariant: **the hidden desecrated slot counts toward the 4-mod requirement but is never the fracture target.**

```text
fn apply_fracturing_orb(item, ctx):
    if not item.is_modifiable():
        return Err(InvalidApplication)

    total = item.fracturing_eligibility_count()
        # = visible_explicit_mod_count() + (hidden_desecrated.is_some() ? 1 : 0)
    if total < 4:
        return Err(InsufficientMods { required: 4, actual: total })

    # Sample space is visible non-fractured mods ONLY
    candidates = []
    for (i, m) in item.prefixes.enumerate(): if !m.is_fractured: candidates.push(Prefix, i)
    for (i, m) in item.suffixes.enumerate(): if !m.is_fractured: candidates.push(Suffix, i)
    if candidates.empty():
        return Err(FractureHiddenMod)

    pick = rng.gen_range(0, candidates.len())
    item[candidates[pick]].is_fractured = true
    Ok()
```

Statistical test (500 trials with 3 visible prefixes + 1 hidden suffix): hidden survives **every** trial; a prefix is fractured **every** trial. The 2/3-chance-per-T1-ES-prefix odds the user's example references emerge naturally from the uniform distribution over the 3 visible mods.

## Fractured-mod immutability

Three paths enforce this:
- **Annul**: `collect_removable_filtered` skips `is_fractured` mods
- **Chaos**: same skip in the removal step
- **Divine**: `reroll_explicit_values` skips `is_fractured` mods (their values are locked forever)

`collect_removable` thus returns the empty list when only fractured mods remain, and the calling currency errors with `InvalidApplication`.

## Hinekora's Lock — preview-and-commit

The lock is a `Option<u64>` seed on the item. The orchestration layer (`crate::engine`) intercepts `apply_currency` / `preview_currency`:

```text
fn apply_currency(currency, item, registry, rng, patch, omens):
    if item.hinekora_lock.is_some():
        seed = item.hinekora_lock.unwrap()
        locked_rng = Xoshiro256PlusPlus::seed_from_u64(seed)
        snapshot = omens.clone()
        ctx = ApplyContext::new(registry, &mut locked_rng, patch, omens)
        result = currency.apply(item, &mut ctx)
        if result.is_ok():
            item.hinekora_lock = None      # consumed
        else:
            *omens = snapshot              # rollback
        return result
    else:
        # Normal path: live RNG, snapshot omens for rollback
        snapshot = omens.clone()
        ctx = ApplyContext::new(registry, rng, patch, omens)
        result = currency.apply(item, &mut ctx)
        if result.is_err():
            *omens = snapshot
        return result

fn preview_currency(currency, item, registry, rng, patch, omens):
    clone = item.clone()
    omens_clone = omens.clone()
    if clone.hinekora_lock.is_some():
        seed = clone.hinekora_lock.unwrap()
        locked_rng = Xoshiro256PlusPlus::seed_from_u64(seed)
        ctx = ApplyContext::new(registry, &mut locked_rng, patch, &mut omens_clone)
        currency.apply(&mut clone, &mut ctx)?
    else:
        ctx = ApplyContext::new(registry, rng, patch, &mut omens_clone)
        currency.apply(&mut clone, &mut ctx)?
    return Ok(clone)
```

Determinism guarantee: `preview_currency(c, item)` followed by `apply_currency(c, item)` yields the same result for `item` (modulo `hinekora_lock = None` after commit). Both runs use the same locked RNG seed.

## Omen consumption

`OmenSet::consume<F>(patch, pred)` is a find-and-remove operation:

```text
fn consume(patch, pred):
    pos = active.position(|o| pred(o.effect) && o.patch_range.contains(patch))
    if pos.some(): return Some(active.remove(pos))
    return None
```

The patch-range check is critical: a Homogenising Exaltation in a player's stash is `Some(Omen)` in the active set, but `consume_homogenising(0.4.0)` returns `None` because the omen's range is `patch_max = 0.3.x`. The player keeps the omen (it's still in the inventory) but it doesn't fire on 0.4 crafts.

Currencies consume omens during their apply. Successful applies commit the consumption; failed applies (returning `Err`) trigger an omen-set rollback in the orchestration layer.

## Concept classification

`crate::analyzer::analyze(&mut [ModDefinition], &dyn Classifier)` populates `concept_set` and toggles `ModFlags::HYBRID`:

```text
for m in mods:
    set = []
    for stat in m.stats:
        c = classifier.classify(stat.stat_id)
        if not set.contains(c): set.push(c)
    m.concept_set = set
    if set.len() > 1: m.flags.set(HYBRID) else m.flags.clear(HYBRID)
```

`BuiltInClassifier` runs ~30 pattern-match rules against the stat-id. Common cases:

| Stat-id pattern | Concept |
|---|---|
| `*energy_shield*` | EnergyShield |
| `base_maximum_life`, `*life_regeneration*` | Life |
| `*all_resistance_*` | AllResistances |
| `*fire*resistance*` | FireResistance (and likewise for cold/lightning/chaos) |
| `additional_strength` | Strength (and likewise for dex/int) |
| `*added_X_damage*`, `*minimum_added_X*`, `*maximum_added_X*` | AddedXDamage (X ∈ {fire, cold, lightning, chaos, physical}) |
| `*movement_velocity*` | MovementSpeed |
| `*critical_strike_chance*`, `*critical_hit_chance*` | CritChance |
| `*level_of_all_X_skills*` | XSkillLevel |
| (no match) | Other |

`CompositeClassifier` adds a bundle-level override layer: `bundle.concept_map` (loaded from a TOML file in the data pipeline) wins over the built-in rules. This lets the pipeline correct misclassifications per patch without code changes.

Hybrid examples (drawn from RePoE-fork classification, 203 mods total):
- `LocalIncreasedEnergyShieldAndLife1` → `{EnergyShield, Life}`
- `LocalIncreasedEvasionAndManaSimple1` → `{Evasion, Mana}`

Atomic-but-multi-stat examples:
- `AddedFireDamageOnHelmet1` (`min_added_fire`, `max_added_fire`) → `{AddedFireDamage}` only

## Vaal Orb outcome distribution

```text
fn sample_vaal_outcome(rng):
    match rng.gen_range(0..6):
        0 => NoChange
        1 => RerollValues       # divine-like
        2 => BrickMods           # placeholder: drops non-fractured mods
        3 => AddEnchantment      # placeholder until corrupted-domain data lands
        4 => AddSocket           # over the cap
        5 => AddQuality          # +5 up to +30
```

Uniform 1/6 today. Omen of Corruption (M2.6 wiring pending) will collapse `NoChange` into a re-roll over the other 5 outcomes.

Statistical test: 600 samples across all 6 variants. The test seeds Xoshiro256PlusPlus with a fixed value and asserts every variant appears at least once.

## Where the engine *doesn't* sample

Several decision points pass to the **caller** rather than sampling internally:

- **Reveal at Well of Souls**: the engine's `sample_reveal_options` returns N candidates; the caller (UI / advisor) chooses one and calls `reveal_at_well_of_souls(chosen)`. This is intentional — the player has agency at this step.
- **Hinekora's Lock commit**: the engine never auto-commits; the caller invokes `apply_currency` after seeing the preview.
- **Bone affix when no Necromancy omen and both slots open**: random uniform pick. The advisor can pre-activate Sinistral/Dextral Necromancy to force the choice.

These caller-decisions are exactly the points where the strategy library and advisor live.

## Performance notes (M2.9 pending)

Current behavior:
- `apply_currency` runs in O(N_eligible_mods + lookups). On the live 0.4 bundle (2123 mods, ~50-200 eligible per affix per item-class), this is microseconds in release mode but unmeasured.
- No memoization yet. The advisor's beam-search will benefit from caching `(canonical_item, currency, omens) → (post_state, probability)` but that's a future commit.
- Allocations: `SmallVec` is used everywhere likely-small (mod slots, eligible lists, omen sets). Hot paths should not allocate beyond stack space for typical items.

The M2.9 perf pass will (a) write a bench harness, (b) memoize state evaluations, and (c) intern IDs to bring `Box<str>` down to `u32` indices.
