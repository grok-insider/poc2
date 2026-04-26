# ADR-0007 — Advisor uses beam-search optimal-path planning

- Status: Accepted
- Date: 2026-04-26

## Context

Three sophistication tiers were considered:

1. **Rules + templates only** — predictable, fast, easy to explain. Like a forward-chained expert system.
2. **Rules + Monte Carlo ranking** — above + simulate top-N candidates and rank by EV.
3. **Beam-search optimal-path** — search multi-step sequences, like POE2HTC. Most ambitious.
4. **ML-trained advisor** — long-term R&D.

## Decision

**Beam-search optimal-path with full re-plan on every state change.**

Algorithm:

```
fn replan(state: Item, goal: Goal, budget: Budget, stash: Stash, patch: PatchVersion, risk: f32)
    -> Stream<Recommendation>
{
    let candidates = generate_candidates(state, stash, patch);
    //              ^ from rules (per-step) + strategies (multi-step templates)

    let beam = BeamSearch::new(width=5, depth=8, budget);
    for step_depth in 1..=depth {
        beam.expand(|node| simulate_outcomes(node, monte_carlo_n=200));
        beam.score(|node| utility(node, risk));
        beam.prune();
        emit_top_n(beam.top(3));    // streaming
    }
}
```

Performance budget per re-plan: **2 seconds for streaming first results**, **30 seconds for converged top-3**. New state arriving cancels in-flight search.

## Rationale

- The user explicitly asked for "smart enough to know how to craft properly using the best strategy". Rule-only is too brittle for the requested intelligence level.
- POE2HTC (AGPL) and pyoe2-craftpath (MIT) both prove beam search works well for this domain. We don't reuse their code, but the algorithm is in the public domain.
- The strategy library serves as a **prior** — strategies emit candidate sequences, the search refines them. This is more efficient than naive search over all currencies × omens.
- Full re-plan is the right call given the user's choice for "Full re-plan on every state change". Streaming top-N keeps UX snappy.

## What's hard about this

- **Branching factor** is huge. With ~30 currencies × ~16 omens × ~5 stash availability bits, naive search explodes.
  - Mitigation: rule-emitted candidates prune aggressively; only "plausible" actions enter the beam.
- **Probabilistic transitions** mean each "next state" is actually a distribution. We sample.
  - Mitigation: Monte Carlo with 200-1000 samples per node; only top-K sampled outcomes propagate.
- **Memoization** is tricky because items have continuous-valued mod rolls (Divine values).
  - Mitigation: canonicalize items by their tier set, not their exact roll values, for memoization keys.
- **User risk preference** affects the utility function. Cautious users want low-variance plans; greedy users tolerate variance for higher EV.
  - Mitigation: utility = `EV(success) - cost - λ * Var(cost)`, with `λ` from the risk slider.

## Streaming UI

The advisor doesn't block. The frontend subscribes to a `Recommendation` stream. As beam depth increases, refined recommendations replace earlier ones. The user sees:

- ~200ms: first result based on rule-emitted candidates only.
- ~2s: top-3 from depth-3 beam.
- ~10s: top-3 from depth-8 beam.
- Convergence: "this is the best we found".

Cancellation: any state change (clipboard event, manual edit) cancels the in-flight search; a new one starts.

## Open questions for M4

- Beam width / depth defaults — to be tuned empirically.
- Memoization key canonicalization — exact form TBD.
- How to surface "we didn't fully explore" to the user without overwhelming them.

## Alternatives reconsidered

- **Rules-only**: rejected because the user asked for cross-strategy optimization. Rules give one-step recommendations only.
- **A* / IDA***: considered, rejected for v1. Beam search has predictable runtime; A* can stall on bad heuristics.
- **MCTS**: tempting (handles probabilistic transitions natively). Deferred to v2 R&D as a possible upgrade.
