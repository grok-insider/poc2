# Crafting-Mechanics Fidelity (0.3 / 0.4 / 0.5) — Detailed Implementation Plan (v4)

**Status:** In progress.
**Author:** OpenCode session, drafted as a handoff document so a different
AI agent (or developer) can resume without context loss.
**Scope:** Engine crafting-mechanics fidelity across patches 0.3, 0.4, and
0.5, plus the 0.5 "Return of the Ancients" content migration. Builds on
`docs/72-v1-execution-plan.md` (v1), `docs/80-crafter-helper-v2-plan.md`
(v2, shipped), `docs/81-engine-training-and-rule-encoding-plan.md` (v3,
shipped).
**Reference docs added by this plan:** `docs/14-crafting-mechanics-cross-version.md`.

---

## 0. Why this plan exists

The user asked us to (a) understand the project, (b) find where sites like
poe2db.tw get their PoE2 data and gather the latest 0.5 data, and (c) raise
the *crafting-mechanics fidelity* of the engine so the advisor reasons about
the real game rules — specifically:

> "the weights are dependant of item level the pool changes based on that
> and which item version you use like exalt, greater exalt and perfect
> exalt will have different pools since some mods are not taken in count
> depending on item level or because they cant roll on low tier of a
> modifier"

This is exactly three mechanics, all confirmed against authoritative
sources during research:

1. **ilvl-dependent pools.** A modifier tier is eligible only when
   `required_level <= item.ilvl`. Raising ilvl unlocks higher tiers, so the
   rollable pool *grows with ilvl*.
2. **Min-Modifier-Level currency variants.** Exalted (no floor) vs Greater
   Exalted (floor 35) vs Perfect Exalted (floor 50) produce *strictly
   nested shrinking* pools. Same for Greater/Perfect Transmute/Aug/Regal/
   Chaos.
3. **Inclusive tier weighting.** When you select/aim for a given tier, its
   effective spawn weight includes the weights of the higher tiers of the
   same mod-type that can roll at the current ilvl (`Σ_{j=m_i}^{m_t0} w_j`).
   The pool top `m_t0` is ilvl-dependent.

There is one subtle exception the user implied ("some mods are not taken in
count ... or because they cant roll on low tier"): the Min-Modifier-Level
floor never deletes an entire mod-*type*. Per the wiki, **"at least one tier
of each mod type will always be eligible, respecting item level"** — if every
tier of a group is below the floor (e.g. Light Radius whose max tier is L30,
under a Perfect floor of 50), the group's highest tier still rolls.

## 0.1 Data provenance (answers "where does poe2db get its data")

- PoE2 game data lives in GGG's `Content.ggpk` as binary `.datc64` tables.
  **GGG publishes no official data export.**
- Community extraction chain: `Content.ggpk` → community schema
  (`poe-tool-dev/dat-schema`, reverse-engineered, updated per patch) →
  extractors (PyPoE, `ggpk-tool`, `exiledb`, `poe-dat-viewer`) → JSON.
- **poe2db.tw** datamines the GGPK directly for mods/bases/art. **Weights
  are NOT in the game files**; poe2db imports Krakenbul / Prohibited Library
  recombinator-derived weights, and for non-recombinable bases (Charms,
  Jewels, Tablets, Waystones) it parses trade-site listings. Verbatim at
  `https://poe2db.tw/weightings`.
- **RePoE-fork** (our primary source) runs PyPoE + dat-schema and publishes
  JSON at `repoe-fork.github.io/poe2/`. **Already serving 0.5** (updated
  2026-05-22; `mods.min.json` ~8.7 MB).
- **Craft of Exile** (our weight source) uses the same recombinator method;
  `poec_data.json` updated 2026-05-30 (post-0.5), schema unchanged, carries
  `is_legacy` flag (usable for league gating).

**Conclusion: our pipeline's source architecture is correct and unchanged.**
The 0.5 work is a content + mechanics migration, not a re-architecture.

## 0.2 Locked decisions (user answers)

1. **Inclusive higher-tier weighting** adopted for normal sampling
   (Exalt/Aug/Regal/Chaos), `m_t0` ilvl-dependent.
2. **Best-effort models** for uncertain mechanics (desecration exclusive
   weights, Modifier-Tier-Rating) — chi-squared tests with wide tolerances,
   most-accepted community model (MTR Model 1). Exact probabilities stay
   `Confidence::Experimental`.
3. **Full test matrix**: deterministic + boundary + cross-version +
   proptests + distribution tests.
4. **Thread `PatchVersion` + `League` through `ApplyContext`** so one test
   body runs across 0.3/0.4/0.5. (`PatchVersion` is already threaded;
   `League` is new.)
5. **0.5 scope**: core refresh + Verisium Alloys + Genesis Tree + Jewel
   emotions/catalysts.
6. **Default league: Runes of Aldur** (Recombinator + Corruption/
   Homogenising omens disabled).

---

## 1. Authoritative mechanics reference (encode these exactly)

### 1.1 Weight / ilvl / tier
- Each tier is a separate `ModDefinition` in a `ModGroup`, ordered by
  `required_level`. An item holds at most one member per group.
- Spawn weight via tag resolution: match base tags against the mod's
  `spawn_weights`; **leftmost (most-significant) tag wins**; weight 0 ⇒
  discard. (poewiki Modifier; poe2wiki Template:Mod.)
- Tier eligibility: `required_level <= ilvl`.
- Inclusive tier weight: `SuccessChance ∝ Σ_{j=m_i}^{m_t0} w_j / w_Z` where
  `m_t0` = highest tier rollable at this ilvl, `w_Z` = total affix weight.
  (poe2wiki Recombinator §3; Belton "Modifier Weights Explained".)

### 1.2 Min-Modifier-Level floors (patch-versioned)
| Variant | 0.3 / 0.4 floor | 0.5 floor | Source |
|---|---|---|---|
| Exalted / Trans / Aug / Regal / Chaos (base) | none | none | wiki |
| Greater Transmutation | 35 | lowered (TBD from 0.5 data) | wiki/patch notes |
| Greater Augmentation | 55 | lowered (TBD) | wiki/patch notes |
| Greater Regal | 50 | 50 | wiki |
| Greater Exalted | 35 (wiki) / 50 (engine const today) | 35 | **wiki says 35** |
| Greater Chaos | 50 | 50 | wiki |
| Perfect (all) | 70 | 70 (Perfect Exalt **50** per wiki) | **see note** |

> **Note / discrepancy to resolve in P2:** the live wiki lists Greater
> Exalted = 35 and Perfect Exalted = 50. The engine currently hard-codes
> Greater Exalt = 50 and Perfect-all = 70 (`basic.rs:1303-1308`). P2 must
> reconcile the constants against the bundle's per-currency
> `Minimum Modifier Level` from RePoE-fork/poe2db rather than trusting the
> historical engine constants. Make the floors patch + currency specific,
> sourced from data where possible.

**Keep-≥1-tier exception:** if every tier of a mod-group is below the floor,
the group's highest tier (still `<= ilvl`) remains eligible.

### 1.3 Essences (0.3 rework; stable through 0.5)
- Lesser/Normal/Greater: Magic→Rare, add one fixed mod.
- Perfect/Corrupted: remove one random non-fractured mod, then add fixed mod
  (on Rare).
- Essence mod has its own `required_level`, can raise item level req.
- Family collision: illegal if the essence mod's group already present
  (checked after removal for remove+add).
- Affix-full forcing: suffix-essence with full suffixes removes a suffix;
  Sinistral/Dextral Crystallisation omens force the removed side.

### 1.4 Desecration / bones (0.3; stable 0.4/0.5)
- Bone size = ilvl semantics: **Gnawed ⇒ max ilvl 64; Preserved ⇒ any ilvl,
  no floor; Ancient ⇒ Min Modifier Level 40.**
- Subtype → class: Rib→armour; Jawbone→weapon/quiver; Collarbone→ring/amulet/
  belt; Cranium→jewel (Preserved only); Vertebrae→waystone (Preserved only).
- Reveal = choose 1 of 3. At **ilvl 65+**, ≥1 option is exclusive-desecrated
  **iff** the class has an exclusive for that affix (helmets have no
  exclusive prefixes). Exclusive mods use uneven weights; regular options use
  normal weights. (`best-effort` model + wide-tolerance tests.)
- **Lord omens (Liege=Amanamu, Sovereign=Ulaman, Blackblooded=Kurgal) are
  Weapon/Jewellery ONLY**, block the other two lords, and **brick the
  Ancient-bone Min-Mod-Level-40 floor** (consumed even if no lord mod is
  possible). Sceptres have no exclusive desecrated.

### 1.5 Cross-version deltas
See `docs/14-crafting-mechanics-cross-version.md` for the full matrix. Key
gates:
- 0.3: essence rework, Greater/Perfect orb tiers, desecration/bones,
  Hinekora's Lock, Sanctification, exceptional bases. Removed omens: Greater
  Annulment, Dextral/Sinistral Alchemy, Dextral/Sinistral Coronation.
- 0.4: Homogenising Exaltation/Coronation disabled; Architect's Orb / Vaal
  Cultivation; Rune Socket → Augment Socket rename; essence rebalances.
- 0.5: **Recombinator + Omen of Recombination removed**; Greater/Perfect
  rarer, Trans/Aug rarer, Divine more common (cost shifts); Greater Trans/Aug
  floors lowered; Verisium Alloys + Runic Ward; Genesis Tree (+ new
  ring/amulet/belt bases + caster/minion mods); Liquid/Ancient Emotions
  (jewel essence-like); 12 new jewel catalysts (catalysts no longer drop);
  Corruption / Homogenising omens Standard-only; Chaotic Rarity/Quantity/
  Monsters inverted + new Chaotic Effectiveness (waystone); new weapon
  classes (Flail/Claw/Dagger/Warstaff).

---

## 2. Engine surface map (current code anchors)

Confirmed by source audit:

- Weight resolution: `crates/engine/src/registry.rs:205-249`
  (`weight_for`, first-match-wins; scope-4 binary fallback at `:240-246`).
- Sampling: `crates/engine/src/currency/basic.rs:75-136`
  (`sample_eligible_mod`); ilvl+floor cut at `:99`; group occupancy at
  `:92,151-164`; weighted CDF pick at `:122-135`.
  `total_weight_for_item` thin wrapper at `:141-148`.
- Min-Mod-Level constants: `basic.rs:1303-1308`. Plumbing
  `add_one_mod_with_min` `:1312-1353`, `chaos_with_min` `:1357-1364`,
  macros `:1367-1453`, instantiations `:1456-1555`.
- ONLY-flag mask: `basic.rs:25-27` applied `:108-110`.
- Inclusive tier sum exists only in recombinator:
  `crates/engine/src/currency/recombinator.rs:336-356`.
- Tier model: implicit — `ModDefinition` has no tier field
  (`crates/engine/src/mods.rs:188-221`); ladder via `by_group`
  (`registry.rs:48,160-163`).
- `BaseAtIlvl` weight scope: wired + indexed but unfed by pipeline
  (`registry.rs:16-18,114-120,215-224`; `weights.rs:48-57`).
- `ApplyContext`: `crates/engine/src/currency.rs:165-202` — has `registry`,
  `base_registry`, `rng`, `patch`, `omens`. **`League` to be added.**
- Essence: `crates/engine/src/currency/essence.rs` (promoting :145-185,
  remove+add :190-256, family collision :169-180/:240-252).
- Bone reveal: `crates/engine/src/currency/bone.rs` (reveal :311).
- Patch model: `crates/engine/src/patch.rs` (`PatchVersion`, `PatchRange`;
  `PATCH_0_4_0`, `PATCH_0_5_0`).
- Bundle schema: `crates/data/src/lib.rs:71` (`BUNDLE_SCHEMA_VERSION = 2`).

### Gaps closed by this plan
1. No real tag-intersection weighting (binary fallback).
2. No keep-≥1-tier floor exception.
3. Inclusive tier weighting only in recombinator, not normal sampling.
4. `BaseAtIlvl` weights never fed by pipeline.
5. Tiers implicit (no tier ordinal).
6. Lord-omen class scope + Ancient-floor-brick not modeled.
7. No `League` in context.

---

## 3. Phases

### P0 — Mechanics ground-truth docs (no engine code)
- Refresh `docs/11-game-mechanics.md` to 0.5 baseline; add weight/ilvl/tier
  + Min-Mod-Level floor + keep-≥1-tier exception + bone-size ilvl + lord-omen
  scope.
- New `docs/14-crafting-mechanics-cross-version.md` — 0.3/0.4/0.5 delta
  matrix with `PatchRange` per mechanic.

### P1 — Weight & tier fidelity (engine core; highest leverage)
1. Add `League` enum (`crates/engine/src/patch.rs` or new `league.rs`);
   thread through `ApplyContext` (new field + `new_*` ctors default to
   `League::current()`).
2. Add explicit tier ordinal to `ModDefinition` (`tier: Option<u16>`,
   1 = highest). Keep `required_level` ordering as the fallback.
3. Tag-intersection weighting in `registry.weight_for` using `BaseRegistry`
   tags (leftmost-tag-wins) as a real scope before the binary fallback.
4. Inclusive higher-tier weighting in normal sampling — a
   `inclusive_weight_for(mod, item, ...)` summing group peers of the same
   affix with `required_level <= ilvl` and `>= mod.required_level`
   (mirrors recombinator's `Σ`), gated by `League`/config (default on).
5. Keep-≥1-tier floor exception in `sample_eligible_mod`: when a group is
   fully below `min_required_level`, retain its highest-`required_level`
   tier that is `<= ilvl`.
6. Feed `BaseAtIlvl` weights from the pipeline (P6 dependency; index already
   exists).

### P2 — Currency-variant pool correctness
- Reconcile Min-Mod-Level constants (see §1.2 note) — make them patch +
  currency specific, prefer data over historical constants. Patch-version
  the floors (0.5 lowered Greater Trans/Aug).

### P3 — Desecration / bone / lord-omen fidelity
- Bone size→ilvl semantics; reveal exclusive guarantee (ilvl65+, per-class-
  affix, best-effort weights); lord-omen Weapon/Jewellery-only +
  Ancient-floor-brick.

### P4 — Cross-version gating
- `PatchRange`/`League` gates: Recombinator ≤0.4 / Standard-only in 0.5;
  Homogenising ≤0.3; removed 0.3 omens; Corruption omen Standard-only in 0.5.

### P5 — New 0.5 systems
- Verisium Alloys (essence-like), Genesis Tree (+ new bases/mods), Liquid/
  Ancient Emotions, 12 new jewel catalysts.

### P6 — Pipeline 0.5 refresh + schema v3
- Rebuild from live RePoE-fork/CoE/poe2db; emit `BaseAtIlvl` weights; bump
  `BUNDLE_SCHEMA_VERSION` → 3; new mod kinds/flags; hard-reset migration.

### P7 — Docs sync + verification
- README/CHANGELOG/AGENTS to v2/v3 + 0.5. Full gate: `cargo fmt --check`,
  `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace`, `pnpm check`, `pnpm build`.

---

## 4. Exhaustive test plan (per-patch via context parameterization)

- `crates/engine/tests/weight_ilvl_pool.rs` (new): ilvl 1/64/82 pool deltas;
  tier boundary `req==ilvl` vs `+1`; leftmost-tag-wins; weight-0 discard;
  inclusive-tier "Tyrannical doubles at ilvl 82".
- `crates/engine/tests/min_mod_level_pools.rs` (new): strictly nested
  Exalt⊃Greater⊃Perfect; floor boundaries; keep-≥1-tier exception (Light
  Radius); empty-pool structured error; floor>ilvl.
- `crates/engine/tests/essence_mechanics.rs` (extend essence.rs tests):
  tier-rarity mechanics; family collision; affix-full forcing +
  Crystallisation; level-req raise; corrupted-item gating.
- `crates/engine/tests/desecration_mechanics.rs` (extend
  `desecration_gating.rs`): size×ilvl; reveal 3 + exclusive at 65+
  (+helmet-prefix negative); lord-omen scope + Ancient-floor-brick;
  already-desecrated error; Putrefaction; full-mods removal.
- `crates/engine/tests/cross_version_gating.rs` (new): Recombinator ≤0.4;
  Homogenising 0.3 only; removed-0.3 omens; Corruption Standard-only in 0.5;
  patch-varying floors.
- advisor: extend `no_illegal_currencies.rs`; no Recombinator in 0.5;
  chaos-spam EV uses ilvl-correct `p_success`.
- pipeline/data: schema-v3, `BaseAtIlvl` presence, 0.5 mod-kind coverage,
  CoE join threshold.
- proptests: never `req>ilvl`; never dup `ModGroup`; Greater/Perfect added
  mod `>= floor` unless exception; pool weight monotonic non-decreasing in
  ilvl; inclusive-tier sum `<=` full-affix weight.
- distribution (chi-squared, wide tol): weighted sampling ratio; MTR Model 1
  redistribution; desecration exclusive-vs-regular split.

---

## 5. Execution order & verification gates

P0 → P1 (+ P1 tests) → P2 (+ tests) → P3 (+ tests) → P4 (+ tests) → P6 (data)
→ P5 (new systems) → P7 (docs + final gate). Each phase ends with the
narrow crate tests, then `cargo test --workspace` when feasible. No phase is
"done" until its tests pass (the user's no-fails directive).

## 5a. Progress log

- **P0–P4 shipped and verified** (full workspace: 58 test blocks pass, 0
  failures; `cargo fmt --check` clean; `cargo clippy --workspace
  --all-targets -- -D warnings` clean).
  - P1: `League` enum threaded through `ApplyContext`; explicit `tier`
    ordinal on `ModDefinition` (`tier_strength_key`); inclusive higher-tier
    weighting (`ModRegistry::inclusive_weight_for`, consumed by
    `sample_eligible_mod`); keep-≥1-tier Min-Mod-Level floor exception.
    Tests: `weight_ilvl_pool.rs`, `min_mod_level_pools.rs`,
    `crafting_invariants_proptest.rs`.
  - P2: `MinModLevelVariant::floor(patch)` replaces the hard-coded consts;
    wiki-correct Greater Exalt = 35, Perfect Exalt = 50; 0.5 lowers Greater
    Transmute/Aug.
  - P3: bone size→ilvl (`BoneSize::max_ilvl` / `min_mod_level`); lord omens
    restricted to Weapon/Jewellery (`supports_lord_pool` = Jawbone |
    Collarbone); Ancient-floor-brick recorded on
    `HiddenDesecratedSlot.min_mod_level` and enforced at reveal. Tests:
    `desecration_mechanics.rs`, `essence_mechanics.rs`.
  - P4: `recombinator_available(patch, league)` + `recombine_gated`;
    `OmenSet::consume_prevent_no_change(patch, league)` gates the Corruption
    omen to Standard-only in 0.5. Test: `cross_version_gating.rs`.

- **P6 shipped:** bundle schema → v3; `assign_tier_ordinals` pipeline
  post-pass; **tag-intersection weighting** (`weight_for_on_base` +
  `spawn_weight_for_tags`, leftmost-tag-wins) using `BaseRegistry` tags.
  `BaseAtIlvl` weight emission remains an operator/pipeline-data concern
  (the index + resolution path are wired; the live rebuild feeds it).

- **League → advisor shipped:** `PlanInput.league` threaded end-to-end; the
  candidate generator drops `Recombine` candidates when
  `recombinator_available(patch, league)` is false (test:
  `candidate::tests::recombine_candidate_gated_by_league_in_0_5`).

- **P5 (new 0.5 systems) — shipped end-to-end:**
  - **Verisium Alloys:** `currency::Alloy` grew `class_targets` (each alloy
    grants a different crafted mod per item class — poe2db per-alloy tables);
    curated `pipeline/data/alloys.json` binds all 13 alloys × 132
    class-targets into `bundle.alloys` (v2 shape); the advisor's candidate
    generator proposes goal-relevant alloys via `CurrencyResolver::alloys()`.
    Alloy outputs are `ModKind::Crafted` and the 0.5 1-crafted-mod cap is
    enforced.
  - **Liquid/Potent/Ancient Emotions:** `Alloy::with_base_targets` (jewel
    base-name keys, exact match, uniform sample among same-base targets);
    `pipeline/data/emotions.json` (26 emotions × 96 targets, all bound) →
    `bundle.emotions` → `emotion_catalogue()`. The jewel mod pool (371 mods,
    domain `misc` upstream incl. `CraftedJewel*`/`JewelRadius*`) is now
    ingested under `ModDomain::Jewel`. Advisor candidates for emotions are
    deferred (need base-level item fidelity); engine apply works for items
    with real jewel bases.
  - **Genesis Tree:** UI-only per scope decision — committed
    `pipeline/data/brequel_tree.json` (248 passives) + curated
    `genesis_meta.toml` (stat templates, display-node overrides, womb
    metadata, 7 source-cited community presets) → `bundle.genesis` → WASM
    `genesisTree` → `GenesisPanel.tsx` (radial layout, PoE2-style frames +
    tooltips, preset highlighting, farming notes). Art via the
    `fetch-genesis-assets` bin. Guidance rules in
    `seed_rules/14_genesis_tree.toml`.
  - **0.5 mechanics deltas:** Vaal "unpredictable values" + Sanctification
    multiply instead of reroll in 0.5 (patch-gated, Experimental factors);
    Sanctification/Blessed omens now actually consumed by Divine; 1-desecrated
    cap at bone application (0.5+); Instant-Leech desecrated already absent
    from live 0.5 data.

- **Live 0.5 bundle rebuilt** with all of the above (3455 mods, 13 alloys,
  26 emotions, 248 genesis nodes, 12 catalysts) and shipped to
  `apps/web/public/poc2.bundle.json.gz`. League is user-switchable from
  Settings (WASM `setLeague`, persisted, re-plans on change).

## 6. Risks
- Inclusive-tier weighting touches the hot sampling path + advisor scorer →
  re-baseline `cargo bench --bench advisor_plan` (v1 budgets have huge
  margin).
- `League` in `ApplyContext` is a wide signature change across all `apply()`
  call sites — mechanical; covered by compiler + existing tests.
- Schema v3 ⇒ hard-reset user state (v3 convention) — first-launch
  wipe+notify UX.
- Min-Mod-Level constant discrepancy (§1.2) must be resolved against data,
  not assumed.
- Uncertain-mechanic tests use wide tolerances to avoid brittleness; exact
  probabilities stay `Confidence::Experimental`.
