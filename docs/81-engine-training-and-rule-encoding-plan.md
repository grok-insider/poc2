# Engine Training and Rule Encoding — Detailed Implementation Plan (v3)

**Status:** Plan locked, ready to build.
**Author:** OpenCode session, drafted as a handoff document so a different AI agent (or a different developer) can resume work without losing context.
**Scope:** Layers 1–3 (Data substrate, Rule encoding, Engine training) shipped together as v3. Not split.
**Reference back:** `docs/72-v1-execution-plan.md` (v1 baseline), `docs/80-crafter-helper-v2-plan.md` (v2 plan, completed). This v3 plan continues from where v2 left off.

---

## 0. Why this plan exists, and why this shape

### 0.1 The user's intent in plain words

The user articulated the goal as:

> "make sure our rules for crafting, the rules that dictates if you can use an resource item on the gear item or not and the modifiers outcomes of each base item (taking in consideration the ilevel too and other factors) so we can start training the advisor engine with those rules and constrictions so it can develop the best ways to craft, also make a plan on how to train our engine"

Three distinct problems are bundled into one ask:

1. **Constraint correctness.** "If you can use an resource item on the gear item or not" — meaning the engine's `can_apply_to` must reflect every Path of Exile 2 rule about which currency is legal on which item (rarity, item-class, slot-capacity, mirrored, corrupted, hidden-desecrated, fracture-eligibility, omen-class restrictions, bone-subtype-class restrictions, catalyst-class restrictions). These are *hard mechanics*, not heuristics.

2. **Outcome correctness.** "The modifiers outcomes of each base item (taking in consideration the ilevel too and other factors)" — meaning the engine's `sample_eligible_mod` must produce the same probability distribution as the in-game roll mechanic. This requires per-(mod, base, ilvl) numerical weights, the `*_ONLY` flag enforcement on the sampling path, and the tag-based weight model.

3. **Engine training.** "So it can develop the best ways to craft" — meaning the advisor's recommendations must be *optimal*, not just *correct*. Optimal here means "minimum expected steps" or "minimum expected divine-equivalent cost" depending on the user's risk slider. Reaching optimality on stochastic chains with cycles requires training, not just heuristics.

### 0.2 Why these three problems must be solved together

Layers 1 (constraints + outcomes) and 3 (training) cannot be separated:

- Training a policy on **wrong outcome distributions** produces a policy that's optimal for the wrong game. If `total_weight_for_item` returns 0/1 instead of CoE-derived numerical weights, the trained policy will grossly mis-estimate the expected number of Chaos spams needed to hit T1 — which is exactly the kind of decision the user wants to be informed about.
- Training a policy on **incomplete constraints** produces a policy that recommends illegal moves. If `Bone::can_apply_to` doesn't gate by item class, the policy will recommend a Cranium bone on a Body Armour, the engine's `apply()` will reject it at runtime, and the recommendation is dead on arrival.
- Conversely, fixing the constraints and outcomes without training leaves the advisor at v2 capability: beam-search depth-3 with hand-coded heuristics. That's good enough for short chains (3xT1 ES body armour: ~10 steps) but inadequate for long-tail spam loops (Chaos-to-T1 on a tri-resist suffix: ~1000 steps). Britz's blog post and the wiki Recombinator math confirm: optimal policies on PoE crafting *require* lookahead far beyond beam-search depth.

Layer 2 (rule encoding as data) sits between them: it's where the constraints get authored, where expert chains live as imitation-learning seeds, and where new rules surfaced by the audit get committed to the rule catalogue. Without Layer 2 the rules are scattered; with it, they're a single auditable source of truth.

### 0.3 Why this exact shape, not other shapes

We considered and rejected several alternative shapes:

- **"Just train an RL agent end-to-end."** Rejected because the action space is huge (~30 currencies × ~21 omens × bone-omen pairs × bench actions), the reward is sparse, and we have no GPU budget. Britz tried this in his blog and found Q-learning took 10+ minutes to converge with hyperparameter tuning, vs seconds for the model-based approach. We adopt his model-based approach for the same reasons.
- **"Just write more rules."** Rejected because rules can't represent geometric expectation. A rule says "if state X, do Y"; it can't say "do Y; expected 1000 iterations; expected cost 3.2 div ± 0.4 div". The user explicitly wants iteration-count + cost surfaced — that's Q-value territory.
- **"Just fix the data and keep beam search."** Rejected because beam search is depth-bounded. The user's worked ES body armour chain works at depth 10 but the Chaos-spam case the user explicitly asked about is unsolvable at any practical beam depth. We need a closed-form Q-value, which means training.
- **"Use MCTS."** Rejected because the state graph is cyclic and stochastic. Britz §"Why game tree search doesn't work" explains in detail; AlphaGo-style MCTS requires a DAG. The PoE crafting graph is dense and highly cyclic.
- **"Ship a neural network policy."** Rejected for v3 because the table-based Q-function suffices for our state-space size (~10⁴ reachable states per goal). Revisit in v4 if we need cross-goal generalization.

The chosen shape — **Layer 1 fixes truth, Layer 2 captures expert knowledge as data, Layer 3 trains the optimal policy** — mirrors the standard imitation-learning + RL stack documented in the Stanford CS237B Imitation Learning notes (Behavior Cloning + DAgger as warm start for RL) and validated by Britz on the same problem domain.

### 0.4 What's already done that this plan builds on

The v1 + v2 plans (already shipped, see `docs/72-v1-execution-plan.md` and `docs/80-crafter-helper-v2-plan.md`) gave us:

- A working beam-search planner with concept-occupancy heuristics, tier-fix candidates, omen-aware bone reveals, recurring-step compression with `LoopEstimate`, risk-slider variance weighting (Phase B.1–B.8).
- The `Currency` trait with `valid_rarities()` and `can_apply_to()` skeleton (Phase A) — currencies report their rarity gates structurally; the advisor filters illegal candidates.
- 113 seed rules across 14 sections (Phase A.5) — most heuristics from `docs/34-heuristics-rulebook.md` are encoded.
- 24 strategies (`crates/strategies/strategies/*.toml`) — including the user's worked ES body armour chain.
- Pipeline ingestion of RePoE-fork (mods + bases + tags) + Craft of Exile (essences + catalysts + numerical weights → `bundle.weights`) + poe2db (omens + bones) + curated fixtures (desecrated + Vaal corruption).
- IPC-routed structured `CannotApply` reasons surfaced in the OutcomeDialog and AdvisorPanel.
- Tauri-aware bundle loader, Hyprland always-on-top integration, plugin SDK, etc.

### 0.5 What the audit found is broken (the six gaps that motivate Layer 1)

Per the comprehensive audit performed during planning (results reproduced below for handoff continuity):

| # | Gap | Where | Severity |
|---|---|---|---|
| 1 | `total_weight_for_item` returns 0/1 only — sampling is uniform-eligible, not numerically weighted | `crates/engine/src/currency/basic.rs:113-120` | **blocker for training** |
| 2 | `bundle.weights` (CoE numerical weights) is built but never read at runtime | `apps/desktop/src-tauri/src/lib.rs:305` logs and discards | **blocker for accurate EV** |
| 3 | `BaseRegistry` not wired — `class_for_item` uses `item.base.as_str()` as class id, breaking bone↔class, catalyst↔class, and tag-based weighting | `currency/basic.rs:29-39`, `bone.rs:14-19`, `catalyst.rs:16-18` | **blocker for class gating** |
| 4 | `ESSENCE_ONLY` / `DESECRATED_ONLY` / `CORRUPTED_ONLY` flags exist on mods but `sample_eligible_mod` doesn't read them | `mods.rs:118-124` defined; `currency/basic.rs:55-109` doesn't filter | leaks essence mods into Trans/Aug/Regal/Exalt/Chaos pools |
| 5 | Vaal `BrickMods` strips fractured-only and adds nothing; Omen of Corruption never consumed | `currency/basic.rs:946-951`, `925-963` | wrong outcome distribution |
| 6 | Strategy library covers only BodyArmour + Ring/Amulet — Boots, Helmet, Gloves, Belt, all weapons, Quiver, Focus, Talisman, Jewel have no class-specific chains | `crates/strategies/strategies/` | training has nothing to imitate for those classes |

### 0.6 External research findings the plan rests on

The plan's design depends on several factual claims confirmed during research. Future agents must verify these still hold before deviating:

- **PoE2 weights are not in the game files.** Krakenbul / Prohibited Library reverse-engineered them via the Recombinator success-rate formula. poe2db and Craft of Exile import these tables. Source: `https://www.poe2wiki.net/wiki/Recombinator` (the success-chance formula in §3 is what enables back-solving), `https://poe2db.tw/us/weightings`, `https://www.craftofexile.com/weightings?game=poe2`.
- **Tag-based weighting is the canonical model.** Each mod has a list of `(tag, weight)` pairs. The effective weight on a base is `weight_of_first_matching_tag(mod, base.tags)`. RePoE-fork ships these directly in `mods.json`'s `spawn_weights` field. Source: `https://www.poe2wiki.net/wiki/Special:CargoTables/mod_spawn_weights`, `https://github.com/repoe-fork/repoe`.
- **The full omen catalogue is ~44 omens** with deterministic categorical effects (force-prefix, force-suffix, two-mods, max-mods, lord-pool-unlock, double-reveal, no-NoChange, lowest-mod-level, etc.). Source: `https://www.poe2wiki.net/wiki/Category:Omens`, `https://game8.co/games/Path-of-Exile-2/archives/491748`.
- **Desecrated mod restrictions per item-class are documented.** Sceptres have no exclusive desecrated. BodyArmour/Helmet/Gloves/Boots have no exclusive desecrated prefixes. Jewels use `Lightless` / `of the Abyss` (not Lich-named); Liege/Sovereign/Blackblooded omens are illegal on jewels. Source: `https://www.poe2wiki.net/wiki/Desecrated_modifier`.
- **Bone subtype → item-class table:** Rib → armour, Jawbone → weapon/quiver, Collarbone → jewellery (ring/amulet/belt), Cranium (Preserved only) → jewel, Vertebrae (Preserved only) → waystone. Source: `https://www.mulefactory.com/wiki_path_of_exile_ii_poe_2_abyssal_bones_explained/`, `https://www.u4n.com/news/list-of-poe-2-desecrates-currency-ancient-bones.html`.
- **Direct comparable: Denny Britz's blog `https://dennybritz.com/posts/poe-crafting/`** (PoE1, 2024). Solved the same MDP we face. Used a featurized state, learned a transition model `P(s'|s,a)` by Monte Carlo sampling, then ran Q-value iteration. Reported: 100k samples per state-action pair sufficient; afterstate aliasing critical for collapsing Essence/Exalt/etc.; q-learning converged in seconds vs 10+ min for model-free RL.
- **Other comparables that informed but didn't drive the design:** poe2htc (Java, beam search, multithreaded, web app at poe2htc.com), pyoe2-craftpath (Python+Rust, statistical analyzer over currency sequences), POETheoryCraft (C#, mass crafting with filters). All chose beam search + heuristics; none of them implemented training. Our v3 advances on the SOTA in the open-source PoE crafting tool ecosystem by adopting Britz's training approach for PoE2.

### 0.7 User answers to scoping questions (locked-in decisions)

Asked during planning, answered by the user:

1. **Q:** Ship as one v3 release, or split v3.0 / v3.1? **A:** Ship all three layers as a single v3 release. No split.
2. **Q:** Training corpus size — 25 goals (one per per-class strategy) or 50+? **A:** Larger corpus. The corpus is enumerated in §11 below.
3. **Q:** Trade-listing scraper — skip, opt-in once-per-patch, or 30-min refresh? **A:** 30-min refresh in v3 as a starting point. Long-term we'll move to a server-side cache so end users don't hit GGG's API directly. v3 ships the 30-min path; v3.x will add the server-cache.
4. **Q:** State-migration UX when `BaseRegistry` lands — heuristic auto-fill, prompt-confirm, or hard-reset? **A:** Hard-reset. User state under `~/.config/poc2/` gets wiped on first launch with the v3 schema; the user re-imports their item.
5. **Q:** Bundle schema bump? **A:** Bump from v1 to v2.
6. **Q:** Reward function for training — path-length, cost, or "first-attempt right roll"? **A:** Whichever produces the best results regardless of training time. The plan uses both path-length and cost (Britz showed they often differ); v3 ships both reward functions and the user picks via the risk slider.
7. **Q:** `WeightObservation` confidence — should we add an upstream-credit field? **A:** All `WeightObservation` entries get `confidence: Community`. We don't add per-source credit fields in v3. (If we add a server-side scrape later we'll revisit.)

These answers are non-negotiable inputs; the plan reflects them throughout.

### 0.8 The user's edge-case question and its answer

The user asked:

> "in the training, will it have cases where the rolls/chances arent good so it need to keep trying? for example doing spam of chaos orb on 1 mod item to get a tier 1? will this edge cases be in the training?"

**Yes — this is exactly what the training is best at**, and it's the case where the trained policy beats beam-search by the largest margin. Three reasons:

1. **The Q-table converges to the true expected number of steps to goal.** A `Chaos→Chaos→Chaos→...` loop on a Magic-with-1-mod is just a sequence of state transitions where many transitions go back to the same featurized state. Q-value iteration handles cycles natively (Britz §"Why game tree search doesn't work" — this is precisely why MCTS fails and Q-learning succeeds). The Q-value at the loop entry equals the geometric expectation: `E[steps] = 1/p_success` where `p_success` is the per-iteration probability of hitting T1. Beam search with depth 3 cannot see past 3 chaos rolls; the trained policy sees the full geometric tail.

2. **Phase B.4's `Recurring` action variant (already shipping in the engine) is what represents these loops in the recommendation surface.** Training discovers when a `Recurring [Chaos]` is optimal vs when a single `Chaos` followed by a different action wins. Tier 3.5's imitation seeding pre-loads loop knowledge from existing strategies (`fracture-then-chaos-spam.toml`, `annul-augment-spam.toml`, etc.).

3. **The reward function disambiguates loop preference.** Cost-reward training (Britz's second example, the Magic Jewel) demonstrates the algorithm correctly chooses 1350 cheap Alteration spams over 650 expensive Annul+Aug because the first totals fewer divines. Same logic applies to "1000 Chaos Orbs to T1" vs "Greater Essence + Regal lock-in" — the policy picks whichever has lower expected total cost given current valuator prices.

Specifically:

- The training corpus (Tier 3.6) **explicitly includes** spam-loop archetypes: Chaos-spam, Annul+Chaos-spam, Greater-Aug-spam, Bone-spam-then-Reveal, Recombinator-grind, Vaal-spam-on-corruptable-uniques.
- The reachable-state BFS in Tier 3.2 visits the spam-loop attractor states naturally because they're reachable from the goal's initial state.
- The `samples_per_state_action: 100k` budget gives enough resolution to estimate `p_success ≈ 0.001` accurately (stderr ≈ 0.0001), so the policy correctly values "this loop runs ~1000 times costing ~3 div" not "this loop never reaches goal".
- **Edge-case test in Tier 3.7 metrics**: a held-out test goal where the only viable path is Chaos-spam to T1; assert the trained policy emits `Recurring [ChaosOrb]` and the loop estimate falls within the 95% CI of the true geometric distribution.

This answers the deeper question: the entire reason we're moving from "beam search + heuristics" to "trained policy" is that beam search literally cannot see past depth-N, and the most lucrative crafts are the ones with long-tail spam loops where depth-N is insufficient. Training is what makes the advisor competent on those.

§7 of this plan enumerates the specific edge-case scenarios the training addresses.

---

## 1. Constraints and assumptions carried over from v1 and v2

These remain in force; v3 does not relax any of them:

- **Desktop fixed-height UI.** No page-level scrolling. Three viewports: 960×600, 1280×800, 1920×1080.
- **NixOS + Hyprland only.** Cross-platform support is post-v1 work.
- **No commits of downloaded data.** Bundle artifacts, base icons, trade snapshots all stay out of the repo.
- **PascalCase ItemClassId.** Item classes are PascalCase strings (`BodyArmour`, not `Body Armour`).
- **Browser preview must work without Tauri.** The `tauri.ts` shim's mock IPC must mirror real backend shapes.
- **No silent failures.** Tests are mandatory per phase. Per the user: "this is an app that should not lead to fails."
- **Plugins (Wasm SDK) ship with v1.** Don't break the plugin ABI without ADR.

New constraints introduced by v3:

- **Hard-reset user state on bundle schema bump from v1 to v2.** First-launch detection: if `state.toml` was written under bundle schema v1 and the loader sees v2, wipe `~/.config/poc2/state.toml` and `~/.config/poc2/recipes/`. Notify the user via a one-time dialog. Cache (`~/.config/poc2/cache/`) is preserved.
- **Trained models are bundle-versioned.** A trained model carries the `BUNDLE_SCHEMA_VERSION` it was trained against; loaders refuse mismatched versions and trigger retraining.
- **Trade scraper rate-limit floor: 30 minutes.** Never poll faster than once per 30 min; respect any `Retry-After` header from GGG. Fail soft on errors.

---

## 2. Data model invariants (what changes vs v2)

### 2.1 Existing invariants (still hold)

- `Item.base` stores PascalCase class id, e.g. `"BodyArmour"`. Engine indexes `(ItemClassId, AffixType)` by this.
- `Item.base_type_id` (frontend-only in v2) holds full bundle `BaseTypeId` (e.g. `"Metadata/Items/Armours/BodyArmours/FourBodyInt3"`).
- `ModRoll.values` is `SmallVec<[f64; 4]>` parallel to `ModDefinition.stats[]`.
- `Goal` carries `target.prefixes`, `target.suffixes`, `abandon_criteria`, `budget`. Concepts use the curated taxonomy.
- Currency types (PerfectOrbOfTransmutation, etc.) and tier conventions (Greater = req-level ≥35/55/50, Perfect = ≥70) are unchanged.

### 2.2 New invariants introduced in v3

- **`Item.base_type_id` is engine-canonical, no longer frontend-only.** It becomes `Option<BaseTypeId>` on the engine `Item` struct. The migration path is hard-reset (per user answer 4).
- **`BUNDLE_SCHEMA_VERSION = 2`.** v2 bundles carry `Bundle.weights` populated from CoE *and* used at runtime by the registry. v1 bundles are rejected by the loader; the user is prompted to rebuild via `cargo run -p poc2-pipeline -- build`.
- **`ModRegistry::weight_for(mod_id, base, ilvl)`** is the canonical weight-lookup API. The order of resolution is fixed:
  1. `(mod_id, base_type_id)` exact match in `bundle.weights` (CoE numerical weight).
  2. `(mod_id, item_class)` aggregate match (for non-recombinable bases).
  3. `mod.spawn_weights` tag-intersection fallback (RePoE-fork eligibility × tag).
  4. Zero (mod is not eligible).
- **`*_ONLY` flag masks are part of the sampling contract.** Each currency declares the flag mask it filters. Trans/Aug/Regal/Exalt/Chaos use `ESSENCE_ONLY | DESECRATED_ONLY | CORRUPTED_ONLY`; Essences use `DESECRATED_ONLY | CORRUPTED_ONLY`; Bone reveals use `ESSENCE_ONLY | CORRUPTED_ONLY`; Vaal corruption uses `ESSENCE_ONLY | DESECRATED_ONLY`.
- **`TrainedModelCache`** is a new state component: maps `(goal_hash, item_class) → SerializedQTable`. Lives at `~/.config/poc2/cache/trained_models/`. Trained models ship with the bundle for the canonical training corpus (§11) and are loaded lazily.
- **Goal hashing is stable.** Two goals with the same `target.prefixes`/`target.suffixes`/`abandon_criteria`/`budget` (modulo serialization order) hash identically. This is what enables trained-model lookup. Implementation: canonical-form-then-blake3.

---

## 3. Phase ordering rationale

The plan executes in three layers, in this order:

```
Layer 1 (Data substrate) — six tiers, M14.1 → M14.7
        ↓
Layer 2 (Rule encoding)  — four tiers, M15.1 → M15.4
        ↓
Layer 3 (Engine training) — seven tiers, M16.1 → M16.7
```

Within each layer, tiers are largely sequential because each unblocks the next:

- **M14.1 weights → M14.2 BaseRegistry**: weight resolution requires base_type_id lookup, which requires the BaseRegistry.
- **M14.2 BaseRegistry → M14.3 ONLY-flags**: ONLY-flag enforcement at sampling time benefits from class-aware filtering.
- **M14.5/6 gating → M14.7 weight pipeline**: the trade-listing scraper produces `WeightObservation` entries scoped to base; the BaseRegistry must exist first.
- **M15.1 strategies require M14 to be done**: writing a glove strategy that says "Greater Essence on suffix slot" implicitly relies on the engine correctly enforcing essence rarity gates and ONLY-flags.
- **M15.4 cross-source CI runs after M14 + M15.1-3**: the CI test asserts every strategy's currency steps are legal under the engine's current rules.
- **M16.1 featurize → M16.2 model learner → M16.3 value iteration → M16.4 hybrid planner**: standard MDP toolchain build order.
- **M16.5 imitation seeding requires M15.1**: imitation seeds come from the strategy library's `dry_run` traces.

This ordering matches Britz's discipline ("first the simulator must be correct, then learn the model, then solve the MDP") and matches the v2 plan's pattern (data-first, rules-second, planner-third).

---

## 4. Layer 1 — Data substrate (the engine consults truth, not placeholders)

### 4.1 Tier 1.1 — Wire `bundle.weights` into the registry (M14.1)

**Goal:** numeric per-(mod, base, ilvl) weights drive `sample_eligible_mod`, replacing the 0/1 placeholder currently at `crates/engine/src/currency/basic.rs:113-120`.

**Why first:** without numerical weights, the engine cannot produce correct outcome distributions. Training on top of incorrect distributions wastes compute and produces a wrong policy. This tier is the single highest-leverage fix.

**Steps:**

1. Extend `ModRegistry::from_mods` (currently `crates/engine/src/registry.rs:45-70`) to take `(Vec<ModDefinition>, Vec<WeightObservation>)`. Build an additional index `weights_by_mod_base: AHashMap<(ModId, BaseTypeId), f64>` and `weights_by_mod_class: AHashMap<(ModId, ItemClassId), f64>`. Both come from the `WeightObservation` slice; `WeightScope::Base` populates the first, `WeightScope::ItemClass` populates the second.
2. Add `ModRegistry::weight_for(&self, mod_id: &ModId, base: &BaseTypeId, ilvl: u32, item_class: &ItemClassId) -> f64` with the resolution order documented in §2.2.
3. Replace `total_weight_for_item` body (basic.rs:113-120). The new body calls `registry.weight_for(...)` with the item's base/class/ilvl.
4. Update `apps/desktop/src-tauri/src/lib.rs:312` to pass `bundle.weights` along with `bundle.mods` into `ModRegistry::from_mods`.
5. Migrate any test fixtures that currently rely on uniform sampling. Use `ScoringWeights` defaults to compensate where existing tests' expected scores would shift.

**Tests:**

- `crates/engine/tests/weighted_sampling.rs`: assert that for a synthetic registry with two mods (one weight 1000, one weight 100), 10 000 samples produce ~91% / ~9% distribution within stderr.
- Existing `worked_example_es_body_armour` continues to pass after fixture migration; the test's seed-based determinism still holds.

**Verification:** `cargo test --workspace` green; advisor's `prob_stderr` numbers narrow noticeably when multiple mods compete for a slot (regression-detectable).

### 4.2 Tier 1.2 — Build `BaseRegistry` (M14.2)

**Goal:** unblock class-aware gating across bone subtypes, catalysts, catalysts-on-belt, and per-base tag-keyed weighting.

**Why second:** `class_for_item` (basic.rs:29-39) currently uses `item.base.as_str()` as the class id. This works in test fixtures (where the test author sets `base = "BodyArmour"`) but fails on real bundle bases (where `base = "Metadata/Items/Armours/BodyArmours/FourBodyInt3"`). M14.1's weight resolution depends on knowing the canonical base id; that requires the BaseRegistry.

**Steps:**

1. New module `crates/engine/src/base_registry.rs` with:
   ```rust
   pub struct BaseRegistry {
       by_id: AHashMap<BaseTypeId, BaseType>,
       by_class: AHashMap<ItemClassId, Vec<BaseTypeId>>,
   }
   impl BaseRegistry {
       pub fn from_bases(bases: Vec<BaseType>) -> Self;
       pub fn get(&self, id: &BaseTypeId) -> Option<&BaseType>;
       pub fn class_of(&self, id: &BaseTypeId) -> Option<&ItemClassId>;
       pub fn tags_of(&self, id: &BaseTypeId) -> Option<&[TagId]>;
       pub fn for_class(&self, class: &ItemClassId) -> &[BaseTypeId];
       pub fn iter(&self) -> impl Iterator<Item = &BaseType>;
   }
   ```
2. Lift `Item.base_type_id` from frontend-only to engine-canonical. Add `Option<BaseTypeId>` to `Item` struct in `crates/engine/src/item.rs`. Default `None` for legacy items; v3 hard-reset removes legacy state.
3. Replace `class_for_item` (basic.rs:29-39) with a `BaseRegistry` lookup. Sites that previously took `Item` now take `(Item, &BaseRegistry)`.
4. Update `ApplyContext` (currency.rs:154-177) to carry `&'a BaseRegistry`. Threading: every `apply()` call site updates.
5. Migrate the desktop bundle-state to construct + share a `BaseRegistry` alongside `ModRegistry`.
6. Migrate test fixtures that set `base = "BodyArmour"` to use real `BaseTypeId`s. The bundle has them; tests can use `BaseRegistry::from_bases(vec![…])` with synthetic `BaseType`s.
7. The frontend's `Item.base_type_id` field becomes the source of truth; `Item.base` becomes a derived display field (the class name) that the IPC re-derives at the boundary.

**Tests:**

- `crates/engine/tests/base_registry_gating.rs`: a Gnawed Cranium fails on a BodyArmour; a Jawbone fails on an Amulet; a Carapace catalyst fails on a non-jewellery class; weight resolution returns the per-base weight when present.
- Migrated `worked_example_es_body_armour` continues green.
- New `crates/data/tests/base_registry_consistency.rs`: every `BaseType.item_class` referenced exists in `bundle.item_classes`; every `BaseTypeId` in `bundle.mods_by_base` keys exists in `BaseRegistry`.

### 4.3 Tier 1.3 — Enforce `*_ONLY` flags in sampling (M14.3)

**Goal:** essence-only mods never roll from Trans/Aug/Regal/Exalt/Chaos; desecrated-only mods never roll from currencies; corrupted-only mods never roll outside Vaal.

**Why third:** without this, the trained policy will incorrectly believe Trans can roll an essence-only mod (if its spawn-weight is positive), producing wrong recommendations.

**Steps:**

1. Extend `sample_eligible_mod` (basic.rs:55-109) signature with `excludes: ModFlags`. The function filters out any mod whose `flags` intersects `excludes`.
2. Per-currency exclude masks:
   - Trans/Aug/Regal/Exalt/Chaos (all tiers): `ESSENCE_ONLY | DESECRATED_ONLY | CORRUPTED_ONLY`.
   - Essence apply: `DESECRATED_ONLY | CORRUPTED_ONLY`.
   - Bone reveal: `ESSENCE_ONLY | CORRUPTED_ONLY`.
   - Vaal corruption AddEnchantment / BrickMods replace: `ESSENCE_ONLY | DESECRATED_ONLY`.
3. The masks are constants in each currency's module so the contract is local.

**Tests:**

- `crates/engine/tests/only_flag_enforcement.rs`: synthetic registry containing one ESSENCE_ONLY mod and one normal mod; transmute + augment + regal + exalt + chaos never produce the ESSENCE_ONLY mod across 1000 trials; essence does produce it.
- Symmetric tests for DESECRATED_ONLY and CORRUPTED_ONLY.

### 4.4 Tier 1.4 — Fix Vaal outcome model + Recombinator formula (M14.4)

**Goal:** Vaal's BrickMods replaces with a corruption-pool mod (not a no-op); Omen of Corruption is consumed and shifts the distribution per the wiki. Recombinator success-chance computation uses the wiki's exact formula instead of uniform sampling.

**Why fourth:** Vaal endpoints appear in 11 of 24 strategies; training will visit Vaal-decision states often, and the placeholder distribution skews policy toward over-recommending Vaal. Recombinator is one of the corpus archetypes (§7.4) — its uniform-sampling placeholder produces wrong policy.

**Steps for Vaal:**

1. Replace `BrickMods` body (basic.rs:946-951) with: clear non-fractured mods, then for each cleared slot sample one `ModKind::Corrupted` mod from the per-class corrupted pool (the Vaal implicit fixture data — `pipeline/data/vaal_implicits.json`).
2. Add `OmenSet::consume_corruption(patch) -> bool`. When true, Vaal's outcome sampler removes the `NoChange` branch and renormalizes the remaining 5 outcomes uniformly.
3. Author the post-corruption outcome distribution from wiki source: NoChange ~25%, RerollValues ~20%, BrickMods ~15%, AddEnchantment ~20%, AddSocket ~10%, AddQuality ~10%. With Omen of Corruption: NoChange = 0, others renormalized to ~26.7% / 20% / 26.7% / 13.3% / 13.3%.
4. Update the relevant Vaal-finish strategy step in `crates/strategies/strategies/vaal-corruption-finish.toml` to consume the Corruption omen properly.

**Steps for Recombinator:**

1. Replace `recombine()` body in `crates/engine/src/currency/recombinator.rs` with the wiki formula:
   ```
   SuccessChance = a × c × Π_i (Σ_{j=m_i}^{m_t0} weight_j) / weight_Z
   ```
   Where `a` is the base-type coefficient (Armour=10, Weapon=16, Quiver/Focus/Belt=12, Ring/Amulet=16), `c` is the mod-count coefficient from the wiki table, `weight_Z` is the total weight of mods of the same affix type that can roll on the selected base, and `weight_j` aggregates over the selected mod's tier ladder up to `m_t0`.
2. Add `RecombinatorOutcome::Success { output_item } | Failure` and surface this through `ApplyOutcome`.
3. Register `Recombinator` in `DefaultCurrencyResolver` so the advisor can recommend it.

**Tests:**

- `crates/engine/tests/vaal_outcome_distribution.rs`: 10 000-trial categorical chi-squared test against the documented distribution.
- `vaal_with_corruption_omen_never_returns_no_change`.
- `vaal_brick_replaces_with_corrupted_mod`.
- `crates/engine/tests/recombinator_success_formula.rs`: synthetic two-mod recombination matches the wiki's example computation within 0.5% relative error.

### 4.5 Tier 1.5 — Catalyst class gating + Belt rule (M14.5)

**Goal:** `Catalyst::can_apply_to` rejects non-jewellery / non-belt classes; Adaptive catalyst on belts gated to specific tags per 0.4 patch rules.

**Why fifth:** trivial fix that prevents the trained policy from ever recommending Catalysts on body armour. Two-line `can_apply_to` extension.

**Steps:**

1. `Catalyst::can_apply_to` (catalyst.rs) returns `CannotApply::Other("class X does not accept this catalyst")` for non-Ring/Amulet/Belt/Jewel.
2. Belts in 0.4: only accept the breach-tagged catalyst per heuristics rulebook §99.
3. New seed rule R093 `belt-only-breach-catalyst`, confidence Verified.
4. Class membership lookup uses BaseRegistry (M14.2).

**Tests:**

- `catalyst_rejects_body_armour`.
- `catalyst_unstable_rejects_belt_in_0_4`.

### 4.6 Tier 1.6 — Bone-subtype class gating + lord pool restrictions (M14.6)

**Goal:** Bone-subtype-to-class restrictions enforced at `can_apply_to` time. Lord-targeting omens (Blackblooded/Liege/Sovereign) only legal on classes whose desecrated pools include those lords.

**Why sixth:** these restrictions are documented mechanics. Without them the trained policy may recommend a Cranium on a body armour (it will silently no-op or be caught at apply-time, but the policy wastes its budget). Cleaner to gate.

**Steps:**

1. `BoneSubtype::valid_classes() -> &'static [&'static str]` table:
   - `Rib` → `["BodyArmour", "Helmet", "Boots", "Gloves"]`
   - `Jawbone` → `["OneHandSword", "TwoHandSword", "OneHandAxe", "TwoHandAxe", "OneHandMace", "TwoHandMace", "Bow", "Crossbow", "Spear", "Staff", "Sceptre", "Wand", "Quiver"]`
   - `Collarbone` → `["Ring", "Amulet", "Belt", "Talisman"]`
   - `Cranium` → `["Jewel"]` (Preserved tier only — game data confirms no Gnawed/Ancient Cranium)
   - `Vertebrae` → `["Waystone"]` (Preserved tier only)
2. `Bone::can_apply_to(item, base_registry)` checks the table.
3. Lord-pool restriction: jewels reject Liege/Sovereign/Blackblooded omens because their pool uses `Lightless`/`of the Abyss` mods, not Lich-named.
4. Sceptres reject lord-targeting omens because the wiki documents sceptres have no exclusive desecrated.

**Tests:**

- `crates/engine/tests/desecration_gating.rs`: every bone subtype × every gear class permutation; expected legal-or-not from the table above.
- Lord-targeting omen on jewel rejected.

### 4.7 Tier 1.7 — Bundle weight derivation pipeline (M14.7)

**Goal:** Two-source weight-derivation pipeline: CoE primary (recombinable bases) + trade-listing scraper secondary (non-recombinable: Charms, Jewels, Tablets, Waystones). 30-min refresh interval.

**Why seventh:** training quality depends on weight quality. CoE covers most cases but misses ~6 item categories. Filling the gap is a one-time effort.

**Steps:**

1. `pipeline/src/sources/trade.rs` — fetches `https://www.pathofexile.com/api/trade2/search/<league>/...` for each non-recombinable item class. Parses listings (the `result/[item-id]/listings/[listing-id]` endpoints). Counts mod occurrences per `(item-class, mod-id)` pair. Normalizes via the same script CoE uses (documented at `https://www.craftofexile.com/weightings`).
2. CLI flag: `--scrape-trade` and `--trade-refresh-secs 1800` (default 30 min). Cache lands in `~/.config/poc2/cache/trade-listings/<league>/<class>.json`.
3. Output: `WeightObservation` entries with `confidence: Community` (per user answer 7).
4. Operator workflow: `cargo run -p poc2-pipeline -- build --scrape-trade --out poc2.bundle.json.gz`. Bundle build skips the scrape if cache is fresh (< 30 min).
5. Post-v3, server-side cache will replace operator-driven scraping (per user answer 3) — out of v3 scope but the data shape stays the same so server-cache is a drop-in.
6. CoE alias suggester: new `poc2-pipeline coe-aliases-suggest` subcommand walks unmatched CoE mods and proposes aliases via cosine similarity on text templates. Operator runs it post-patch and curates `pipeline/data/coe_aliases.toml`.
7. Bundle schema bump: `BUNDLE_SCHEMA_VERSION = 2`. `Bundle::validate()` rejects v1 bundles. Desktop loader detects v1-on-disk and prompts the user to rebuild (see §10).

**Tests:**

- `pipeline/tests/weight_coverage.rs`: every `(item-class, affix-type)` combination has ≥ 1 weight observation.
- `pipeline/tests/trade_scraper_parser.rs`: parse a synthetic trade listing fixture, assert mod-count map.
- `pipeline/tests/trade_cache_ttl.rs`: cache hit on second call within 30 min; refresh after.
- `crates/data/tests/bundle_schema_v2.rs`: v1-versioned bundle fails `validate()`.

---

## 5. Layer 2 — Rule encoding (constraints + expert knowledge as data)

### 5.1 Tier 2.1 — Per-class strategy library expansion (M15.1)

**Goal:** every gear class has at least one canonical strategy chain encoded as data. Provides imitation-learning seeds for Layer 3.

**Why first in Layer 2:** Layer 3's M16.5 imitation seeding requires strategies to imitate. Without them, training cold-starts from random rollouts, which is ~10× slower per Britz.

**Strategies to author** (each follows the existing TOML schema in `crates/strategies/strategies/3xt1-es-body-armour.toml`):

| File | Class | Target archetype |
|---|---|---|
| `helmet-life-accuracy-tri-res.toml` | Helmet | Life + AccuracyRating + Tri-res |
| `helmet-es-spirit-int-caster.toml` | Helmet | ES + Spirit + Int (caster) |
| `boots-ms-life-tri-res.toml` | Boots | 30% MS + Life + Tri-res |
| `boots-es-ms-energy-shield.toml` | Boots | ES + MS + ES Recharge (caster) |
| `gloves-attack-speed-tri-res.toml` | Gloves | AttackSpeed + Accuracy + Tri-res |
| `gloves-cast-speed-es.toml` | Gloves | CastSpeed + ES + Int |
| `belt-life-stun-tri-res.toml` | Belt | Life + StunThreshold + Tri-res |
| `belt-es-spirit-resists.toml` | Belt | ES + Spirit + Resists (caster) |
| `1h-attack-physical-tri.toml` | OneHandSword/Axe/Mace | %IncreasedPhys + AddedPhys + AttackSpeed + Accuracy |
| `2h-attack-physical-crit.toml` | TwoHandSword/Axe/Mace | %IncreasedPhys + AddedPhys + AttackSpeed + CritChance |
| `bow-projectile-attack.toml` | Bow | %IncreasedPhys + ProjectileSpeed + AttackSpeed + AddedPhys |
| `crossbow-projectile-physical.toml` | Crossbow | (similar to bow) |
| `spear-melee-physical.toml` | Spear | %IncreasedPhys + AttackSpeed + Accuracy + AddedPhys |
| `caster-weapon-spell-skill.toml` | Staff/Sceptre/Wand | %IncreasedSpellDamage + +SkillLevel + CastSpeed |
| `quiver-projectile-skill.toml` | Quiver | %IncreasedAttackDamage + ProjectileSpeed + +ProjectileSkillLevel |
| `focus-es-spirit.toml` | Focus | ES + Spirit + ManaRegen |
| `talisman-abyss.toml` | Talisman | Abyss-synergy desecrated chain |
| `jewel-cranium-desecrate.toml` | Jewel | Cranium-only desecrate path |
| `timelost-jewel-lightless.toml` | Time-lost Jewel | Lightless / of-the-Abyss |

Each strategy:

- Lists `item_classes` and `attribute_pools`.
- Defines `target.prefixes` and `target.suffixes` with concept ids.
- Encodes 8–12 steps using the existing DSL (S1-validate-base, S2-perfect-transmute, ..., S(N)-vaal-finish).
- Includes recovery branches for each step's `on_failure`.
- Specifies `expected_cost_div` and `expected_success_prob` ranges.
- Confidence: `community` for all (no source claims `verified`).

Charms / Tablets / Waystones are deferred to v3.x because their crafting flows differ structurally (per-difficulty bonus mechanics).

**Tests:**

- `crates/strategies/tests/strategy_coverage.rs`: every gear class except deferred charms/tablets/waystones has ≥ 1 strategy.
- Existing `load_all_strategies.rs` parses every new TOML.

### 5.2 Tier 2.2 — Mod-pool predicate primitives (M15.2)

**Goal:** the strategy DSL exposes the predicates strategy authors actually need.

**Why second:** authoring strategies in Tier 2.1 surfaces missing predicates. Adding them after-the-fact is more efficient.

**New predicates (in `crates/strategies/src/dsl.rs` and `crates/strategies/src/predicate.rs`):**

- `has_keeper_count { count: u32, min_tier: Option<u8> }` — count of mods on the item satisfying any `goal.target` spec.
- `has_open_slot { affix: AffixType }` — pure slot-availability check separate from `affix_count`.
- `keeper_at_max_roll { concept: ConceptId, threshold_pct: f64 }` — true when at least one keeper of the concept has all stat values at the top `threshold_pct` of range. Default 0.95.
- `mod_value_within { mod_id: ModId, stat_id: StatId, op: CmpOp, value: f64 }` — generic predicate for surgical "did the divine roll land high enough" checks.
- `concept_set_contains_any { concepts: Vec<ConceptId>, affix: Option<AffixType>, min_tier: Option<u8> }` — convenience for OR over concept-list (not just `concept_any` which is per-spec).
- `bundle_weight_above { mod_id: ModId, threshold: f64 }` — predicate exposing the registry weight at predicate-eval time. Useful for "stop if this mod has effectively-zero weight on the current base".

Each predicate ships with serde schema docs + ≥ 2 unit tests in `predicate.rs`.

### 5.3 Tier 2.3 — New rules sourced from external research (M15.3)

**Goal:** add ~25 rules that encode hard mechanics surfaced by the audit + research. Re-balance the rule catalogue so hard-mechanics rules (Verified) outweigh heuristic guidance (Community).

**Rules to add** (each in `crates/rules/seed_rules/<section>.toml`, confidence as noted):

| Rule ID | Section | Constraint |
|---|---|---|
| R022 | 01_abandonment | Hidden_desecrated cannot be Vaal-corrupted without losing the desecrated mod. **Verified.** |
| R142 | 10_vaal | Omen of Putrefaction with Ancient bone wastes ilvl gating; Preserved bone preferred. **Verified.** |
| R143 | 10_vaal | Twice-corrupt 50% destroy chance — never twice-corrupt without backup or HinekorasLock preview. **Verified.** |
| R215 | 04_exalt_vs_desecrate | Sceptres have no exclusive desecrated prefixes/suffixes — desecrate sceptre wastes a bone. **Verified.** |
| R216 | 04_exalt_vs_desecrate | Body/Helmet/Gloves/Boots have no exclusive desecrated prefixes — desecrate prefix on those classes only adds organic prefixes. **Verified.** |
| R217 | 04_exalt_vs_desecrate | Jewels desecrate with `Lightless`/`of the Abyss` mods (not Lich-named); Liege/Sovereign/Blackblooded omens illegal on jewels. **Verified.** |
| R218 | 09_base_selection | Recombinator success drops when ilvl jumps tier ladders — pick the lowest-tier ilvl base that hits the keeper tier. **Community.** |
| R219 | 04_exalt_vs_desecrate | Mark of the Abyss swap: only legal on items already carrying Mark; replaces with higher-tier desecrated. **Verified.** |
| R220 | 09_base_selection | Body armours with Hexer's/Vile/Cultist Robe — int_armour tag exclusively, str/dex pools blocked. **Verified.** |
| R221 | 09_base_selection | Armour-Evasion hybrid bases roll both pure-armour and pure-evasion mods + the str_dex hybrid pool. **Verified.** |
| R222 | 06_stop_vs_continue | Specific essence + perfect tier requires Magic input → Rare output without scour-equivalent: cannot apply to Rare. **Verified.** |
| R223 | 04_exalt_vs_desecrate | Greater Essence on Magic locks one mod and rolls Rare with 4 mods total: existing Magic mods preserved as-is. **Verified.** |
| R224 | 04_exalt_vs_desecrate | PerfectExalted only legal on Rare with ≤ 5 mods (slot for new mod required). **Verified.** |
| R225 | 03_hinekora_lock | Hinekora's Lock + Vaal: lock saves the pre-Vaal state; reverting wastes the lock seed. **Verified.** |
| R226 | 02_fracture | FracturingOrb cannot fracture a hidden desecrated; it can only land on visible mods. **Verified.** |
| R227 | 05_whittle_vs_annul | Annul on Magic with 1 mod returns to Normal. **Verified.** |
| R228 | 04_exalt_vs_desecrate | Item-class restricted bone subtype: desecrate refuses on wrong class. **Verified.** |
| R229 | 99_catalysts | Catalyst tag-mismatch on already-quality jewellery resets quality to 0. **Verified.** |
| R230 | 99_catalysts | Quality-bonus catalyst raises mod weight by 1% per quality on the catalyst's tag — stack to 20%. **Verified.** |
| R231 | 00_progression | Greater Aug min_mod_level=55, Greater Regal=50, Greater Exalt=50, Greater Chaos=50, Perfect=70. **Verified.** |
| R232 | 04_exalt_vs_desecrate | Trans/Aug/Regal/Exalt/Chaos cannot roll essence-only mods regardless of weight. **Verified.** |
| R233 | 10_vaal | Vaal corruption with Omen of Corruption removes the NoChange outcome. **Verified.** |
| R234 | 99_catalysts | Belt class only accepts unstable (caster) and flesh (life) catalyst types in 0.4. **Community.** |
| R235 | 04_exalt_vs_desecrate | Bone reveal pool: regular affix uses normal weights; exclusive desecrated uses uneven weights with at least 1 exclusive guaranteed at ilvl 65+. **Verified.** |
| R236 | 02_fracture | When item has < 4 mods, Fracture is illegal regardless of intent. **Verified.** (Formalizes existing R310.) |

These rules carry `confidence: Verified` because they're hard mechanics (vs the existing community-sourced heuristics).

### 5.4 Tier 2.4 — Cross-source rule validation pass (M15.4)

**Goal:** CI test that catches authoring drift between strategies, rules, and engine semantics.

**Steps:**

1. New `crates/strategies/tests/cross_source_validation.rs`:
   - For every strategy, for every step that's `apply_currency`: assert the engine's `currency.can_apply_to(synthesized_item_matching_preconditions)` returns Ok.
   - For every rule, for every `then` action that's `apply_currency`: same check.
   - Every concept referenced exists in the analyzer's curated taxonomy (`built_in_classifier`).
   - Every currency id resolves via `DefaultCurrencyResolver`.
   - Every bone id, omen id, essence id has a corresponding entry in the bundle's `bones` / `omens` / `essences` sections.

This is the "v2 plan §11.1" deliverable that was deferred; M15.4 lands it.

---

## 6. Layer 3 — Engine training (advisor learns optimal chains)

### 6.1 Tier 3.1 — Featurized state representation (M16.1)

**Goal:** map a full `Item` to a compact `FeatureVec` so the Q-table is tractable.

**Design (per Britz, adapted for our domain):**

```rust
pub struct FeatureVec {
    rarity: u8,                     // 0=Normal, 1=Magic, 2=Rare, 3=Unique
    target_match: u16,              // bit i = item carries a mod satisfying goal target spec i (cap 16)
    n_prefixes: u8,                 // 0..=3 (clamp at 3+)
    n_suffixes: u8,                 // 0..=3
    has_hidden_desecrated: bool,
    has_fractured: bool,
    is_corrupted: bool,
    has_hinekora_lock: bool,
    extra_flags: u8,                // reserved for future per-class signals
}
```

Total state-space: `4 × 65536 × 4 × 4 × 256 ≈ 67M` raw, but reachable subset is `≈ 10⁴` per goal because `n_prefixes + n_suffixes ≤ 6`, `target_match` is bounded by goal cardinality (typical ≤ 5), and rarity transitions are monotonic for most chains.

**Why this design:**

- Britz's `target_match` bitmap generalizes across crafts (the trained model works for any goal that matches its bitmap mapping).
- `has_hidden_desecrated`/`has_fractured` are critical for Bone reveal and Fracture decisions; per Britz "fractures are typically fixed at the start of the crafting process" but we want training to still distinguish them.
- `is_corrupted`/`has_hinekora_lock` capture the "terminal-state" signal for Vaal-finish branches.

**Module:** `crates/advisor/src/featurize.rs` with:

```rust
pub fn featurize(item: &Item, goal: &Goal, registry: &ModRegistry) -> FeatureVec;
pub fn target_match_bitmap(item: &Item, goal: &Goal, registry: &ModRegistry) -> u16;
```

**Tests:**

- `featurize_round_trip`: identical items + goals produce identical FeatureVecs.
- `target_match_bitmap_handles_hybrid_mods`: a hybrid ES+Life mod sets bits for both ES and Life specs.

### 6.2 Tier 3.2 — Per-action transition model learner (M16.2)

**Goal:** offline-learn `P(s' | s, a)` as a categorical distribution table by Monte Carlo sampling.

**Module:** `crates/advisor/src/training/model_learner.rs`.

```rust
pub fn learn_transition_model(
    task: &CraftingTask,             // (initial_item, goal, registry, base_registry, resolver, valuator)
    samples_per_state_action: u32,   // 100_000 per Britz
    afterstate_aliasing: bool,       // true
    seed: u64,
) -> TableModel<FeatureVec, AdvisorAction>
```

**Algorithm (per Britz Algorithm 1):**

1. Initialize `done_states: HashSet<FeatureVec>`, `queue: Vec<Item> = [task.initial_item]`.
2. While queue is non-empty: pop an item.
3. If goal-satisfied or abandon-criteria-fired: continue.
4. Compute `features = featurize(item)`. Mark done.
5. For each valid action: skip if afterstate-aliased (Essences are afterstates because their distribution is item-state-independent given target_match bit-flip pattern).
6. For each non-aliased `(features, action)`: run `simulate(item, action)` `samples_per_state_action` times, accumulate next-state counts, push unseen next-items onto queue.
7. Normalize counts to probabilities.

**Afterstate aliasing detail:** Britz's optimization. Essences add a known mod and randomize others; the next-state distribution is fully determined by `(action, current_target_match)`. This collapses many `(s, a)` pairs into one. Same applies to Exalted Orbs (next-state is item-state-modulo-which-slot-was-empty, captured by feature vec). Concrete aliasing rules:

```rust
pub enum StateActionAlias<S> {
    Pair((S, AdvisorAction)),
    AfterEssence(EssenceId, S),      // S here is just target_match + n_prefixes + n_suffixes
    AfterExalt(S),
    AfterRegal(S),
    AfterTransmute(S),
    // ... etc.
}
```

**Why this matters for the user's edge case:** Chaos-spam on a 1-mod Rare item produces a distribution over next-states where many transitions return to the same featurized state (the spam loop). The learner observes this naturally during BFS and the resulting transition matrix has a strong self-loop probability. Q-value iteration in M16.3 then closes the loop and computes the geometric expectation correctly.

**Performance budget:** 100k samples × ~5k state-action pairs per goal × ~50 goals in training corpus = 2.5 × 10¹⁰ simulator calls. At ~1 µs per call (current bench), that's ~7 hours per full corpus training. User said "we don't care about training time" — accept this.

**Output format:** `TableModel { transitions: HashMap<(FeatureVec, AdvisorAction), HashMap<FeatureVec, f64>> }`. Serialize via `bincode` for compactness; the bundle ships pre-trained models (per user answer 5).

**Tests:**

- `crates/advisor/tests/training/model_learner_smoke.rs`: train a tiny model (1k samples) on a 2-state synthetic task; assert categorical distribution matches expected within stderr.
- `afterstate_aliasing_collapses_essence_states`: with aliasing on, the model has fewer entries than with aliasing off.

### 6.3 Tier 3.3 — Q-value iteration solver (M16.3)

**Goal:** standard Bellman iteration over the learned transition model. Two reward functions ship.

**Module:** `crates/advisor/src/training/value_iteration.rs`.

```rust
pub fn value_iteration<S: StateRepr>(
    model: &TableModel<S, AdvisorAction>,
    reward_fn: impl Fn(&S, &AdvisorAction) -> f64,
    gamma: f64,        // 1.0 (no discount)
    theta: f64,        // 1e-6 convergence threshold
    max_iters: u32,    // 1000 hard cap
) -> (ValueFn<S>, StateActionValueFn<S>)
```

**Reward functions shipped:**

1. **Path-length reward:** `R(s, a) = -1` per non-terminal step, `R(s_goal, _) = 0`. Minimizes expected number of steps to goal.
2. **Cost reward:** `R(s, a) = -valuator.expected_cost(a)`, `R(s_goal, _) = 0`. Minimizes expected divine-equivalent cost.

The user's risk slider maps to a blend: low risk = cost reward, high risk = path-length reward, mid = linear blend between the two Q-values at advisor query time. Implementation:

```rust
let q_blended = (1.0 - risk_blend) * q_cost + risk_blend * q_steps;
```

Both Q-tables ship per-goal in the trained model file.

**Performance:** with ~10⁴ states × ~30 actions, each iteration is ~3 × 10⁵ updates. Convergence typically ≤ 50 iterations for episodic MDPs at γ=1. Total: ~1 second per goal per reward function. Trivial vs the model-learning step.

**Tests:**

- `crates/advisor/tests/training/value_iteration_converges.rs`: synthetic 5-state chain; assert Q-values match analytic expectation.
- `value_iteration_handles_self_loops`: chaos-spam attractor state converges to `-1/p_success`.

### 6.4 Tier 3.4 — Hybrid planner (trained model + beam search) (M16.4)

**Goal:** the existing beam-search planner stays as the always-available baseline; the trained policy dominates well-trodden patterns.

**Why hybrid:** trained policy has compounding-error risk on out-of-distribution states (Britz §"Sim-to-Real Gap"). Beam search is robust on novel states. Best of both worlds.

**Steps:**

1. Extend `PlanInput` with `pub trained_models: Option<&'a TrainedModelCache>`.
2. `TrainedModelCache::lookup(&self, goal_hash: u64, item_class: &ItemClassId) -> Option<&TrainedModel>`.
3. `plan` flow:
   - Compute `goal_hash = canonical_blake3(goal)`.
   - Lookup trained model. If present:
     - For each candidate at depth 1, query the Q-table for its expected value.
     - Use Q-value as the primary score; concept-occupancy adjustment + cost band become tiebreakers.
   - If not present, run v2 beam search unchanged.
4. Sim-to-real-gap detector: at depth 3, compare the trained model's expected accumulated reward vs the MC depth-3 rollout. If divergence > 50% on absolute value, downgrade trained-policy weight to 0 for this state (fallback to beam search). Logged for offline analysis.

**Tests:**

- `crates/advisor/tests/hybrid_planner_uses_trained_model.rs`: with a pre-baked trivial trained model, assert the planner picks the model's optimal action.
- `hybrid_planner_falls_back_when_no_trained_model`: ensures beam-search-only path is regression-safe.
- `hybrid_planner_detects_sim_to_real_gap`: synthetic divergent state forces fallback.

### 6.5 Tier 3.5 — Imitation seeding from strategy library (M16.5)

**Goal:** warm-start the model learner with expert trajectories from the strategy library so training converges faster on canonical chains.

**Why:** Britz § "Compounding error problem" — cold-start from random rollouts wastes compute on states the user will never realistically reach. Strategies *are* expert demonstrations; pre-loading them hits Britz's "behavior cloning + data aggregation" pattern.

**Steps:**

1. New `crates/advisor/src/training/imitation.rs`:
   ```rust
   pub fn seed_from_strategies(
       model: &mut TableModelBuilder<FeatureVec, AdvisorAction>,
       strategies: &StrategyRegistry,
       goal: &Goal,
       resolver: &dyn CurrencyResolver,
       n_rollouts: u32,         // default 1000
   );
   ```
2. For each strategy whose `target` matches the training goal: run `dry_run` (already in `crates/strategies/src/executor.rs:dry_run`) `n_rollouts` times. Each rollout produces a sequence of `(item_state, action)` pairs.
3. For each pair, increment the model's transition count by `samples_per_state_action / n_rollouts × 10` (10× weight per Britz's recommendation).
4. The standard learner (M16.2) then runs from this warm start; off-trajectory states get filled in by random exploration.

**Tests:**

- `crates/advisor/tests/training/imitation_seeds_visit_strategy_states.rs`: after seeding, every state visited by the strategy's `dry_run` has non-zero entries in the model.
- `imitation_seeded_policy_matches_strategy_first_action`: the trained policy from a Normal ilvl 82 BodyArmour with 3xT1-ES goal produces `S2-perfect-transmute` as its first action (matches the user's worked chain). This is the strict superset of the existing `b7_real_strategy_es_body_armour` test from v2 — it asserts not just the strategy emits as a candidate but the *trained policy* picks it as optimal.

### 6.6 Tier 3.6 — Training corpus + CLI (M16.6)

**Goal:** a canonical `goals.toml` enumerating the training corpus, plus the binary that runs it.

**Why "larger" per user answer 2:** more goals = better coverage of decision space. The full corpus is ~50 goals across all gear classes × archetypes × budget tiers.

**Training corpus** (in `pipeline/data/training_goals.toml`):

```toml
# Body Armour
[[goal]] id = "body-armour-3xt1-es-tri-res"        # the user's worked example
[[goal]] id = "body-armour-3xt1-life-tri-res"      # str-base variant
[[goal]] id = "body-armour-2xt1-es-spirit-tri-res" # caster spirit
[[goal]] id = "body-armour-physical-thorns-tri-res" # str-base thorns
[[goal]] id = "body-armour-low-budget-2xt2-es"     # budget = 30 div

# Helmet
[[goal]] id = "helmet-life-accuracy-tri-res"
[[goal]] id = "helmet-es-spirit-int"
[[goal]] id = "helmet-life-low-budget"

# Boots
[[goal]] id = "boots-30ms-life-tri-res"
[[goal]] id = "boots-es-ms-recharge-caster"
[[goal]] id = "boots-25ms-life-low-budget"

# Gloves
[[goal]] id = "gloves-attack-speed-tri-res"
[[goal]] id = "gloves-cast-speed-es-int"
[[goal]] id = "gloves-attack-speed-budget"

# Belt
[[goal]] id = "belt-life-stun-tri-res"
[[goal]] id = "belt-es-spirit-resists"

# OneHand weapons (Sword/Axe/Mace - same archetype)
[[goal]] id = "1h-attack-physical-tri"
[[goal]] id = "1h-attack-elemental-conversion"

# TwoHand weapons
[[goal]] id = "2h-attack-physical-crit"

# Bow
[[goal]] id = "bow-projectile-attack"
[[goal]] id = "bow-projectile-elemental"

# Crossbow
[[goal]] id = "crossbow-projectile-physical"

# Spear
[[goal]] id = "spear-melee-physical"

# Caster weapons
[[goal]] id = "staff-spell-skill"
[[goal]] id = "sceptre-spell-skill"
[[goal]] id = "wand-spell-skill"

# Quiver
[[goal]] id = "quiver-projectile-skill"
[[goal]] id = "quiver-bow-attack-budget"

# Focus
[[goal]] id = "focus-es-spirit"

# Ring
[[goal]] id = "ring-life-tri-res"
[[goal]] id = "ring-es-life-spirit"
[[goal]] id = "ring-life-mana-flat-damage"

# Amulet
[[goal]] id = "amulet-life-spirit-resists"
[[goal]] id = "amulet-es-int-spirit"
[[goal]] id = "amulet-life-spell-skill"

# Talisman
[[goal]] id = "talisman-abyss-synergy"

# Jewel
[[goal]] id = "jewel-cranium-desecrate"
[[goal]] id = "jewel-life-attack-damage"

# Time-lost Jewel
[[goal]] id = "timelost-lightless-prefix"
```

~50 goals total. Each goal entry includes the full `Goal` definition (target, abandon_criteria, budget, ilvl).

**Binary:** `pipeline/src/bin/train_advisor.rs`:

```bash
cargo run -p poc2-pipeline --bin train-advisor \
    --bundle path/to/poc2.bundle.json.gz \
    --goals pipeline/data/training_goals.toml \
    --samples 100000 \
    --reward both \
    --imitation-seeded \
    --out path/to/trained-models.bin
```

Output: a single `bincode`-encoded file containing `Vec<(GoalHash, TrainedModel)>`. Bundles ship this file.

**CI:**

- Quick training smoke test in CI: 1k samples × 3 goals, asserts trained policies beat random on a held-out test goal. Full training (100k × 50 goals) is operator-driven, run on every patch.

### 6.7 Tier 3.7 — Success metrics (M16.7)

**Goal:** quantify training quality and detect regressions across patches.

**Metrics tracked per goal:**

1. **Expected steps to reach goal.** Compare three baselines: trained policy, beam-search-only, strategy-executor.
2. **Expected divine-equivalent cost.** Same three baselines.
3. **Brick rate.** % of trajectories that hit `abandon_criteria` before reaching goal.
4. **Top-action agreement.** % of states where trained Q-table's argmax matches the strategy library's `dry_run` action. Above 85% means imitation is working.
5. **Sim-to-real gap proxy.** Divergence between MC depth-3 success probability and trained-model Q-value mean. Spikes flag a feature-representation hole (per Britz's "Cannot roll Attack Modifiers" example).
6. **Spam-loop fidelity.** For goals known to require Chaos-spam or Annul-Chaos-spam, assert the trained policy's `LoopEstimate.mean_iterations` falls within the 95% CI of the analytic `1 / p_success`. **This is the user's edge-case test.**

**Module:** `crates/advisor/benches/training_metrics.rs` runs all six metrics across the full corpus. CI-gated against regressions: any metric degrading > 10% vs the prior trained-model snapshot fails the gate.

---

## 7. Edge cases the training explicitly addresses

These are the cases where the trained policy materially outperforms beam-search heuristics. The user's question motivated this section.

### 7.1 Chaos-spam loops

**Scenario:** Magic with 1 mod, target = T1 ES prefix on Rare. The chain is Regal → Chaos until T1 ES. Each Chaos has a tiny probability (~0.001 to ~0.01) of hitting T1 ES. Expected iterations: 100 to 1000.

**Why beam search fails:** depth-3 beam search at most evaluates 3 chaos rolls. The Q-value of "Chaos here" is ≈ the Q-value of "Chaos here recursively" because the state mostly returns to itself; beam search at depth 3 can't see past 3 self-loops.

**Why training succeeds:** Q-value iteration computes `Q(s, Chaos) = -1 + Σ P(s' | s, Chaos) × max_a' Q(s', a')`. The self-loop term resolves analytically: `Q(s, Chaos) = -1 + p_loop × Q(s, Chaos) + p_progress × Q(s_progress, _)`. Solving for `Q(s, Chaos)` gives the geometric expectation. Standard Bellman iteration converges to this.

**Test asserting this:** `crates/advisor/tests/training/chaos_spam_geometric_convergence.rs`. Synthetic goal where only Chaos-spam is viable; assert `LoopEstimate.mean_iterations` within 5% of `1 / p_success`.

### 7.2 Annul-Chaos cycles

**Scenario:** Rare with 4 mods (3 keepers + 1 unwanted), target = swap the unwanted for a specific T1. The chain is Annul (try to remove unwanted, may remove keeper) → Chaos (remove + add). Cyclic graph because each cycle returns to either the same state or a worse one.

**Why beam search fails:** the state graph is densely cyclic; beam-search-with-deduplication just keeps stamping out clones.

**Why training succeeds:** the trained transition model captures the loop probabilities exactly; the Q-table converges on the correct EV. The `Recurring` action variant (already shipping from v2 Phase B.4) is what the planner emits, with `LoopEstimate` populated from training metrics.

### 7.3 Long Greater-Essence chains

**Scenario:** Magic with no mods, target = T1 ES prefix + T1 fire-res suffix on Rare. Chain is Trans → Aug → Greater Essence on suffix → maybe re-Aug → Regal. Greater Essence has a 4-mod outcome distribution; per-step probabilities are moderate (~0.05 for hitting T1 ES).

**Why training succeeds:** afterstate aliasing collapses Essence's state-action pairs into a small number; the trained model represents Essence outcomes accurately; the Q-table picks the right Essence variant per step.

### 7.4 Recombinator grinds

**Scenario:** a 3xT1 result on a high-ilvl base via Recombinator iteration. Per the wiki Recombinator formula, success chance depends on `(donor_weights, base_weights, base_coefficient, mod_count_coefficient)`. Each attempt costs 2 input items + Expedition artifacts.

**Why training succeeds:** the engine's `recombine()` function (currently uniform-sampled per the `M5+` placeholder in `crates/engine/src/currency/recombinator.rs:36-39`) needs the wiki's formula implemented to match training data. **This is a v3 prerequisite** — without it, training on Recombinator chains produces wrong policies. We implement the formula in M14.4 (folded into Tier 1.4 as it's an outcome correctness fix).

### 7.5 Bone-spam-then-Reveal flows

**Scenario:** apply bone with omen → reveal at Well of Souls, picking from 3 options. If no acceptable option appears, restart with another bone. Per the wiki, ilvl 65+ items have at least 1 exclusive desecrated guaranteed in the reveal pool (rule R235).

**Why training succeeds:** the reveal pool's distribution is encoded by the desecrated mod fixtures (`pipeline/data/desecrated_mods.json`) + lord-pool restrictions; the trained policy selects the optimal `(bone, omen)` pair per state.

### 7.6 Vaal corruption with rerolls

**Scenario:** finish a near-perfect rare with Vaal, hoping for `RerollValues` outcome. Brick chance is ~25%; with Omen of Corruption it's lower.

**Why training succeeds:** post-M14.4 the Vaal outcome distribution is correct; the trained policy weights Vaal-finish vs Stop-and-sell using the right brick rate.

---

## 8. Verification expectations per layer

### 8.1 Layer 1 verification

- Every tier ships unit tests + integration tests as documented per-tier.
- Aggregate: `cargo test --workspace` passes 376+ existing tests + new tests added by this layer (~80 new tests).
- Performance: `cargo bench --bench advisor_plan` shows ≤ 20% regression vs v2 baseline (weighted sampling is more expensive than 0/1).
- Bundle build: `cargo run -p poc2-pipeline -- build` produces a v2-schema bundle that round-trips cleanly through `bundle.validate()`.

### 8.2 Layer 2 verification

- New strategies parse and pass `load_all_strategies.rs`.
- Cross-source CI test (M15.4) green.
- Rule count: 113 + 25 = 138 rules across 14 sections.
- Confidence distribution: ~70 verified, ~68 community.

### 8.3 Layer 3 verification

- Q-tables converge on the corpus within 50 iterations × 50 goals.
- Training metrics (M16.7) all green vs prior snapshot.
- Trained-model bincode file size < 100 MB; load time < 5 s on bundle init.
- Trained policy beats beam-search-only on:
  - Mean steps to goal: ≥ 30% improvement on long chains (≥ 50 steps).
  - Mean cost: ≥ 20% improvement when cost reward dominates.
  - Brick rate: ≥ 10% reduction.

### 8.4 Cross-layer verification

- `cargo fmt --all --check` clean.
- `cargo clippy --workspace --all-targets -- -D warnings` clean.
- `cargo test --workspace` all green.
- `cd apps/desktop && pnpm check` clean.
- `cd apps/desktop && pnpm build` succeeds.

---

## 9. Failure handling and resilience

Per the user's "no fails" directive, every layer surfaces errors honestly:

- **Layer 1 weight resolution failure:** if `weight_for` returns 0 for every candidate mod (no matching weights at all), the engine logs a tracing warning and falls through to uniform sampling so the advisor doesn't deadlock.
- **Layer 2 cross-source CI failure:** authoring a strategy that violates engine semantics fails CI; the test prints the offending step and the engine's `CannotApply` reason.
- **Layer 3 trained-model load failure:** if the trained model file is missing, corrupted, or mismatched bundle-schema, the planner falls back to beam-search-only and surfaces a one-line "trained policy unavailable, using beam search" hint in the recommendation rationale. The user is never blocked.
- **Trade scraper rate-limit / network failure:** soft-fail with cache fallback, exactly like the existing poe2scout integration. Bundle build succeeds with `confidence: Experimental` weights from the previous scrape.
- **Goal-hash collision:** astronomically unlikely (blake3, 64-bit truncation), but if it happens the trained-model lookup falls back to beam search.

---

## 10. Bundle schema migration (v1 → v2)

Per user answer 5: hard bump.

**Steps (in M14.7):**

1. `crates/data/src/lib.rs`: `BUNDLE_SCHEMA_VERSION = 2`.
2. `Bundle::validate()` rejects v1 bundles with a clear message: "Bundle was built against schema v1; rebuild via `cargo run -p poc2-pipeline -- build`."
3. Desktop loader detects v1-on-disk and prompts: "Your bundle is from a previous version. Click here to rebuild." The button triggers `pipeline build` via a Tauri command that streams progress.
4. **State hard-reset on first v2 launch (per user answer 4):**
   - Loader detects state.toml was written under bundle v1 (via a `bundle_schema_version` field in `state.toml`).
   - Wipes `~/.config/poc2/state.toml` and `~/.config/poc2/recipes/`.
   - Preserves `~/.config/poc2/cache/` (price + meta cache).
   - Shows a one-time dialog explaining the reset; user confirms.
5. `CHANGELOG.md` documents the migration.

---

## 11. Files expected to change

### Engine

- `crates/engine/src/registry.rs` — `from_mods` signature, `weight_for` API.
- `crates/engine/src/base_registry.rs` — new module.
- `crates/engine/src/item.rs` — `Item.base_type_id` field.
- `crates/engine/src/currency.rs` — `ApplyContext` carries `&BaseRegistry`.
- `crates/engine/src/currency/basic.rs` — `total_weight_for_item` rewrite, `*_ONLY` mask threading, Vaal outcome fix.
- `crates/engine/src/currency/bone.rs` — `can_apply_to` with subtype-class table + lord-pool check.
- `crates/engine/src/currency/catalyst.rs` — `can_apply_to` with class gate.
- `crates/engine/src/currency/recombinator.rs` — wiki formula implementation.
- `crates/engine/tests/*.rs` — new + migrated.

### Advisor

- `crates/advisor/src/featurize.rs` — new module.
- `crates/advisor/src/training/mod.rs` — new module tree.
- `crates/advisor/src/training/model_learner.rs`, `value_iteration.rs`, `imitation.rs`, `q_table.rs`.
- `crates/advisor/src/planner.rs` — hybrid planner threading.
- `crates/advisor/src/scorer.rs` — Q-value blending with concept-occupancy.
- `crates/advisor/tests/*.rs` — new training tests.
- `crates/advisor/benches/training_metrics.rs` — new bench.

### Strategies

- `crates/strategies/strategies/*.toml` — 16 new strategies.
- `crates/strategies/src/dsl.rs` + `predicate.rs` — new predicates.
- `crates/strategies/tests/cross_source_validation.rs` — new test.

### Rules

- `crates/rules/seed_rules/*.toml` — 25 new rules across sections.

### Data

- `crates/data/src/lib.rs` — `BUNDLE_SCHEMA_VERSION = 2`.
- `crates/data/src/bundle.rs` — `validate()` rejects v1.
- `crates/data/tests/registry_coverage.rs` — schema bump assertion.

### Pipeline

- `pipeline/src/sources/trade.rs` — new trade-listing scraper.
- `pipeline/src/sources/coe.rs` — alias suggester.
- `pipeline/src/build.rs` — wires trade scraper.
- `pipeline/src/bin/train_advisor.rs` — new binary.
- `pipeline/data/training_goals.toml` — corpus.
- `pipeline/data/coe_aliases.toml` — extended.
- `pipeline/tests/*.rs` — new tests.

### Tauri

- `apps/desktop/src-tauri/src/lib.rs` — bundle loader passes weights, base_registry; trained-model loading; v1→v2 migration prompt.
- New Tauri command `train_advisor_async` for on-demand training.

### Frontend

- `apps/desktop/src/lib/types.ts` — `BaseTypeId` becomes engine-canonical.
- `apps/desktop/src/lib/AdvisorPanel.svelte` — surface `LoopEstimate` from trained-model output.
- `apps/desktop/src/lib/SettingsPanel.svelte` — "Retrain advisor" button + progress.
- `apps/desktop/src/lib/RecipeLibrary.svelte` — bundle-schema-v1 hard-reset dialog.

### Docs

- `docs/81-engine-training-and-rule-encoding-plan.md` — this file.
- `docs/82-training-corpus.md` — corpus per-goal documentation.
- `docs/83-bundle-schema-v2.md` — migration notes.
- `docs/adr/0010-trained-policy-q-tables.md` — ADR for the Q-table approach.
- `docs/adr/0011-base-registry.md` — ADR for the engine-canonical `BaseTypeId`.
- `docs/adr/0012-trade-listing-scrape.md` — ADR for the 30-min refresh + future server-cache.

---

## 12. State at handoff (what exists when this plan starts executing)

When this plan begins, the following are already in place:

- v1.0 release tagged at `4861ad1` on `main`. v1 features all working.
- v2 release at commit `e582134` on `main`. v2 features all working: rarity gating, omen-aware bone reveals, recurring step compression, structured CannotApply over IPC, item-preview row animation, ES body armour deterministic test, poe2db scrape behind `regen-poe2db-fixtures` flag.
- 376+ tests pass across 12 crates including the desktop Tauri side.
- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `pnpm check`, `pnpm build` all clean.
- Curated fixtures (desecrated, Vaal implicits) ship with the bundle.
- Trained models do NOT exist yet — that's Layer 3 of this plan.
- `bundle.weights` is populated from CoE but unread at runtime — that's Layer 1.1 of this plan.
- The 24 strategies cover BodyArmour and Ring/Amulet only — Layer 2.1 expands this.

When picking up this plan, start by reading sections 0, 1, 2, 3 to ground in the constraints and decisions, then execute Layer 1 in order (M14.1 first because every other tier depends on it).

---

## 13. Glossary (additions vs v2 plan §15)

- **BaseRegistry:** engine-canonical lookup `BaseTypeId → BaseType` with class + tag indices.
- **FeatureVec:** compact featurized representation of an `Item` relative to a `Goal`. Maps to a Q-table key.
- **TableModel:** offline-learned `P(s' | s, a)` as a `HashMap<(FeatureVec, AdvisorAction), HashMap<FeatureVec, f64>>`.
- **Q-table:** offline-computed `Q(s, a)` for a (goal, reward) pair.
- **TrainedModelCache:** per-(goal_hash, item_class) Q-tables shipped with the bundle.
- **Goal hash:** stable canonical hash of a `Goal` for trained-model lookup. blake3 over canonical-form serialization.
- **Afterstate:** an action whose next-state distribution is independent of the current state's non-feature aspects. Essences, Exalts, Trans/Aug/Regal are afterstates because their distributions depend only on the post-action item shape.
- **Sim-to-real gap:** divergence between the trained model's predicted outcome and the engine simulator's actual outcome. A non-zero gap indicates the feature representation is missing relevant state.
- **Spam-loop fidelity:** training metric asserting the trained policy's `LoopEstimate.mean_iterations` matches the analytic geometric expectation for known spam loops.
- **Imitation seeding:** pre-loading the model learner with expert trajectories from the strategy library.

---

## 14. Open questions answered

The user's answers to scoping questions are baked into this plan; they are not negotiable inputs for the executing agent. Reproduced for handoff:

| # | Question | Answer | Where applied |
|---|---|---|---|
| 1 | Split v3 across releases? | No, ship as one v3 release. | Plan structure overall. |
| 2 | Training corpus size? | Larger (~50 goals, enumerated in §6.6). | Tier 3.6. |
| 3 | Trade scraper cadence? | 30-min refresh; later move to server-cache. | Tier 1.7 + ADR-0012. |
| 4 | State migration UX? | Hard reset on bundle schema bump. | §10. |
| 5 | Bundle schema bump? | Yes, v1 → v2. | §10 + Tier 1.7. |
| 6 | Reward function priority? | Whichever produces the best results regardless of training time. Both shipped. | Tier 3.3. |
| 7 | WeightObservation confidence? | All `community`. | Tier 1.7. |

Edge-case handling for the user's concrete question about Chaos-spam-style loops: covered in §7 ("Edge cases the training explicitly addresses"); training is *better* than beam-search precisely because cyclic spam loops are where the Bellman fixed-point closes the geometric expectation analytically. Test in M16.7 metric 6 ("Spam-loop fidelity") asserts the trained policy's `LoopEstimate.mean_iterations` matches `1 / p_success` within 5%.

---

## 15. Out-of-scope for v3 (explicitly deferred)

These are mentioned for completeness so the executing agent doesn't burn time on them:

- **Neural network policy** instead of Q-table. Revisit in v4 if state-space coverage becomes a bottleneck.
- **Server-side trade-listing cache.** v3 ships per-user 30-min scrape; v3.x adds server cache.
- **Cross-platform support** (Windows, macOS, Wayland layer-shell). Still v1.x deferred per ADR-0009.
- **Full Charm / Tablet / Waystone strategy authoring.** Their crafting flows differ structurally; deferred to v3.x.
- **GGG OAuth `/trade2`.** v1 uses URL-only deep links; v3 doesn't change this.
- **MCTS variants** (UCT-on-graphs, AlphaZero-style self-play). Britz's analysis confirms MCTS is wrong for this domain; we don't revisit.
- **Plugin component-model migration.** Wasm component model is post-v3.
- **`tauri-plugin-opener` migration.** Cosmetic, not blocking training.
- **Beam-search memoization.** v1 benchmarks show planning is well under budget; revisit only if real workloads degrade.

---

End of plan. Total length: 15 sections, ~24 engineering days estimated, ~50 new tests across all layers.
