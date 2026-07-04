# ADR-0015 — Analytic transition models + on-demand goal solving

- Status: **Accepted** (2026-07-04)
- Relates to: [ADR-0007](0007-advisor-beam-search.md) (the beam-search
  planner the trained policy uplifts — unchanged),
  `docs/81-engine-training-and-rule-encoding-plan.md` (the v3 training
  design this supersedes in part).

## Context

The v3 trained-policy pipeline (docs/81, Britz-style) learned the
per-action transition model `P(s' | s, a)` by **Monte Carlo sampling**:
100 000 simulator rollouts per `(state, action)` alias, ≈ 2.5×10¹⁰
`simulate()` calls ≈ 7 hours per full-corpus training run. The trained
artefact only covered the **51-goal curated corpus**, keyed by a
`goal_hash` that included the budget — any user goal outside the corpus
(or any budget tweak) silently fell back to heuristic planning.

Two observations dissolve both costs:

1. **The model is known.** Britz sampled because he was outside the
   game. We own the engine: every distribution the simulator draws from
   is held in closed form — slot picks are a fair coin iff both sides
   are open, mod picks are a weighted categorical over the eligibility
   pool (`enumerate_eligible_mods`, the sampling-identical pool builder
   factored out of `sample_eligible_mod`), removals are uniform over
   non-fractured mods, and value rolls never reach `FeatureVec`. The
   exact categorical can be **constructed** instead of estimated.
2. **Exact construction is fast enough to run at plan time.** Building
   the analytic model + two Bellman solves takes well under a second per
   goal at a bounded state budget — comfortably inside the engine
   worker's `recommend` call, which already runs off the UI thread.

## Decision

1. **Analytic transition construction is the production trainer.**
   `training::analytic_model` builds exact distributions for the
   basic-orb families (Transmute/Augment/Regal/Exalt all tiers; Chaos
   remove-one-add-one with removal×add convolution; Annul uniform;
   Divine = feature-space identity), with engine-atomicity-faithful
   failure self-loops and a Monte Carlo fallback for actions without a
   closed form (essences, omen-conditioned applies). The historical MC
   learner survives as `train-advisor --model mc`, and the per-item
   cross-validation test (`analytic_cross_validates_against_monte_carlo_per_item`)
   pins the two paths against each other — a disagreement is a bug in
   one of them.
2. **One shared solver, two call sites.** `training::solve::solve_goal`
   (analytic model → path-length + cost value iteration → packaged
   `TrainedModel` pair) is the single recipe. `train-advisor` calls it
   at the offline budget for the corpus precompute; the **WASM engine
   calls it on demand** at `SolveProfile::on_demand` (2 000-state BFS
   cap) whenever a `recommend` misses the trained-model cache for the
   user's `(goal, item-class)` — or hits a cached model that doesn't
   cover the current item's featurized state (solved from a different
   starting point), which triggers a re-solve from the new root.
3. **The corpus becomes a warm start, not a boundary.** The optional
   `/trained-models.json` artefact still preloads the curated goals
   (operator asset, never committed), but *every* goal now gets an exact
   policy the first time it is planned. `Engine::recommend` is `&mut
   self` (the worker is single-threaded); the ⚛ topbar chip reflects
   live cache growth after each plan.
4. **Invalidation.** `setLeague` (league gates the candidate set) and
   `setPluginDispatch`/`clearPluginDispatch` (custom predicates can sit
   in goal criteria) clear the whole trained cache; the next recommend
   re-solves on demand. The cache holds at most 256 `(goal × class)`
   entries and is cleared wholesale past the cap (a solve is sub-second;
   LRU bookkeeping isn't worth it).

## Consequences

- The "production-scale retrain" operator task shrank from ~7 h to
  ~30 s, and stopped being release-blocking: the shipped artefact is a
  warm start only.
- Trained-policy coverage extends to arbitrary user goals, budgets
  excluded from `goal_hash` (artefact schema v2), and mid-craft starting
  items (coverage-triggered re-solves).
- The first `recommend` for a fresh goal pays the solve inside the
  worker (typically well under a second; the UI's existing `planning`
  state covers it). Superseded plans are token-discarded, not cancelled
  — unchanged from the previous behavior.
- File-loaded artefacts are dropped on league/plugin-dispatch changes
  rather than re-fetched; on-demand solving recomputes what's actually
  used. (The worker could re-send the asset on demand later if the warm
  start proves valuable across switches.)
- Deep-RL / GPU training was evaluated and rejected: the state space is
  capped (~10³–10⁴ per goal by design), the model is known, and the
  bottleneck was never neural-net math — tabular exact solving is both
  more accurate and cheaper. MCTS-at-plan-time remains future work if
  goals ever outgrow the BFS budget.

## Verification

- `crates/advisor/src/training/analytic_model.rs` — per-item
  analytic-vs-MC cross-validation, exact-weight pins, floor +
  keep-≥1-tier exception pin, determinism, count-goal reachability.
- `crates/advisor/src/training/solve.rs` — cache-ready pair, determinism,
  terminal predicate.
- `crates/poc2-wasm/tests/on_demand_solve.rs` — on-demand solve +
  cache reuse, budget-insensitive keying, coverage re-solve, league
  invalidation, empty-target no-op.
