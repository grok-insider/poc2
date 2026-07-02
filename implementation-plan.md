# Implementation Plan & Handoff — Crafting-Mechanics Fidelity + PoE2 0.5

> **HISTORICAL — superseded handoff.** This snapshot predates the
> web/WASM migration and the Electron shell: the Tauri 2 + Svelte stack,
> `apps/desktop/src-tauri`, NixOS-only scope, and `main`-branch flow it
> describes are gone. For current context read `README.md`, `AGENTS.md`,
> `CHANGELOG.md`, `docs/70-roadmap.md`, `docs/83-crafting-fidelity-plan.md`,
> and ADR-0010/0013. The "do not commit" rule below applied to that
> session only.
>
> **Audience:** an AI agent or developer picking this up with **zero prior
> context**. Read this top-to-bottom before touching code. It records what
> the project is, what has been done in the current effort, exactly how it
> was verified, what remains, and the conventions you must follow.
>
> **Hard rule from the user:** **DO NOT COMMIT under any circumstance.** The
> user performs all commits. Leave changes in the working tree.

---

## 1. What this project is

**Path of Crafting 2 (`poc2`)** — a native desktop crafting **advisor** for
Path of Exile 2 (PoE2). You import an in-game item, declare target mods +
budget, and the advisor recommends the optimal next currency/omen with EV
math, Monte-Carlo confidence bands, and full traceability to the source
rule/strategy. It re-plans on every state change and surfaces recovery
branches after failures.

- **Stack:** Rust workspace (edition 2021, MSRV 1.82) + Tauri 2 desktop app
  + Svelte 5 / Vite / TypeScript frontend. Nix flake dev/release flow.
- **Platform:** NixOS + Hyprland only (v1 scope; do not add other
  compositors unless asked).
- **Crates:** `engine`, `data`, `strategies`, `rules`, `probability`,
  `market`, `advisor`, `parser`, `plugin-host`, `plugin-sdk`, `pipeline`,
  plus the Tauri app at `apps/desktop/src-tauri` (crate `poc2-desktop`).

### Version history (important context)
- **v1.0** (tag `v1.0.0` at `4861ad1`, may be unpushed) targeted patch 0.4.
- **v2** (`docs/80-crafter-helper-v2-plan.md`) — shipped: rarity gating,
  full mod-pool outcome dialog, cost-aware ranking, recurring-step loops.
- **v3** (`docs/81-engine-training-and-rule-encoding-plan.md`) — shipped:
  numeric weights wired in, `BaseRegistry`, `*_ONLY` flag enforcement,
  Vaal/Recombinator fixes, per-class strategies, trained-policy advisor
  (Q-learning), training corpus + `train-advisor` binary. Bundle schema v2.
- **This effort** (`docs/83-crafting-fidelity-plan.md`) — crafting-mechanics
  fidelity + migration to **patch 0.5 "Return of the Ancients"** (live in
  game since 2026-05-29). Bundle schema bumped to **v3**.

`main` is well past `v1.0.0`. The README/CHANGELOG/AGENTS were stale (said
"v1.0") and have been updated by this effort.

---

## 2. The core problem this effort solves

The user asked to make the engine model real PoE2 crafting mechanics, in
their words:

> "the weights are dependant of item level the pool changes based on that and
> which item version you use like exalt, greater exalt and perfect exalt will
> have different pools since some mods are not taken in count depending on
> item level or because they cant roll on low tier of a modifier"

Three coupled mechanics, all now implemented:

1. **Item-level-dependent pools.** A mod tier is eligible only when
   `required_level <= item.ilvl`. Higher ilvl unlocks higher tiers → the
   rollable pool grows with ilvl.
2. **Min-Modifier-Level currency variants.** Exalted (no floor) vs Greater
   Exalted (floor 35) vs Perfect Exalted (floor 50) produce **strictly
   nested shrinking** pools. Same for Greater/Perfect Transmute/Aug/Regal/
   Chaos. **Exception (subtle):** a floor excludes *tiers*, never an entire
   mod-*type* — "at least one tier of each mod type always rolls, respecting
   item level" (e.g. Light Radius, max tier L30, still rolls under a Perfect
   floor of 50).
3. **Inclusive tier weighting.** Aiming for a tier inherits the spawn weight
   of the same-group higher tiers rollable at the current ilvl
   (`Σ_{j=m_i}^{m_t0} w_j`, with `m_t0` ilvl-dependent). This is why crafts
   get cheaper at higher ilvl once a new top tier unlocks.

Plus a **cross-version** requirement: the same engine binary must evaluate
patches 0.3 / 0.4 / 0.5 correctly (different disabled items, floors, league
gates).

---

## 3. Authoritative game-mechanics reference (verified from sources)

These are encoded in the engine. Re-verify against the cited sources before
changing them.

### 3.1 Where the data comes from
- PoE2 game data lives in GGG's `Content.ggpk` as binary `.datc64` tables.
  **GGG publishes no official export.**
- Chain: `Content.ggpk` → community schema `poe-tool-dev/dat-schema` →
  extractors (PyPoE etc.) → JSON.
- **poe2db.tw** datamines the GGPK; weights are **not** in game files — it
  imports Krakenbul / Prohibited Library recombinator-derived weights and
  parses trade-listings for non-recombinable bases. (`poe2db.tw/weightings`.)
- **RePoE-fork** (`repoe-fork.github.io/poe2/`) is our **primary** source
  (mods/bases/tags). **Craft of Exile** (`craftofexile.com/json/poe2/main/poec_data.json`)
  is our **weights** source. Both already serve 0.5.
- **Our pipeline source architecture is correct and unchanged** — 0.5 was a
  content + mechanics migration, not a re-architecture.

### 3.2 Min-Modifier-Level floors (patch-versioned)
| Variant | 0.3/0.4 | 0.5 | Source |
|---|---|---|---|
| Greater Transmutation | 35 | lowered (engine uses 20, `TODO(0.5-data)`) | patch notes |
| Greater Augmentation | 55 | lowered (engine uses 35, `TODO(0.5-data)`) | patch notes |
| Greater Regal | 50 | 50 | wiki |
| Greater Exalted | **35** | 35 | wiki (NOT the legacy engine's 50) |
| Greater Chaos | 50 | 50 | wiki |
| Perfect Exalted | **50** | 50 | wiki (NOT 70) |
| Perfect (Trans/Aug/Regal/Chaos) | 70 | 70 | historical |

> The 0.5 Greater Transmute/Aug values are community estimates marked
> `TODO(0.5-data)` in `crates/engine/src/currency/basic.rs`
> (`MinModLevelVariant::floor`). When a live 0.5 bundle exposes per-currency
> `Minimum Modifier Level`, source them from data and remove the TODO.

### 3.3 Essences (0.3 rework, stable 0.4/0.5)
- Lesser/Normal/Greater: Magic→Rare, add one fixed mod.
- Perfect/Corrupted: remove one random non-fractured mod, then add fixed mod
  (Rare only). Sinistral/Dextral Crystallisation force the removed side.
- Family-collision illegal; essence mod can raise item level requirement.

### 3.4 Desecration / bones (0.3+)
- Bone size = ilvl gate: **Gnawed ≤ ilvl 64; Preserved any; Ancient → Min
  Modifier Level 40.**
- Subtype → class: Rib→armour; Jawbone→weapon/quiver; Collarbone→
  ring/amulet/belt/talisman; Cranium→jewel (Preserved only); Vertebrae→
  waystone (not in engine enum yet).
- Reveal = choose 1 of 3; at ilvl 65+, ≥1 option is exclusive-desecrated iff
  the class has an exclusive for that affix.
- **Lord omens (Liege=Amanamu, Sovereign=Ulaman, Blackblooded=Kurgal) are
  Weapon/Jewellery ONLY**, block the other two lords, and **brick the Ancient
  Min-Mod-Level-40 floor** (consumed even if no lord mod is possible).

### 3.5 Cross-version gates (full matrix in `docs/14-crafting-mechanics-cross-version.md`)
- 0.3: essence rework, Greater/Perfect orb tiers, desecration/bones,
  Hinekora's Lock, Sanctification. Removed omens: Greater Annulment,
  Dextral/Sinistral Alchemy/Coronation.
- 0.4: Homogenising Exaltation/Coronation disabled (0.3-only); Architect's
  Orb / Vaal Cultivation; Rune Socket→Augment Socket rename.
- 0.5 "Return of the Ancients": **Recombinator removed** (Standard-only);
  Greater/Perfect rarer, Trans/Aug rarer, Divine more common; Greater
  Trans/Aug floors lowered; **Verisium Alloys** + Runic Ward; **Genesis
  Tree** (new ring/amulet/belt bases + caster/minion mods); **Liquid/Ancient
  Emotions** (jewel essence-likes); **12 new Jewel catalysts** (catalysts no
  longer drop); Omen of Corruption + Homogenising Standard-only; Chaotic
  Rarity/Quantity/Monsters inverted + new Chaotic Effectiveness; new weapon
  classes (Flail/Claw/Dagger/Warstaff).

---

## 4. Current state — what is DONE (all verified green)

**Verification baseline (re-run to confirm):**
- `cargo fmt --all --check` — clean
- `cargo clippy --workspace --all-targets -- -D warnings` — clean (0 issues)
- `cargo test --workspace` — **59 test blocks, 0 failures**
- `cd apps/desktop && pnpm check` — 0 errors; `pnpm build` — succeeds
- `cargo build -p poc2-desktop` (Tauri) — succeeds

### 4.1 Engine — crafting fidelity
- **`League` enum** (`crates/engine/src/patch.rs`): `Standard` | `Challenge`
  (default `Challenge`). Threaded through `ApplyContext`
  (`crates/engine/src/currency.rs`) via the `league` field + `with_league`.
- **Tier ordinal** on `ModDefinition` (`crates/engine/src/mods.rs`):
  `tier: Option<u16>` (`#[serde(default)]`, 1 = strongest) + helper
  `tier_strength_key()` (falls back to `required_level` when `tier` is None).
- **Inclusive higher-tier weighting**:
  `ModRegistry::inclusive_weight_for` / `inclusive_weight_for_on_base`
  (`crates/engine/src/registry.rs`), consumed by `sample_eligible_mod`
  (`crates/engine/src/currency/basic.rs`) via `inclusive_weight_for_item`.
- **Keep-≥1-tier floor exception** in `sample_eligible_mod`
  (`basic.rs`): groups fully below the Min-Mod-Level floor retain their
  highest-`required_level` tier.
- **Patch-versioned floors**: `MinModLevelVariant` enum + `.floor(patch)`
  (`basic.rs`) replaced the old `const MIN_LEVEL_*`. The Greater/Perfect
  add-currency + chaos macros call `$variant.floor(ctx.patch)`.
- **Tag-intersection weighting** (leftmost-tag-wins):
  `ModDefinition::spawn_weight_for_tags` (`mods.rs`) +
  `ModRegistry::weight_for_on_base` (`registry.rs`). `weight_for` now
  delegates to `numeric_weight` + `eligibility_stub` helpers; the tag path is
  used when `BaseRegistry` tags are available (threaded from the sampler).

### 4.2 Engine — desecration / bones
`crates/engine/src/item.rs`:
- `BoneSize::max_ilvl()` (Gnawed=Some(64)) + `min_mod_level()` (Ancient=40).
- `BoneSubtype::supports_lord_pool()` now only `Jawbone | Collarbone`
  (Weapon/Jewellery). Rib (armour) + Cranium (jewel) → false.
- `HiddenDesecratedSlot` gained `min_mod_level: u32` (`#[serde(default)]`).

`crates/engine/src/currency/bone.rs`:
- `Bone::apply` enforces the Gnawed ilvl-64 ceiling, computes the effective
  floor (Ancient=40, **bricked to 0 by a lord omen**), stores it on the slot.
- `sample_reveal_options` filters by `required_level >= hidden.min_mod_level`.

### 4.3 Engine — cross-version gating
- `recombinator_available(patch, league)` + `recombine_gated`
  (`crates/engine/src/currency/recombinator.rs`): Recombinator disabled in
  0.5 unless `League::Standard`.
- `OmenSet::consume_prevent_no_change(patch, league)`
  (`crates/engine/src/omen.rs`): Omen of Corruption Standard-only in 0.5.
- Homogenising omens remain 0.3-only (pre-existing `patch_range`).

### 4.4 Engine — new 0.5 system
- **Verisium Alloys**: `crates/engine/src/currency/alloy.rs` — `Alloy`
  currency, essence-like remove-then-add on a Rare item, patch-gated to 0.5+,
  with Crystallisation affix-forcing + family-collision rules. Registered in
  `DefaultCurrencyResolver` (`resolver.rs`) via `with_alloys` / `add_alloy`.

### 4.5 Pipeline / data
- **Bundle schema → v3** (`crates/data/src/lib.rs`: `BUNDLE_SCHEMA_VERSION = 3`).
  v2 bundles hard-rejected; the legacy-rejection test is parameterized on
  `BUNDLE_SCHEMA_VERSION` (`crates/data/src/bundle.rs`).
- **Tier-ordinal derivation post-pass**:
  `pipeline/src/normalize/tiers.rs::assign_tier_ordinals`, wired into
  `pipeline/src/build.rs` (after `flag_essence_target_mods`).

### 4.6 Advisor
- `PlanInput.league` threaded end-to-end (`crates/advisor/src/planner.rs`,
  `lib.rs`); candidate generator (`candidate.rs`) drops `Recombine`
  candidates when `recombinator_available` is false.
- **Guidance ranking fix** (`planner.rs`): `Guidance` actions are excluded
  from beam expansion (non-mutating) and collected as a depth-1 fallback via
  `advisory_recommendations`; the grouping was extracted into
  `group_by_first_action`. The rule note now populates the guidance card
  (`candidate.rs`: `note.clone_from(&r.suggestion.note)`).
- **Trained-model schema guard** (`crates/advisor/src/training/artefact.rs`):
  `load_artefact_file` skips models whose `bundle_schema_version` /
  `engine_schema_version` mismatch the current build, so a stale 0.4 model is
  ignored and the advisor falls back to heuristics.

### 4.7 Live 0.5 bundle (built + installed)
A real 0.5 bundle was built from live sources and installed at
`~/.config/poc2/bundles/poc2.bundle.json.gz`:
- schema_version 3, game_patch 0.5.0, **3098 mods, 4988 weights**, tier
  ordinals assigned, 45 omens, 81 essences, 12 catalysts, 10 bones.
- The advisor plans against it end-to-end and recommends Perfect Orb of
  Transmutation as the first step for a Normal ES body-armour goal.

### 4.8 Docs
- New: `docs/83-crafting-fidelity-plan.md` (detailed plan + progress log),
  `docs/14-crafting-mechanics-cross-version.md` (0.3/0.4/0.5 delta matrix).
- Updated: `docs/11-game-mechanics.md` (0.5 baseline + weight/tier sections),
  `README.md`, `CHANGELOG.md` (`[Unreleased]` section), `AGENTS.md`.

### 4.9 New tests (all passing)
- `crates/engine/tests/weight_ilvl_pool.rs` — inclusive weighting, "Tyrannical
  doubles at ilvl 82", boundaries, monotonicity.
- `crates/engine/tests/min_mod_level_pools.rs` — nested Exalt⊃Greater⊃Perfect
  pools, keep-≥1-tier exception (Light Radius), ilvl boundary.
- `crates/engine/tests/crafting_invariants_proptest.rs` — 4 proptests.
- `crates/engine/tests/desecration_mechanics.rs` — bone-size/ilvl, lord-omen
  scope, Ancient-floor brick.
- `crates/engine/tests/essence_mechanics.rs` — tier-rarity, family collision,
  Crystallisation, corrupted-item gating.
- `crates/engine/tests/cross_version_gating.rs` — Recombinator / Corruption /
  Homogenising gates.
- `crates/advisor/tests/live_bundle_smoke.rs` — loads the on-disk bundle and
  plans (skips gracefully if no bundle; override path with `POC2_BUNDLE`).
- Engine unit tests for `Alloy`, tag-intersection weighting, patch-versioned
  floors, tier-ordinal derivation, trained-model version guard.

---

## 5. What REMAINS (prioritized)

### 5.1 CoE→engine weight join rate (~49.8%) — quality, not blocking
The live 0.5 build logs "more CoE mods unmatched (653) than matched (647)".
Unmatched mods fall back to **tag-intersection weighting** (works, less
precise). To improve:
- `cargo run -p poc2-pipeline -- coe-aliases-suggest <bundle> --top-k 1 --out suggestions.toml`
- **Do NOT bulk-merge.** Auto-suggestions include wrong-domain matches even at
  score=1.00 (e.g. "+# to Spirit" → `AlloySpiritOnBoots1`, "+# to Dexterity" →
  an *implicit*). Each addition to `pipeline/data/coe_aliases.toml` must be
  verified against live RePoE-fork `mods.json` / poe2db that the engine_mod_id
  is the correct **general explicit** mod. This is careful curation work.

### 5.2 0.5 Min-Mod-Level floors from data
`MinModLevelVariant::floor` (`basic.rs`) hard-codes 0.5 Greater Transmute=20 /
Aug=35 as community estimates (`TODO(0.5-data)`). Confirm from poe2db's
per-currency "Minimum Modifier Level" and update.

### 5.3 New 0.5 systems beyond Alloys (data-driven; no engine code needed)
- **Genesis Tree** mods + new ring/amulet/belt bases → flow through
  tag-intersection weighting; they appear once the bundle carries them.
- **Jewel emotions (Liquid/Ancient)** → mechanically identical to the
  essence/Alloy remove-add path; if a dedicated currency type is wanted,
  clone `alloy.rs` scoped to Jewels.
- **12 new Jewel catalysts** → already supported by the data-driven
  `Catalyst` (Jewel is an eligible class). Populate via `with_catalysts`.
- **Verisium Alloy catalogue wiring**: `Alloy` exists + resolves, but the
  pipeline does not yet *emit* an alloy catalogue into the bundle, and
  `DefaultCurrencyResolver` is not yet seeded with alloys from the bundle.
  When 0.5 alloy data is available, add a bundle section + resolver seeding
  analogous to essences/catalysts.

### 5.4 Optional: retrain the advisor on the 0.5 bundle
The advisor works heuristically without trained models. To restore the
trained-policy uplift on 0.5:
```
cargo run --release --bin train-advisor -- \
  --corpus pipeline/data/training_goals.toml \
  --bundle ~/.config/poc2/bundles/poc2.bundle.json.gz \
  --out ~/.config/poc2/cache/trained_models/poc2-trained-models-0.5.0.json \
  --samples 10000 --verbose          # smoke ~10 min; 100000 for production
```
Always pass `--bundle` (without it `V_path(s0)` degenerates to -1000). The new
artefact will carry schema v3 and be accepted by the version guard; the stale
0.4 artefact is now correctly ignored.

### 5.5 `BaseAtIlvl` weight emission
The engine indexes + resolves per-ilvl-stratified weights
(`WeightScope::BaseAtIlvl`), but the pipeline does not yet emit them. If a
source provides ilvl-stratified weights, emit `WeightObservation`s with that
scope; the resolution path is already wired.

---

## 6. Conventions & gotchas (read before editing)

- **NEVER commit.** The user commits. Leave changes in the working tree.
- **`ApplyOutcome` is `()`** (`crates/engine/src/currency.rs`). Currency
  `apply` returns `EngineResult<ApplyOutcome>`; success is `Ok(())`.
- **`ModDefinition` has 16 fields** including the new `tier`. Every struct
  literal needs it; most code uses `tier: None` (the pipeline post-pass
  assigns real values). `HiddenDesecratedSlot` needs `min_mod_level` (use 0
  in test literals). Both are `#[serde(default)]`, so deserialization of
  older JSON still works, but Rust struct literals must include them.
- **Currency exports:** basic orbs are under `poc2_engine::currency::basic`;
  `Alloy`, `Bone`, `recombine_gated`, `recombinator_available` are re-exported
  from `poc2_engine::currency`.
- **No network in ordinary tests.** poe2scout/poe.ninja/pipeline fetches must
  soft-fail or use fixtures. `live_bundle_smoke` reads a local file and skips
  if absent.
- **Patch parameterization:** `PatchVersion::PATCH_0_4_0`, `PATCH_0_5_0`;
  `PatchRange::{ALL, from, until, between}`. Tests run one body across
  versions by constructing different `(patch, league)` in `ApplyContext` /
  `PlanInput`.
- **Bundles/large data are artifacts** — they live in `~/.config/poc2/`, not
  the repo. Never commit a `.bundle.json.gz` or base-icons.
- **Uncertain mechanics** (desecration exclusive weights, Modifier-Tier-
  Rating): model best-effort, tag weights `Confidence::Experimental`, test
  invariants with wide tolerances rather than exact probabilities.

---

## 7. How to verify (the gate)

Run all of these; they must be clean before declaring done:
```
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cd apps/desktop && pnpm check && pnpm build
cargo build -p poc2-desktop          # Tauri Rust side
```
Optional end-to-end (needs an installed bundle):
```
cargo test -p poc2-advisor --test live_bundle_smoke -- --nocapture
# or point at a specific bundle:
POC2_BUNDLE=/path/to/poc2.bundle.json.gz cargo test -p poc2-advisor --test live_bundle_smoke
```
Rebuild a live 0.5 bundle (needs network):
```
cargo build --release -p poc2-pipeline
./target/release/poc2-pipeline build --out ~/.config/poc2/bundles/poc2.bundle.json.gz --patch 0.5.0
./target/release/poc2-pipeline info ~/.config/poc2/bundles/poc2.bundle.json.gz
```

---

## 8. Key file index (where things live)

| Concern | File |
|---|---|
| Weight resolution + inclusive weighting + tag-intersection | `crates/engine/src/registry.rs` |
| Sampling (ilvl gate, floor exception, inclusive weight) | `crates/engine/src/currency/basic.rs` (`sample_eligible_mod`, `MinModLevelVariant`) |
| Tier ordinal + `spawn_weight_for_tags` + flags | `crates/engine/src/mods.rs` |
| `League` / `PatchVersion` / `PatchRange` | `crates/engine/src/patch.rs` |
| `ApplyContext` (carries patch + league) | `crates/engine/src/currency.rs` |
| Bones / desecration / lord omens | `crates/engine/src/currency/bone.rs`, `crates/engine/src/item.rs` |
| Essences | `crates/engine/src/currency/essence.rs` |
| Verisium Alloys | `crates/engine/src/currency/alloy.rs` |
| Recombinator gating | `crates/engine/src/currency/recombinator.rs` |
| Omen consumption (Corruption league gate) | `crates/engine/src/omen.rs` |
| Currency resolver (essences/catalysts/alloys/bones) | `crates/engine/src/currency/resolver.rs` |
| Bundle schema version + history | `crates/data/src/lib.rs`, `crates/data/src/bundle.rs` |
| Pipeline build orchestration | `pipeline/src/build.rs`, `pipeline/src/main.rs` |
| Tier-ordinal derivation | `pipeline/src/normalize/tiers.rs` |
| CoE / RePoE / poe2db sources + normalizers | `pipeline/src/sources/`, `pipeline/src/normalize/` |
| CoE alias curation | `pipeline/data/coe_aliases.toml` |
| Advisor planner (beam search, guidance fix, league) | `crates/advisor/src/planner.rs` |
| Advisor candidate generation + gates | `crates/advisor/src/candidate.rs` |
| Trained-model loading + version guard | `crates/advisor/src/training/artefact.rs` |
| Desktop Tauri commands + bundle loader | `apps/desktop/src-tauri/src/lib.rs` |
| Plans | `docs/83-crafting-fidelity-plan.md`, `docs/14-crafting-mechanics-cross-version.md`, `docs/11-game-mechanics.md` |

---

## 9. Suggested next session (concrete first steps)

1. Re-run the verification gate (§7) to confirm a clean baseline.
2. If a fresh 0.5 bundle is desired, rebuild it (§7) and run
   `live_bundle_smoke`.
3. Tackle §5.1 (CoE alias curation) for higher weight precision **or** §5.4
   (retrain advisor on 0.5) for the trained-policy uplift — both are
   independent and additive.
4. Keep each change small, run the narrow crate tests then the full gate, and
   **do not commit** — hand finished work back to the user.
