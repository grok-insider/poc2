# v1.0 Execution Plan

> Companion to [`70-roadmap.md`](70-roadmap.md). This document tracks the
> phase-level execution of the remaining work to v1.0. Each section gets
> a checkbox per deliverable; commits tick them as work lands.
>
> **Scope decisions (locked in by user)**:
> - Full coverage of strategies (15 remaining) and rules (~75 remaining)
> - Wayland overlay deferred to v1.1; v1 ships always-on-top via Hyprland
> - Trade integration: URL-only for v1; OAuth later
> - Monte Carlo aggregator before v1
> - Wasm plugin SDK **shipped in v1** (re-opens ADR-0008)
> - NixOS+Hyprland-only platform
> - Flake-only release distribution

## Phase summary

| Phase | Deliverable theme | Walltime | Status |
|---|---|---|---|
| A | Catalogue + data refinement | ~2 weeks | in progress |
| B | UI completion (M6) | ~1.5 weeks | pending |
| C | Advisor MC + streaming | ~1 week | pending |
| D | Live integration (M7 sans overlay) | ~1.5 weeks | pending |
| E | Market enrichment (poe.ninja) | ~1 week | pending |
| F | Wasm Plugin SDK | ~3-4 weeks | pending |
| G | Polish + release | ~1 week | pending |

**Total walltime estimate**: ~10-12 weeks.

## Critical path

```
A ──────────────────────────┐
                            │
B ──────────────────────────┤
                            │
C ──────────────────────────┤
                            ├──► G (release)
D ──┬───────────────────────┤
    │                       │
E ──┘                       │
                            │
F (depends on A) ───────────┘
```

A's predicate-language extension (A.1) is the linchpin — it touches the
strategies, rules, and advisor crates. Everything downstream consumes
the new `PredicateContext`.

---

## Phase A — Catalogue + data refinement

### A.1 — Predicate language extension
- [ ] Refactor `strategies::predicate::eval(predicate, item, registry)` →
      `eval(predicate, item, ctx)` with `PredicateContext { registry,
      cost_so_far: DivEquiv, valuator: &Valuator, stash: &Stash,
      expected_sale_price_div: Option<f64> }`
- [ ] New `FloatValuePredicate { op, value: f64 }`
- [ ] New `ItemPredicate::CostSpent { op, value_div: FloatValuePredicate }`
- [ ] New `ItemPredicate::ExpectedSalePrice { op, value_div: FloatValuePredicate }`
- [ ] New `ItemPredicate::StashHas { currency: CurrencyId, count: ValuePredicate }`
- [ ] New `ItemPredicate::Quality(ValuePredicate)`
- [ ] New `ItemPredicate::HasDesecratedRevealed(bool)`
- [ ] New `ItemPredicate::ModCount(ValuePredicate)`
- [ ] Thread `ctx` through `rules::engine::evaluate`
- [ ] Thread `ctx` through `advisor::candidate::generate_candidates`
- [ ] Update existing 45 seed rules + 8 strategies to compile against new signature
- [ ] Unit tests for each new predicate

### A.2 — DSL action extensions
- [ ] `Action::ActivateOmen { omen: OmenId }` — separates omen-binding from apply
- [ ] `Action::RevealWithPreference { prefer: Vec<ConceptId>, use_abyssal_echoes: bool }` (richer than current `Reveal`)
- [ ] `Action::Recombine { other_item: ItemPredicate }` — caller chooses second item from stash
- [ ] Strategy executor dispatch for new variants
- [ ] Advisor `from_strategy_action` lift
- [ ] Tests

### A.3 — CoE→engine mod-id join refinement
- [ ] Sample current 593 unmatched mods → catalog failure modes (script in `pipeline/scripts/`)
- [ ] Build `pipeline/data/coe_aliases.toml` for explicit name overrides
- [ ] Add stat_id-based fallback when CoE exposes RePoE-fork stat ids
- [ ] Verify ≥ 80% join rate via `pipeline/info`

### A.4 — Encode 15 remaining strategies
- [ ] #5 Perfect Essence + Crystallisation (Isolation Variant)
- [ ] #6 Greater Exaltation Stacking
- [ ] #9 Omen of Light Desecration Cleanup
- [ ] #10 Hinekora's Lock Save-State (Preview)
- [ ] #11 Sanctification Finish (Mirror-Tier Polish)
- [ ] #12 Vaal Corruption Finish
- [ ] #13 Double Corruption (Twice-Corrupt) — 0.4
- [ ] #14 Bones with Abyssal Echoes (Reroll Reveal Options)
- [ ] #15 Liege / Blackblooded / Sovereign Omens
- [ ] #16 Recombinator Strategy (Omen of Recombination)
- [ ] #17 Magic Base Exit Strategy (Sell at Magic)
- [ ] #19 Belton's Four-T1 Rubric
- [ ] #20 Mark of the Abyss Swap (Essence of the Abyss Combo)
- [ ] #21 ilvl 82 + Tri-Resist Convergence (cross-cutting)
- [ ] #22 Wraeclast / Itemized Crafting Workflow Order (cross-cutting)
- [ ] #23 Exceptional Bases Exploit (cross-cutting)

### A.5 — Encode 75 remaining rules
- [ ] Migrate `crates/rules/src/seed.rs` → `crates/rules/seed_rules/*.toml` (one per category)
- [ ] Loader walks `seed_rules/` dir at startup
- [ ] Section 1 (Abandonment) — full coverage (10 rules)
- [ ] Section 2 (Fracture timing) — full (8)
- [ ] Section 3 (Hinekora's Lock) — full (6)
- [ ] Section 4 (Exalt-vs-Desecrate) — full (10)
- [ ] Section 5 (Whittle-vs-Annul) — full (8)
- [ ] Section 6 (Stop-vs-Continue) — full (6)
- [ ] Section 7 (Pricing exit) — full (7)
- [ ] Section 8 (Budget) — full (6) — uses `CostSpent` predicate
- [ ] Section 9 (Item Base Selection) — full (10)
- [ ] Section 10 (Vaal Corruption) — full (13)
- [ ] Section 11 (Market Awareness) — full (7)
- [ ] Section 12 (Recovery) — full (10)
- [ ] Section 13 (Confidence/EV) — full (8)

### A.6 — Bundle hot-swap
- [ ] `reload_bundle(path: Option<String>)` Tauri command
- [ ] Settings UI "Reload bundle" button
- [ ] AdvisorState fields wrapped in `Arc<Mutex<...>>` where necessary

### A.7 — Author missing docs
- [ ] `docs/37-recovery-flows.md`
- [ ] `docs/32-probability-math.md`
- [ ] `docs/36-decision-engine.md`
- [ ] `docs/51-market-meta.md`

---

## Phase B — UI completion

### B.1 — Target panel
- [ ] `apps/desktop/src/lib/TargetPanel.svelte`
- [ ] Concept picker driven by `bundle.concepts`
- [ ] Tier-min slider per spec
- [ ] `allow_hybrid` toggle
- [ ] Budget editor (DivEquiv triple)
- [ ] Replaces hard-coded `WORKED_EXAMPLE_GOAL`
- [ ] Goal persisted to `~/.config/poc2/state.toml`

### B.2 — Recovery panel
- [ ] `apps/desktop/src/lib/RecoveryPanel.svelte`
- [ ] Reads `Step::recovery` from active strategy
- [ ] Visible only when last action's outcome was a failure (state in App.svelte)
- [ ] Each hint clickable — applies the suggested goto

### B.3 — Settings page
- [ ] `apps/desktop/src/lib/SettingsPanel.svelte`
- [ ] Bundle selector + "Reload" button (calls A.6)
- [ ] League dropdown (populated from poe2scout `/Leagues`)
- [ ] Risk slider persistence to `settings.toml`
- [ ] Refresh-prices auto-interval (off / 5min / 30min / 1hr)
- [ ] Plugin enable/disable list (links to F.6)

### B.4 — Recipe library
- [ ] CRUD Tauri commands for `~/.config/poc2/recipes/*.toml`
- [ ] `apps/desktop/src/lib/RecipeLibrary.svelte`
- [ ] Save current `(item, goal)` as named recipe
- [ ] Load recipe into builder
- [ ] Share via copy-to-clipboard of TOML text

---

## Phase C — Advisor sophistication

### C.1 — Monte Carlo aggregator
- [ ] `BeamConfig::mc_samples: u32` (default 50)
- [ ] Per-candidate run N samples; aggregate `(mean_prob, var_prob, mean_cost)`
- [ ] `Recommendation::expected_prob: f64` becomes mean ± stderr (add `prob_stderr` field)
- [ ] Frontend renders "P(reach) ≈ 65% ± 8%"
- [ ] Bench updated; depth-3 with 50 MC samples target ≤ 5 ms

### C.2 — Streaming recommendations
- [ ] `crates/advisor/src/lib.rs::plan_streaming(input, sender) -> JoinHandle`
- [ ] Tokio mpsc channel
- [ ] Emit at depth=1 (≤ 200ms), depth=3, depth=8
- [ ] Cancellable on new `recommend` call
- [ ] Tauri event channel for frontend subscription

### C.3 — Simulation runner UI
- [ ] `apps/desktop/src/lib/SimulationRunner.svelte`
- [ ] `run_n_trials(action, n_trials)` Tauri command
- [ ] Histogram via lightweight SVG (no heavy charting dep)
- [ ] Cost + probability distribution charts side by side

---

## Phase D — Live integration

### D.1 — Client.txt watcher
- [ ] `apps/desktop/src-tauri/src/client_log.rs`
- [ ] `notify` crate (inotify backend)
- [ ] Configurable Wine prefix path in Settings
- [ ] Detect: area changes, item drops, death events
- [ ] Tauri event emitter on each event

### D.2 — Always-on-top via Hyprland
- [ ] ADR-0009 — defer real layer-shell to v1.1
- [ ] `flake.nix` example Hyprland config (windowrulev2 = float, pin)
- [ ] `docs/41-ui-flows.md` "Hyprland integration" section
- [ ] Verify `wl-clipboard` works for clipboard reads

### D.3 — Trade search adapter (URL-only)
- [ ] `apps/desktop/src-tauri/src/trade_search.rs`
- [ ] Construct `pathofexile.com/trade2/search/<league>/...` URL from current item
- [ ] `trade_search(item)` Tauri command opens URL in default browser via `tauri-plugin-shell`
- [ ] Frontend "Search trade" button on advisor panel

---

## Phase E — Market enrichment

### E.1 — poe.ninja PoE2 meta-build aggregator
- [ ] Investigate poe.ninja PoE2 builds endpoint
- [ ] `crates/market/src/meta.rs` polls endpoint, caches locally
- [ ] Emits `MetaBuild { id, name, popularity, key_mods, base_choices }`
- [ ] `Tauri::fetch_meta_builds()` command

### E.2 — Off-meta finder
- [ ] `crates/market/src/meta::off_meta(builds, prices)` rank niche crafting goals
- [ ] Surfaces in Settings as "What to craft right now" hint

---

## Phase F — Wasm Plugin SDK (full)

### F.1 — ADR-0008 v2
- [ ] Rewrite ADR to "Plugin SDK shipped in v1"
- [ ] Document capability set, wasmtime+ComponentModel choice, perf contract,
      security model

### F.2 — poc2-plugin-host crate
- [ ] New workspace member `crates/plugin-host`
- [ ] wasmtime Engine + Store with capability-gated linker
- [ ] WIT bindings (Component Model)
- [ ] Plugin manifest TOML (`capabilities = [...]`, `entry_point = "..."`)
- [ ] Loader: `discover_plugins(dir) -> Vec<LoadedPlugin>`
- [ ] Per-plugin sandbox: memory limits, fuel budget for execution

### F.3 — Custom predicate dispatch
- [ ] `ItemPredicate::Custom { plugin_id: PluginId, name: String, args: serde_json::Value }`
- [ ] Predicate evaluator detects Custom and dispatches via host
- [ ] Cache: `(item canonical hash, plugin_id, name, args hash) → bool`
- [ ] Perf benchmark: < 1 ms per beam-search expansion with 10 plugins active

### F.4 — Recommendation emitter callbacks
- [ ] `RecommendationSource::Plugin { plugin_id, name }`
- [ ] Candidate generator calls each enabled plugin's `emit_recommendations(state) -> Vec<PluginCandidate>`
- [ ] Per-plugin per-state hard timeout (1 ms); auto-disable on exceed

### F.5 — poc2-plugin-sdk crate
- [ ] Guest-side helpers
- [ ] Macros: `declare_strategy!`, `declare_rule!`, `declare_predicate!`
- [ ] Type re-exports for engine types over wasm boundary

### F.6 — Plugin browser UI
- [ ] `apps/desktop/src/lib/PluginManager.svelte`
- [ ] Lists discovered plugins + requested capabilities
- [ ] Enable/disable toggle
- [ ] Capability review at install ("This plugin wants to: read engine state, emit recommendations…")
- [ ] Manage `~/.config/poc2/plugins/`

### F.7 — Example plugins
- [ ] `examples/plugins/strategy-alt-3xt1-es/` (alternate strategy emitter)
- [ ] `examples/plugins/rule-budget-watcher/` (active recommendation emitter using `CostSpent`)
- [ ] `examples/plugins/predicate-meta-build-match/` (custom predicate that matches build popularity)

### F.8 — Plugin host tests
- [ ] Integration test: load each example plugin → verify outputs
- [ ] Capability denial test: plugin without `emit_recommendations` cap can't call that import
- [ ] Sandbox test: malicious plugin can't break out of memory limit
- [ ] Perf test: 10 plugins × depth-3 beam stays < 50 ms

---

## Phase G — Polish + release

### G.1 — Performance pass + memoization
- [ ] Re-run benches after MC + plugins land
- [ ] Beam-search memoization: canonicalize `Item` by tier-set (drop Divine values)
- [ ] Target: depth-8 plan ≤ 50 ms with 10 plugins enabled

### G.2 — README + demo + v1.0 tag
- [ ] Public README with `nix run github:anomalyco/poc2` instructions
- [ ] Demo screencast: clipboard import → advisor recommendation → trade search
- [ ] CHANGELOG.md
- [ ] v1.0 git tag

---

## Risks tracked

1. **wasmtime Component Model maturity** — pin a specific wasmtime version
   + lock the `wit-bindgen` contract early (Phase F.1)
2. **Predicate-context refactor (A.1)** — touches every consumer of `eval`.
   Big mechanical diff but low semantic risk; existing predicates keep
   working
3. **Plugin recommendation callback in hot path (F.4)** — capability
   misuse could blow the perf budget. Mitigation: hard timeout per-plugin
   per-state (~1 ms); plugins exceeding timeout get auto-disabled
4. **Full-coverage rule encoding** — some rules (section 8 Budget,
   section 11 Market) reference data we have to invent. Encoded as
   `Guidance` rules with `Always` predicates where the engine can't
   verify the trigger; surfaced in UI as "tips"
5. **DSL action extensions backward compat** — existing 8 strategies
   must keep loading after we add new action variants. Default-deserialize
   behavior must hold (A.2 tests this)
