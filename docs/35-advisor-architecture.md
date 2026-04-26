# Advisor Architecture (M4)

> Companion to [ADR-0007](adr/0007-advisor-beam-search.md). Describes the
> implementation in `crates/advisor/`.

## Overview

The advisor is a beam-search optimal-path planner. Given an
`(Item, Goal, Stash, Risk)` quadruple, it returns a ranked
`Vec<Recommendation>` describing the next action the user should take,
with full traceability back to the source rule, strategy, or heuristic.

## Inputs

| Input | Type | Source |
|-------|------|--------|
| Item | `poc2_engine::Item` | clipboard parser, item builder, or saved fixture |
| Goal | `poc2_advisor::Goal` | UI target panel (`Target` + abandon predicates + budget) |
| Stash | `poc2_advisor::Stash` | UI stash inventory (or `Stash::unlimited()` for what-if) |
| Patch | `poc2_engine::PatchVersion` | bundle's `header.game_patch` |
| Rules | `poc2_rules::RuleSet` | seed catalogue + user TOML files |
| Strategies | `poc2_strategies::StrategyRegistry` | bundle + `~/.config/poc2/strategies/` |
| Valuator | `poc2_market::Valuator` | conservative defaults (M5.3 swaps in live prices) |
| Risk | `f64` in `[0, 1]` | UI slider; 0 = cautious, 1 = greedy |

## Output

```rust
pub struct Recommendation {
    pub action: AdvisorAction,
    pub source: RecommendationSource,
    pub expected_cost: DivEquiv,
    pub expected_prob: f64,
    pub score: f64,
    pub rationale: String,
    pub depth: u32,
}
```

`AdvisorAction` is a unified enum spanning currency apply, Hinekora's
Lock, Reveal, Stop, Abandon, Guidance — folded from
`poc2_strategies::Action` and `poc2_rules::SuggestionAction`.

`RecommendationSource` carries the originating rule id, strategy id +
step, or heuristic name so the UI can show "rule R001 (verified)" or
"strategy `apprentice-blueprint` :: S2-augment".

## Algorithm

```text
                  ┌──── Stash ────┐
                  │   filters     │
                  ▼               │
   ┌─────────┐  ┌──────────┐  ┌────────┐
   │ Rules   │─▶│Candidate │  │  Item  │
   │ Strat.  │  │Generator │  │ State  │
   │ Heurist.│  └─────┬────┘  └────────┘
   └─────────┘        │
                      ▼
                ┌──────────┐
                │Simulator │ ─── one engine.apply() per candidate
                └─────┬────┘
                      ▼
                ┌──────────┐
                │ Scorer   │ ─── utility = success_prob - λ*cost - μ*Var
                └─────┬────┘
                      ▼
                ┌──────────┐
                │ Beam     │ ─── width=W, depth=D, group by first action
                │ Prune    │
                └─────┬────┘
                      ▼
                Vec<Recommendation>
```

### 1. Candidate Generation (`crates/advisor/src/candidate.rs`)

Three parallel sources contribute candidates for a state:

1. **Rules**: `poc2_rules::evaluate(ruleset, item, registry)` returns
   one suggestion per matching rule. Each suggestion is lifted to an
   `AdvisorAction` via `from_rule_action`.
2. **Strategies**: every strategy whose preconditions match the item
   contributes its entry-step action via `from_strategy_action`.
   Multi-step lookahead happens implicitly when the planner re-runs
   the generator at deeper beam depths against the simulated child
   state.
3. **Heuristics**: a small fallback set ("Normal item → Transmute",
   "Magic with empty slot → Augment") so the advisor never returns
   empty when both rules and strategies fall silent.

Candidates are filtered by `Stash::can_afford` (currency + every
required omen owned), then deduplicated by exact action equality with
priority-tie-breaking.

### 2. Simulator (`crates/advisor/src/simulator.rs`)

`simulate(item, action, omens, registry, resolver, patch, seed)` runs
the engine's `apply_currency` once with a deterministic RNG seed and
returns:

```rust
pub struct SimulationOutcome {
    pub item: Item,        // post-apply state
    pub success: bool,     // engine.apply returned Ok?
    pub error: Option<String>,
    pub change_count: u32, // how many state diffs the apply caused
}
```

Determinism matters because the planner replays the same seed across
benchmarks and tests. Real Monte Carlo (multiple samples per candidate)
is M5.x work.

### 3. Scorer (`crates/advisor/src/scorer.rs`)

```text
utility(node, risk, weights)
    = success_prob
    - λ * cost.risk_adjusted(1 - risk)
    - μ * (cost.max - cost.min)
    + prior_weight * source_prior
```

- `success_prob` is the planner's estimate of reaching the goal from
  the post-action state. Today it's `joint_step_prob × partial_progress`;
  `partial_progress` is the fraction of `Goal::target` specs satisfied
  by the post-state (concept-aware, hybrid-aware).
- `cost.risk_adjusted(1 - risk)` lerps between the band's expected and
  max bounds. A cautious user (`risk=0`) gets the worst-case cost; a
  greedy user (`risk=1`) gets the optimistic cost.
- `λ` (default 1.0) weights cost vs probability.
- `μ` (default 0.05) penalizes wide cost bands (uncertainty premium).
- `source_prior` is `0.9 / 0.7 / 0.5` for rule confidence
  Verified / Community / Experimental.

### 4. Planner (`crates/advisor/src/planner.rs`)

Beam search with configurable `width × depth × top_n`:

```rust
pub struct BeamConfig {
    pub width: u32,    // frontier size at each depth
    pub depth: u32,    // expansion rounds (1 = single-step)
    pub risk: f64,
    pub top_n: u32,    // recommendations returned
    pub seed: u64,
    pub weights: ScoringWeights,
}
```

Defaults: `width=5, depth=3, top_n=3, risk=0.5`.

The planner short-circuits when the goal is already met at root, and
terminates beam nodes that hit `Goal::abandon_criteria`. After the
configured depth, frontier nodes are grouped by their **first action**
and the highest-scoring node per group becomes the
`Recommendation` for that action — this gives the user one rec per
distinct first move rather than `width` near-identical recs.

## Performance

Per-bench (i7-class laptop):

| Operation | Time |
|-----------|------|
| Single `apply_currency` (basic orb) | 244-563 ns |
| Single `apply_currency` via trait object | 293 ns |
| `plan_depth_1_top_3` (rules-only fast path) | 1.35 µs |
| `plan_depth_3_top_3` (full beam) | 3.08 µs |
| `plan_depth_5_width_8` (stress) | 4.95 µs |

These are 4-5 orders of magnitude under the ADR-0007 budget of 2s for
streaming first results. Even at depth=8 width=5 with 200 MC samples
per candidate, the math says we're well inside the budget.

## Critical Test: Canonical Rediscovery

Per [ADR-0007](adr/0007-advisor-beam-search.md) and the M4 roadmap, the
advisor must produce the user's worked-example strategy (or strictly
better). The integration test
`crates/advisor/tests/canonical_rediscovery.rs` asserts:

1. Top recommendation for a fresh Normal ilvl 82 BodyArmour is a
   `PerfectOrbOfTransmutation` apply
2. The recommendation traces to either rule R001 or strategy
   `3xt1-es-body-armour-isolation` step S2
3. Every recommendation cites a non-empty source + rationale
4. Risk slider monotonically reorders scores
5. Already-satisfied goals short-circuit to `Stop`

All five tests pass. The advisor is functionally correct for the
canonical user worked example.

## Future Work

- **Monte Carlo aggregation**: average outcomes over N samples per
  candidate instead of single-sample. M5.x.
- **Streaming results**: emit recommendations as the beam deepens via
  Tokio channels so the UI shows depth-1 results immediately and
  refines them with depth-3 / depth-8 over the next few seconds.
- **Memoization**: canonicalize items by tier-set rather than exact
  roll values to share simulator results across beam siblings.
- **MCTS upgrade**: probabilistic transitions handled natively. Deferred
  to v2 R&D per ADR-0007.
- **Richer Goal types**: tier weighting beyond min_tier; budget
  allocation across phases of a multi-step plan.
