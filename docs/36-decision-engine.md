# Decision Engine (M3 + M4)

> Companion to [`33-strategy-library.md`](33-strategy-library.md),
> [`34-heuristics-rulebook.md`](34-heuristics-rulebook.md), and
> [`35-advisor-architecture.md`](35-advisor-architecture.md). Documents
> the production-rule synthesis and ranking pipeline.

## Pipeline Overview

```
┌──────────────┐
│ Item state   │
│ + Goal       │ ──┐
│ + Stash      │   │
│ + Patch      │   │      ┌───────────────────────┐
└──────────────┘   ├─────▶│ PredicateContext      │
                   │      │   registry            │
┌──────────────┐   │      │   cost_so_far_div     │
│ Valuator     │   │      │   valuator (opt)      │
│ (live prices)│ ──┤      │   stash    (opt)      │
└──────────────┘   │      │   expected_sale_price │
                   │      └──────────┬────────────┘
┌──────────────┐   │                 │
│ RuleSet      │ ──┤                 │
│ (113 rules)  │   │                 ▼
└──────────────┘   │      ┌──────────────────────┐
                   │      │ Forward-chain        │
┌──────────────┐   │      │ rule engine          │
│ StrategyReg. │ ──┤      │  • eval each rule    │
│ (23 strats.) │   │      │  • emit suggestion   │
└──────────────┘   │      │  • sort by priority  │
                   │      └──────────┬───────────┘
                   │                 │
                   │                 ▼
                   │      ┌──────────────────────┐
                   │      │ Strategy preconds.   │
                   │      │  • per-class scope   │
                   │      │  • patch-version OK  │
                   │      │  • emit entry action │
                   │      └──────────┬───────────┘
                   │                 │
                   │                 ▼
                   │      ┌──────────────────────┐
                   └─────▶│ Heuristic fallback   │
                          │  (only if both       │
                          │   above empty)       │
                          └──────────┬───────────┘
                                     │
                                     ▼
                          ┌──────────────────────┐
                          │ Stash filter         │
                          │  (drop unaffordable) │
                          └──────────┬───────────┘
                                     │
                                     ▼
                          ┌──────────────────────┐
                          │ Beam search          │
                          │  (depth × width)     │
                          └──────────┬───────────┘
                                     │
                                     ▼
                          ┌──────────────────────┐
                          │ Score + group by     │
                          │  first action        │
                          └──────────┬───────────┘
                                     │
                                     ▼
                          Vec<Recommendation>
```

## Production-Rule Synthesis

Each rule in the catalogue follows the IF-THEN-BECAUSE-SOURCE shape
defined in /docs/34. In code:

```rust
pub struct Rule {
    pub id: RuleId,                      // R001-perfect-transmute-on-normal
    pub category: Category,              // base_selection | fracture | ...
    pub when: ItemPredicate,             // IF clause
    pub then: SmallVec<[Suggestion; 4]>, // THEN actions
    pub explanation: String,             // BECAUSE rationale
    pub source: String,                  // SOURCE attribution
    pub confidence: Confidence,          // Verified / Community / Experimental
}
```

The IF clause uses the same `ItemPredicate` enum as the strategy DSL
(see /docs/30-domain-model.md). All predicates from M3 + A.1 are
available:

| Predicate | Notes |
|---|---|
| `Always` / `Never` | universal triggers |
| `Ilvl(ValuePredicate)` | item-level constraint |
| `Rarity(Rarity)` | exact rarity |
| `Corrupted(bool)` / `Sanctified(bool)` / `Mirrored(bool)` | state flags |
| `ItemClass(ItemClassId)` / `ItemClassAny(Vec<...>)` | class scoping |
| `AttributePool(...)` / `AttributePoolAny(...)` | attribute-pool match |
| `AffixCount { affix, count }` | per-slot count |
| `ModCount(ValuePredicate)` | total prefix+suffix count |
| `Quality(ValuePredicate)` | item quality |
| `HasConcept { concept, affix?, min_tier? }` | concept-aware (hybrid OK) |
| `HasFractured(bool)` / `HasHiddenDesecrated(bool)` / `HasDesecratedRevealed(bool)` / `HasHinekoraLock(bool)` | special-state checks |
| `StashHas { currency, count }` | stash inventory (A.1) |
| `CostSpent(FloatValuePredicate)` | budget tracking (A.1) |
| `ExpectedSalePrice(FloatValuePredicate)` | market value (A.1) |
| `All(...)` / `Any(...)` / `Not(...)` | logical composition |

## Rule-Priority Arithmetic

Each `Suggestion` carries a `priority: u32` (default 100). The rule
engine sorts results by priority descending, ties broken by rule
insertion order. Convention used in the seed catalogue:

| Range | Meaning |
|---|---|
| 250+ | Mandatory pre-action (Hinekora's Lock before fracture, etc.) |
| 200-249 | Strong actionable preference (Erasure, Whittle on locked sides, ...) |
| 100-199 | Routine suggestion (catalysts, default progressions) |
| 50-99  | Weak guidance (Vaal warnings, conditional EV checks) |
| 30-49  | Pure tip (no immediate action) |
| 0-29   | League-wide / metalevel advice (Tarke bankroll, market cycle) |

## Tagged Guidance (A.5)

Some suggestions carry a `tag: Option<String>` field to mark them as
non-actionable for the advisor's ranking pipeline:

| Tag | UI treatment |
|---|---|
| `"warning"` | High-stakes caution; surfaced prominently in red |
| `"meta"` | EV / confidence / strategy theory; surfaced as a tooltip or help text |
| `"league_advice"` | Time-of-league / market-state / bankroll discipline; surfaced in Settings as a tip card, not in the top-N list |
| `None` | Regular actionable recommendation, ranked by score |

The Settings panel (Phase B.3) renders `league_advice`-tagged
suggestions as a "Tips" sidebar separate from the advisor's main
recommendation list.

## Strategy Synthesis

Strategies (codified in /docs/33) are multi-step recipes loaded from
TOML. The candidate generator pulls the entry-step action from every
strategy whose preconditions match the current item state and whose
patch range covers the active patch. Lookahead happens implicitly via
the planner's beam-search depth: at depth N the generator runs against
the *simulated* post-state of depth N-1.

## Conflict Resolution

When a rule and a strategy emit the same `(currency, omens)` pair, the
candidate deduplicator keeps the higher-priority entry. Tie-breaker
order:

1. Higher `Suggestion::priority` (rules) or strategy's `expected_success_prob.hi * 255`.
2. Higher source confidence (`Verified > Community > Experimental`).
3. Insertion order.

## Tracing & Explainability

Every recommendation cites its origin via
`Recommendation::source`. The UI (`AdvisorPanel.svelte`) renders this
as `rule R001 (verified)` or `strategy 3xt1-es-body-armour-isolation
:: S2-perfect-transmute`. The `Suggestion::note` /
`strategy.name` strings populate `Recommendation::rationale`,
displayed beneath the action.

## Future Work

- **Rule-confidence-conditional weights** — the planner could downweight
  Experimental-confidence rules' contributions to the beam-search prior.
- **Plugin-emitted rules** (Phase F.4) — the rule engine currently
  ingests only embedded TOML; plugins will register custom rules at
  runtime via the Wasm Component Model.
- **Conflict warnings** — when two high-priority rules suggest
  incompatible actions (e.g. one says Lock + Fracture, the other says
  Annul), surface that as an explicit note in the UI rather than
  silently picking the highest-scored.
