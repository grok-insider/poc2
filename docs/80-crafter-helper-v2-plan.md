# Crafter Helper v2 — Detailed Implementation Plan

**Status:** Plan locked, ready to build.
**Author:** OpenCode session, drafted on user instruction so a different AI
agent can continue work without context loss.
**Scope:** Phase A through Phase G. Implementation order is A → E → F → B → C → D → G,
not the alphabetical order, because the planner can't reason about chains
it doesn't have data for.

---

## 0. Why this plan exists

The user is iterating on the desktop crafter helper. After several
rounds of UI work and per-base art, two structural problems surfaced:

1. **Invalid recommendations.** The advisor was suggesting Exalted Orb
   on a Normal-rarity item. Exalts are only legal on Rare items. The
   engine's `apply()` rejects this at runtime, but the advisor's
   candidate generator never filters it before scoring. Recommendations
   with zero probability of working still reach the user.

2. **Outcome dialog shows too few mods.** When the user clicks
   "I just used this", the dialog shows ~5 mods instead of the full
   pool that poe2db / Craft of Exile show for the base. Ground truth
   for `Vile Robe ilvl 82` is roughly:
   - 7 prefix groups, ~59 prefix tiers
   - 11 suffix groups, ~85 suffix tiers
   - 11 desecrated mods
   - 4 essence-only prefixes + 2 essence-only suffixes
   - 9 Vaal corruption implicits

   The user confirmed the screenshot was from the **browser preview**
   (mock IPC), so the immediate cause is the hand-curated mock list.
   But we also must verify the live Tauri bundle doesn't have the same
   gap, and extend the pipeline if it does.

The user also articulated the philosophy the engine should follow:

> "The engine needs a lot of training to get the best, most efficient
> and deterministic ways to craft items."

The engine isn't just a probability calculator. It needs to encode
expert crafting knowledge. The user gave a concrete example for ES body
armour:

1. Apply Transmutation, then Augmentation.
2. If we already have 2 T1 ES prefixes, do **not** Regal — Regal might
   roll a third prefix that displaces the path to a third ES roll.
   Instead, use an **Essence on the suffix** to lock in a decent
   suffix mod.
3. **Desecrate the prefix slot with an omen** to bias the reveal.
4. Use **Divine Orb** to push the T1 ES values to max.
5. Use **Fracturing Orb** to lock the max-rolled T1 ES.
6. Use **Reveal-desecrate with Omen of Echoes** for double-chance on
   a great prefix tier.

This pattern shows the advisor must reason about:
- Concept-occupancy on each affix slot (what's already there).
- Whether an action *protects* or *risks* the keepers.
- Tier-fix opportunities (Divine + Fracture when keeper is max).
- Omen variants on bones, with their own pool effects and prices.

Risk-slider semantics: low risk = deterministic essence chains;
high risk = Regal/Exalt/Chaos for upside per orb. Cost-aware
ranking uses live essence and omen prices from poe2scout.

Tie-breaking: lower variance wins. But higher-variance/cheaper
options must remain visible because Omen of Light alone is 80–120
divines and many users can't afford the deterministic path.

---

## 1. Constraints carried over

- **Desktop fixed-height UI.** No page-level scrolling. Internal
  scrolling within panels only. Three target viewports: 960×600,
  1280×800, 1920×1080.
- **No commits of downloaded data.** `apps/desktop/public/base-icons/`
  is `.gitignore`d. The `cargo run -p poc2-pipeline --bin fetch-base-icons`
  one-shot script populates it locally.
- **Released gear bases only** in the Item Database picker.
- **Class IDs in PascalCase** for all engine and frontend code
  (e.g., `BodyArmour`, not `Body Armour`). Bundle's display strings
  get normalized at the IPC boundary by `pascal_class()`.
- **Browser preview must work** without the Tauri shell. The
  `apps/desktop/src/lib/tauri.ts` shim returns mock data when
  `__TAURI_INTERNALS__` is not on `window`.
- **Mocks stay realistic.** The mock IPC must mirror real backend
  shapes and volumes; otherwise UI development is misleading.
- **No silent failures.** Tests are mandatory per phase. The user
  said: "this is an app that should not lead to fails."

---

## 2. Data model invariants

`Item.base` stores the **PascalCase class ID** (e.g. `"BodyArmour"`).
The engine's mod registry indexes by `(ItemClassId, AffixType)`.

`Item.base_type_id` (frontend-only) holds the full bundle
`BaseTypeId` (e.g. `"Metadata/Items/Armours/BodyArmours/FourBodyInt3"`).
This is used purely for art lookup against
`/base-icons/manifest.json` and for the displayed base name.
The engine ignores it when planning.

`Item.base_display_name` (frontend-only) is `"Hexer's Robe"` etc.
Used for top bar title and item preview.

`ModRoll.values` is a `SmallVec<[f64; 4]>` parallel to the parent
`ModDefinition.stats[]`. The outcome dialog's per-stat sliders must
emit one value per stat, not a single shared midpoint, when computing
the result.

`Goal` carries `target.prefixes`, `target.suffixes`, `abandon_criteria`,
`budget`. Concepts in the goal use the same ConceptId taxonomy as the
mod registry (e.g. `EnergyShield`, `FireResistance`, `AllResistances`).

Currency types:
- Family roots: `OrbOfTransmutation`, `OrbOfAugmentation`, `RegalOrb`,
  `ExaltedOrb`, `ChaosOrb`, `OrbOfAnnulment`, `DivineOrb`, `VaalOrb`,
  `HinekorasLock`, `FracturingOrb`, `OrbOfAlchemy`, `Recombinator`.
- Tiers: `Greater*`, `Perfect*` for the Trans/Aug/Regal/Exalt/Chaos
  family. Each tier inherits the family's rarity gate but raises the
  `min_required_level` floor (Perfect = 70).
- Essences: `LesserEssenceOf*`, `EssenceOf*`, `GreaterEssenceOf*`,
  `PerfectEssenceOf*`, `CorruptedEssenceOf*`. Quality determines what
  rarity transitions the essence performs.
- Catalysts: jewellery-only quality currency, out of scope for body
  armour example, but recognised by the engine.
- Bones: `{Size}{Subtype}` (e.g. `AncientCranium`, `PreservedRib`).
  Combined with omens for desecrate flows.
- Omens: cosmetic to the engine when stored on the item, but they
  *gate* what bone reveals can produce. Omen-aware desecrate is
  central to high-end crafting.

---

## 3. Phase ordering rationale

Implementation order: **A → E → F → B → C → D → G**.

- **A first** — Rarity gating is small, mostly mechanical, and unblocks
  every other phase by removing illegal candidates. Without A, every
  phase below has to special-case "but is this even legal".
- **E second** — Pipeline must register desecrated, essence-only, and
  Vaal mods in the registry before B (smart chain selection) can
  reason about them. If the registry doesn't have desecrated mods,
  the planner can't suggest "desecrate prefix with omen" intelligently.
- **F third** — Cost-aware ranking needs essence and omen prices.
  Without F, B's chain comparison can't weigh "Essence vs Regal" or
  "Omen of Echoes vs unconditioned reveal".
- **B fourth** — Smart chain selection consumes the data and prices
  the prior phases produce.
- **C fifth** — Outcome dialog enriches the user's interactive surface
  using everything from A–B.
- **D sixth** — UI signaling polish on top of C.
- **G last** — Verification gates after each phase, plus a final
  cross-cut.

Tests for each phase land *inside* that phase, not at the end. The
user's no-fails directive demands tests-first per phase.

---

## 4. Phase A — Rarity gating + chain hygiene

### A.1 `Currency::valid_rarities()`

New trait method on `poc2_engine::currency::Currency`. Returns a
`RaritySet` (a small newtype around `bitflags!` over `Rarity` variants).

Per-currency mapping:

| Currency | Valid rarities |
|---|---|
| `OrbOfTransmutation` (any tier) | `Normal` |
| `OrbOfAugmentation` (any tier) | `Magic` |
| `RegalOrb` (any tier) | `Magic` |
| `OrbOfAlchemy` | `Normal` (but excluded from advisor candidates) |
| `ExaltedOrb` (any tier) | `Rare` |
| `ChaosOrb` (any tier) | `Rare` |
| `OrbOfAnnulment` | `Magic`, `Rare` |
| `DivineOrb` | `Magic`, `Rare`, `Unique` |
| `VaalOrb` | `Normal`, `Magic`, `Rare`, `Unique` |
| `HinekorasLock` | `Normal`, `Magic`, `Rare` |
| `FracturingOrb` | `Rare` (and item must have ≥ 4 mods) |
| `Recombinator` | `Rare` (both inputs) |
| Essence (Lesser) | `Normal` (creates Magic) |
| Essence (Greater/Perfect) | `Magic` (locks one mod, fills others) |
| Essence (Corrupted) | `Rare` |
| Bones | `Magic`, `Rare` (gate further by bone subtype + class) |

### A.2 `Currency::can_apply_to(item) -> Result<(), CannotApply>`

Composes:
- `valid_rarities().contains(item.rarity)` — rarity gate.
- Slot capacity: enough open prefix/suffix slots for the action.
- Fractured count: Fracture requires ≥ 4 visible mods, none yet
  fractured, and the chosen mod isn't a hidden desecrated.
- Hinekora's Lock: not already locked.
- Vaal: not already corrupted (else needs `DoubleCorrupted` flag).
- Recombinator: same `BaseTypeId`, same ilvl.
- Bones: omen-conditioned class membership.

`CannotApply` enum carries a structured reason so the UI can show
specific messages ("Exalted Orb requires Rare", "no open prefix slot",
"item already corrupted", etc).

### A.3 Drop `OrbOfAlchemy` from advisor candidates

`crates/advisor/src/candidate.rs::generate_candidates()` filters out
`OrbOfAlchemy` unconditionally. The user's directive: Alchemy is
deterministic-randomness anti-pattern; the engine must never recommend
it. The orb is still resolvable by `record_outcome` if the user
applies it themselves.

### A.4 Strategy library audit

In `crates/strategies/strategies/`, search every TOML for
`OrbOfAlchemy` and replace with the controlled chain:
- Step 1: `OrbOfTransmutation` (or `PerfectOrbOfTransmutation` per
  budget).
- Step 2: `OrbOfAugmentation` (or Greater/Perfect per budget).
- Step 3: `RegalOrb` *or* `GreaterEssenceOf*` (engine selects per B).
- Step 4: optional Annul-to-one + Chaos spam loop.

Each step's `recovery` block carries hints for the failure path.

### A.5 Tests

`crates/engine/tests/rarity_gating.rs` (new):
- Iterate every currency. For each, assert `valid_rarities()` matches
  the table above.
- Assert `can_apply_to` returns the right `CannotApply` reason for
  each invalid combination.

`crates/advisor/tests/no_illegal_currencies.rs` (new):
- Normal item, depth-3 plan: assert no recommendation references
  Exalt/Chaos/Aug/Regal/Annul.
- Magic item, depth-3 plan: no Trans/Exalt/Chaos.
- Rare item, depth-3 plan: no Trans/Aug/Regal.
- All depths, all rarities: no Alchemy.

---

## 5. Phase E — Pipeline data extension

The advisor can only reason about mods that are in `bundle.mods`. The
user's example uses desecrated mods (Amanamu, Kurgal, Ulaman),
essence-only mods (Bears the Mark of the Abyssal Lord), and Vaal
implicits. We must verify these are in the registry before B can
plan around them.

### E.1 Audit current bundle

Read-only inspection of `~/.config/poc2/bundles/poc2.bundle.json.gz`:
- Total `bundle.mods.len()` (today: 2123 per startup log).
- Count by `kind`: `Explicit`, `Implicit`, `Desecrated`, `Enchantment`,
  `Corrupted`.
- Per-class explicit count for `BodyArmour`, `Helmet`, `Boots`,
  `Ring`, `Amulet`, `Belt`.
- Verify essence-target mods land in `bundle.mods` with `kind =
  Explicit` and `flags.ESSENCE_ONLY` set, *not* embedded only in
  `bundle.essences.entries`.
- Verify desecrated entries land as `kind = Desecrated`.
- Verify Vaal corruption implicits land as `kind = Corrupted`.

If any class is missing entries that poe2db / CoE list, plan E.2.

### E.2 Pipeline normalize extensions

In `pipeline/src/normalize/repoe_to_bundle.rs`:
- For each essence in `bundle.essences.entries`, ensure its target
  mod is registered as `ModDefinition` with `flags.ESSENCE_ONLY = true`
  and `kind = Explicit`. Today the essence catalogue may carry the
  mod as JSON without registry promotion.

In `pipeline/src/normalize/poe2db_to_bundle.rs`:
- Scrape `https://poe2db.tw/us/Desecrated_Modifiers` for the full
  table of desecrated mods per class. Each row becomes a
  `ModDefinition` with `kind = Desecrated`, `flags.DESECRATED_ONLY`,
  appropriate `tags` (lord-bound: Amanamu/Kurgal/Ulaman), and an
  `affix_type` derived from the table.
- Scrape Vaal corruption implicit tables and register as
  `ModDefinition` with `kind = Corrupted`, `flags.CORRUPTED_ONLY`.

If schema changes (e.g., new `ModFlags` bits), bump
`BUNDLE_SCHEMA_VERSION` in `crates/data/src/lib.rs`.

### E.3 Tests in `crates/data/tests/`

`crates/data/tests/registry_coverage.rs` (new):
- For each gear class: `bundle.mods_by_kind(class, Desecrated).count()
  >= expected_min` (where the minimum comes from poe2db ground truth).
- For BodyArmour: expect ≥ 11 desecrated mods.
- For BodyArmour: expect ≥ 6 essence-only target mods.
- For BodyArmour: expect ≥ 9 Vaal implicits.
- Symmetric tests for Helmet, Boots, Gloves, Ring, Amulet, Belt where
  the ground truth is available.

### E.4 Schema migration

If the schema changes:
- Bump `BUNDLE_SCHEMA_VERSION`.
- Update `Bundle::validate()` to reject older schemas with a clear
  message ("re-run pipeline build").
- Document the migration in `CHANGELOG.md`.

---

## 6. Phase F — Cost-aware planning data

### F.1 Valuator extension

In `crates/market/src/valuator.rs`:
- Add `omen_prices: HashMap<OmenId, f64>` and `essence_prices:
  HashMap<CurrencyId, f64>` fields.
- Extend `apply_feed_to_valuator` to accept omen and essence price
  feeds.
- Cache fetched prices on disk under
  `~/.config/poc2/cache/prices/<league>.json` with TTL (default 1 h).
  When network fails, fall back to last cached values; when no cache,
  fall back to engine-defined placeholder prices.

### F.2 poe2scout integration

In `crates/market/src/poe2scout.rs`:
- Add `fetch_omen_snapshot(league)` and `fetch_essence_snapshot(league)`
  endpoints. poe2scout exposes these under
  `https://poe2scout.com/api/items/omen?league=<league>` and
  `.../essence?league=<league>` (verify exact endpoints during build;
  the existing currency endpoint is the template).
- Batch fetch on price refresh; UI's existing "Refresh prices"
  button triggers all three (currency, omens, essences) in one call.

### F.3 Advisor consumption

In `crates/advisor/src/scorer.rs`:
- When scoring an `apply_currency` candidate that uses an essence,
  read the essence's price from the valuator and add to expected cost.
- When scoring a `bone reveal` candidate, add the bone's price plus
  any omen used.
- Variance-aware ranking:
  - Compute `score = expected_progress / expected_cost`.
  - Compute `variance = stderr * cost_stderr` from Monte Carlo sample.
  - Sort by `score` descending; break ties by `variance` ascending.
- Always retain at least one alternative with higher variance + lower
  cost in the recommendation set so the user sees the cheaper path.

### F.4 Tests

`crates/market/tests/price_cache.rs` (new):
- Mock poe2scout responses for currency/omen/essence.
- Assert prices land in the valuator after a refresh.
- Assert cache hit on second call within TTL.
- Assert cache fallback when network fails.

---

## 7. Phase B — Smart chain selection

### B.1 Concept-occupancy heuristics

In `crates/advisor/src/scorer.rs`:
- Compute `current_target_satisfaction(item, goal)` = per-affix-slot
  count of mods already satisfying the target.
- When a candidate would *risk* an existing target-satisfying slot
  (e.g., Regal on a Magic item with 2 T1 ES prefixes might roll a
  third prefix that crowds out the third ES), reduce its score.
- When a candidate *protects* existing keepers (essence on the empty
  affix, desecrate on the empty prefix), boost its score.

### B.2 Tier-fix opportunities

When the item carries a target-concept mod at tier 1 (or whatever
`min_tier` the goal specifies):
- If the mod's value is below max range, boost `DivineOrb` score.
- If the mod's value is at max range, auto-suggest `FracturingOrb`
  with rationale "T1 keeper at max — fracture before the next risky
  step".

### B.3 Risk-slider semantics

`PlanInput.risk` ∈ [0, 1]:
- < 0.3: filter out high-variance candidates entirely. Prefer
  deterministic chains (essence, desecrate, divine, fracture).
- 0.3–0.7: blend by `score * (1 - risk * variance_penalty)`.
- > 0.7: prefer raw expected progress regardless of variance.

### B.4 Recurring-step compression

In `crates/advisor/src/recommendation.rs`:
- Add a `Recurring` variant to `AdvisorAction` carrying:
  - inner sequence (e.g., `[Annul, Chaos]`)
  - stop predicate (`StopPredicate`)
  - expected iterations (mean + stderr)
  - expected total cost (sum of inner costs × expected iterations)
- In `crates/advisor/src/planner.rs`, add a post-pass that detects
  loop patterns:
  - Annul-to-1 + Chaos: `[Annul, Chaos]` repeated until target
    concept at tier ≥ N.
  - Chaos spam: `[Chaos]` repeated until target concept at tier ≥ N.
  - Greater Essence chain: `[Augment, Regal]` repeated until two
    target prefixes.
- When the item already satisfies the prerequisite for the inner
  loop start (e.g., already has 1 mod), the loop's first Annul step
  is skipped automatically.

### B.5 Stop predicates

In `crates/advisor/src/recommendation.rs`:
- New type:
  ```rust
  pub struct StopPredicate {
      /// All concepts in this list must be present at tier ≥ min_tier.
      pub concepts: Vec<ConceptCriterion>,
      /// Total mod count must be ≤ max_mods.
      pub max_mods: Option<u8>,
  }

  pub struct ConceptCriterion {
      pub concept: ConceptId,
      pub min_tier: u8,
      pub affix: Option<AffixType>,
  }
  ```
- The frontend displays this in the Recurring card as a friendly
  list ("Stop when: T1 ES on prefix and T1 Cold Res on suffix").

### B.6 Omen-aware desecrate recommendations

For each bone in candidate generation, emit one recommendation per
bone with the omen as a sub-control on the action:
- `AdvisorAction::Reveal { bone, omen, prefer, use_abyssal_echoes,
  min_acceptable, abandon_if_no_match }`
- The OutcomeDialog sub-control lets the user pick which omen they
  applied (Sinistral Necromancy, Dextral Necromancy, Blackblooded,
  Liege, Sovereign, Echoes of the Abyss, etc.).
- The cost in the recommendation is `bone_price + omen_price`.

### B.7 ES body armour example as test fixture

`crates/advisor/tests/es_body_armour_chain.rs` (new):
- Fixture: Vile Robe ilvl 82, target = 3× T1 ES prefix + 2× T1 Resist
  suffix, budget = 100 div, risk = 0.2 (low).
- Assert plan emits the user's example chain in order:
  1. Trans / Perfect Trans (depending on budget).
  2. Aug / Greater Aug.
  3. Greater Essence on suffix (because 2 ES already locked).
  4. Desecrate prefix with omen (e.g., Echoes for double-chance).
  5. Divine Orb (to push T1 ES to max).
  6. Fracture T1 ES.
  7. Reveal-desecrate with Omen of Echoes for second T1.

### B.8 Annul + Chaos spam test

`crates/advisor/tests/annul_chaos_loop.rs` (new):
- Fixture: Magic item with 1 unwanted prefix.
- Assert plan emits a single `Recurring` recommendation with inner
  `[Chaos]` (Annul skipped because already at 1 mod), stop predicate
  matching the goal, expected iterations matching Monte Carlo mean.

---

## 8. Phase C — Outcome dialog rebuild

### C.1 Backend `eligible_mods` already returns full pool

Already verified in `apps/desktop/src-tauri/src/lib.rs::eligible_mods`.
Returns every registry mod for the class+affix, with eligibility
metadata. No backend change needed for the dialog to show everything.

### C.2 Frontend dialog

`apps/desktop/src/lib/OutcomeDialog.svelte` rebuild:
- Always show every mod in the response, never filter to "target
  only" by default.
- Currency-rollable highlighted gold.
- Greyed reasons:
  - Below `min_required_level` (currency floor).
  - Above ilvl.
  - Group already occupied.
  - Essence-only when action is non-essence.
  - Desecrated-only when action is non-bone-reveal.
  - Corrupted-only when action is non-Vaal.
- Each row shows: tier badge T{i}/T{n}, name, affix chip, ilvl chip,
  weight + weight share bar, concept chips, kind chips.

### C.3 Filter chips

Above the search:
- **Affix scope**: All / Prefix / Suffix.
- **Roll source**: All / Currency / Essence / Desecrated / Vaal.
- **Tier filter**: All / T1 / T1–T2 / T1–T3.
- **Pool view**: All / Target / Non-target.

Default selections depend on action kind:
- `apply_currency`: Roll source = Currency.
- `apply_essence`: Roll source = Essence.
- `bone reveal`: Roll source = Desecrated.

### C.4 Action header

Render:
- **Item state**: rarity, ilvl, prefix occupancy (used/cap), suffix
  occupancy (used/cap), fractured count.
- **Action precondition**: required rarity, slot, currency floor.
- **Cannot-apply banner**: red row when `can_apply_to(item)` errors,
  with the specific reason. Apply button disabled.

### C.5 Bone reveal sub-control

When action is bone reveal:
- Dropdown lists allowed omens for the bone (filtered by class).
- Selecting an omen updates `min_required_level`, `prefer`,
  `use_abyssal_echoes` flags in the action.
- Mod list re-fetches with the new constraints.

### C.6 Browser preview parity

`apps/desktop/src/lib/tauri.ts::mockEligibleMods` expansion:
- For BodyArmour: encode the full Vile Robe ilvl 82 pool ground truth:
  - 7 prefix groups, ~59 prefix tiers (Physical Thorns, Increased ES,
    Hybrid ES + ES, ES + Life, +Max ES, +Max Life, +Spirit).
  - 11 suffix groups, ~85 suffix tiers (Life Regen, ES Recharge,
    Reduced Attribute Req, Bleed/Ignite/Poison Duration, +Int,
    +Stun Threshold, +Chaos Res, +Cold Res, +Fire Res,
    +Lightning Res).
  - 11 desecrated mods.
  - 4 essence-only prefixes + 2 essence-only suffixes.
  - 9 Vaal implicits.
- Same shape for Helmet and OneHandSword.

---

## 9. Phase D — UI signaling polish

### D.1 Step cards

In `apps/desktop/src/lib/AdvisorPanel.svelte`:
- "Cannot apply" badge on steps whose `can_apply_to` errors, click
  disabled.
- Recurring step card shows iteration count, stop predicate (rendered
  from `StopPredicate` via a small helper), and a "Show inner
  sequence" expander.

### D.2 Right column Eligible tab

Same Roll-source filter chips as the OutcomeDialog so users can
compare Currency vs Essence vs Desecrated vs Vaal pools without
opening the dialog.

### D.3 Step card highlight when item changes

When `record_outcome` fires, animate the affected mod row in the
Item Preview to draw attention to the change.

---

## 10. Phase G — Verification

### G.1 Static checks (gate every phase)

- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `pnpm check`
- `pnpm build`

### G.2 Targeted unit tests (additions per phase)

Per Phase A:
- `crates/engine/tests/rarity_gating.rs`
- `crates/advisor/tests/no_illegal_currencies.rs`

Per Phase E:
- `crates/data/tests/registry_coverage.rs`

Per Phase F:
- `crates/market/tests/price_cache.rs`

Per Phase B:
- `crates/advisor/tests/es_body_armour_chain.rs`
- `crates/advisor/tests/annul_chaos_loop.rs`
- `crates/advisor/tests/risk_slider_semantics.rs`

Per Phase C:
- `apps/desktop` Svelte component tests where feasible (svelte-check
  enforces type safety; visual regressions caught by browser preview
  screenshots).

### G.3 Integration checks

- **Live Tauri spot check**: pick Vile Robe via BasePicker, click
  "I just used this" on a Perfect Transmute step, screenshot the
  dialog. Confirm ≥ 50 prefix candidates and ≥ 70 suffix candidates
  appear (matching the bundle's actual coverage post-Phase E).
- **Browser preview at 960×600, 1280×800, 1920×1080**: confirm
  filter chips behave, action header shows preconditions, recurring
  step card renders correctly.

### G.4 Failure handling

User said "this app should not lead to fails." Concretely:
- Every IPC has `?` error propagation; no unwraps in production paths.
- The frontend handles `null`/`undefined` IPC returns and shows
  styled error banners, not blank screens.
- Pipeline scripts are best-effort: when a poe2db page is missing, it
  lands in `manifest.missing[]` and `exit 0` is preserved.
- Tests assert error banners surface for every documented failure
  mode.

---

## 11. Open implementation guidance

### 11.1 Avoid drift between strategies and engine semantics

The `crates/strategies/strategies/*.toml` library encodes user-visible
crafting workflows. Engine constraints (rarity, slot capacity, fracture
count) must match. Add a CI-time test that loads every strategy and
asserts each step's currency is legal for the rarity the strategy
assumes at that step. If we change a currency's rarity gate, the
strategy library breaks loudly.

### 11.2 Recurring step cost CI

The recurring step's expected cost is `mean_iterations *
inner_cost_per_iteration`. Compute via Monte Carlo (already done by
the engine simulator). Surface a `prob_stderr`-style confidence
interval so the user can judge "this loop runs 8±3 times" instead of
just "this loop runs 8 times".

### 11.3 Omen-pool expansion

Bones with omens read from a small static table:
- `OmenOfSinistralNecromancy` → forces prefix on next bone apply.
- `OmenOfDextralNecromancy` → forces suffix.
- `OmenOfBlackblooded` → enables Amanamu lord pool on weapon/jewellery.
- `OmenOfTheLiege` → enables Kurgal lord pool.
- `OmenOfTheSovereign` → enables Ulaman lord pool.
- `OmenOfEchoesOfTheAbyss` → reveal yields *two* mods at next reveal.
- `OmenOfWhittling` → next Annul targets lowest `required_level`.
- `OmenOfSinistralErasure` → next Annul targets prefix.
- `OmenOfDextralErasure` → next Annul targets suffix.
- `OmenOfLight` → next Annul targets desecrated mod (Greater costs
  80–120 divines per the user; cheap variant exists at Lesser).

These map to existing engine omens. The advisor's bone candidate
generator iterates all (bone, omen) pairs that are *legal* for the
class + state and emits one recommendation per pair, ranked by
`expected_progress / (bone_price + omen_price)`.

### 11.4 The user's no-fails rule

If a phase's tests don't pass, that phase is incomplete. Don't move
on. The user's exact words: "this is an app that should not lead to
fails." When in doubt:
- Add a defensive test.
- Surface the error to the user in the UI.
- Never crash, never freeze, never recommend something the engine
  can't apply.

---

## 12. Phase implementation order summary

```
A  — Rarity gating + chain hygiene  (engine + advisor + tests)
E  — Pipeline data extension        (poe2db scrape + bundle schema + tests)
F  — Cost-aware planning data       (valuator + poe2scout + cache + tests)
B  — Smart chain selection          (planner + scorer + recurring step + tests)
C  — Outcome dialog rebuild         (frontend + mock parity)
D  — UI signaling polish            (frontend)
G  — Verification cross-cut         (static + integration + screenshots)
```

Each phase ends with verification before the next begins.

---

## 13. Files expected to change

Engine:
- `crates/engine/src/currency/mod.rs` — add `valid_rarities`,
  `can_apply_to` methods.
- `crates/engine/src/currency/<each currency>.rs` — implement above.
- `crates/engine/tests/rarity_gating.rs` — new.

Advisor:
- `crates/advisor/src/candidate.rs` — filter by `can_apply_to`,
  drop Alchemy, emit per-omen bone recommendations.
- `crates/advisor/src/scorer.rs` — concept-occupancy heuristic,
  tier-fix bonus, variance-aware tie-break.
- `crates/advisor/src/planner.rs` — recurring-step compression,
  loop collapse for already-at-1-mod cases.
- `crates/advisor/src/recommendation.rs` — `Recurring` variant,
  `StopPredicate`.
- `crates/advisor/tests/*.rs` — new test files per phase.

Strategies:
- `crates/strategies/strategies/*.toml` — Alchemy audit + replace.
- New strategy: `es-body-armour-deterministic.toml` covering the
  user's example chain.

Pipeline:
- `pipeline/src/normalize/repoe_to_bundle.rs` — register essence
  target mods with `flags.ESSENCE_ONLY`.
- `pipeline/src/normalize/poe2db_to_bundle.rs` — desecrated and Vaal
  mod scrape and registration.
- `pipeline/src/sources/poe2db.rs` — extend scrape.

Data:
- `crates/data/src/lib.rs` — bump `BUNDLE_SCHEMA_VERSION` if needed.
- `crates/data/tests/registry_coverage.rs` — new.

Market:
- `crates/market/src/valuator.rs` — omen + essence prices, cache.
- `crates/market/src/poe2scout.rs` — omen + essence endpoints.
- `crates/market/tests/price_cache.rs` — new.

Tauri layer:
- `apps/desktop/src-tauri/src/lib.rs` — IPC: surface
  `cannot_apply` reasons, expose recurring step shape, omen
  sub-control list.

Frontend:
- `apps/desktop/src/lib/OutcomeDialog.svelte` — full pool, filter
  chips, action header, omen sub-control.
- `apps/desktop/src/lib/AdvisorPanel.svelte` — cannot-apply badge,
  recurring step card.
- `apps/desktop/src/lib/EligiblePanel.svelte` — Roll-source filter.
- `apps/desktop/src/lib/tauri.ts` — expand `mockEligibleMods` to the
  full Vile Robe pool plus Helmet and OneHandSword.
- `apps/desktop/src/lib/types.ts` — `StopPredicate`, `Recurring`,
  `CannotApply`.

Tests fixtures:
- `apps/desktop/src/lib/fixtures.ts` — extend with the Vile Robe ES
  fixture.

---

## 14. State at handoff

When this plan was written, the following were already in place:
- Phase 0–13 of the prior crafter helper iteration is complete:
  fixed-height layout, base picker with image manifest, eligible_mods
  / record_outcome IPC, outcome dialog basic, target builder, smart
  guidance v1, history and Eligible tabs, cheapest/safest tiles, etc.
- The fetch-base-icons script has been run partially (~352 of ~1500
  bases) and is still progressing in the background. It must be
  allowed to finish or rerun.
- `apps/desktop/public/base-icons/` is populated with downloaded webp
  assets and the `manifest.json`. This directory is gitignored.
- Browser preview is working at all three target viewports without
  page-level scroll.
- All workspace tests pass (per Phase 13 verification).

When picking up this plan, start by reading sections 0, 1, 2, 3 to
ground in the constraints, then execute Phase A.

---

## 15. Glossary

- **Affix slot**: prefix or suffix occupied by exactly one rolled mod.
- **Brick**: a craft outcome that wastes the item, usually because
  Annul/Chaos rolled the wrong direction.
- **Concept**: stat-id grouping (EnergyShield, FireResistance, etc.).
- **Concept-occupancy**: how many target-satisfying mods sit on the
  current item per affix.
- **Controlled chain**: deterministic crafting steps (Trans → Aug →
  Regal/Essence → Annul → Chaos), preferred over Alchemy.
- **Currency floor**: `min_required_level` enforced by Greater
  (≥ 35) and Perfect (≥ 70) currency tiers.
- **Keeper**: a mod the user wants to preserve.
- **Recurring step**: a single recommendation that internally repeats
  an inner sequence until a stop predicate is satisfied.
- **Rolling/spamming**: applying a currency repeatedly hoping for a
  desired roll.
- **Stop predicate**: condition under which a recurring step exits.
- **Tier (T1, T2, ...)**: mod-group ladder index, T1 = highest
  required-level / strongest version.
- **Target concept**: any concept in `Goal.target` that the user
  wants on the finished item.
