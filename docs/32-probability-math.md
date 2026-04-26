# Probability Math (M5)

> Companion to [`30-domain-model.md`](30-domain-model.md) and
> [`35-advisor-architecture.md`](35-advisor-architecture.md). Documents
> the probability primitives the advisor uses for cost / variance / EV
> calculations.

## Geometric Distribution — "How many tries until X?"

For a single-trial success probability `p`, the number of attempts
until the first success is geometrically distributed:

```
P(N = k) = (1 - p)^(k - 1) * p     for k = 1, 2, 3, ...
E[N]     = 1/p
Var[N]   = (1 - p) / p^2
```

The advisor uses these for chaos-spam, slam-spam, and any "keep trying
until hit" loop where each attempt is independent.

The **median** number of attempts (the 50%-confidence threshold) is:

```
N_50  =  ceil( ln(0.5) / ln(1 - p) )
       ~  0.69 / p     (for small p, by Taylor expansion)
```

This is what the UI's "P(reach) ≈ 65%" tooltip is computed from.

### Worked example

Belton's four-T1 rubric (#19) targets the rarest of 3 desired same-side
mods on a 4-mod base. Per Belton's mod-weight tables, T1 prefix lock
hits at ~25%–33% (1-in-3 or 1-in-4 depending on the desecrated-mod
pre-load trick #2.7). At p = 1/4:

```
E[N]   = 4 attempts
N_50  ≈ 2.4 attempts (round to 3)
N_90  ≈ 8 attempts
```

So a typical 4T1 craft expects 4 fracture attempts; budgeting for 8
covers 90% of real-world runs.

## Monte Carlo Aggregation (Phase C.1)

Some craft outcomes are not single-trial geometric — Recombinator,
multi-mod Exalts (Greater Exaltation), and Vaal-then-revisit chains
have correlated draws across steps. For these the advisor runs N
Monte Carlo simulations per candidate (default `mc_samples = 50`)
and reports `(mean_prob, var_prob, mean_cost)`.

The mean is reported as `Recommendation::expected_prob`; the
standard error of the mean is reported as `Recommendation::prob_stderr`
(added in C.1). UI render: `P(reach) ≈ 65% ± 8%`.

### Convergence guarantees

For a true probability `p` and `N` independent samples:

```
SE(mean)  =  sqrt( p * (1 - p) / N )
```

At `N = 50` and `p = 0.5` (worst-case variance), `SE ≈ 0.07`. This is
under 10 percentage points — sufficient for ranking-quality decisions.
For finer estimates the user can crank `mc_samples` via the
SimulationRunner UI (Phase C.3).

## Wilson Score Interval (Streaming Probability Updates)

The streaming planner (Phase C.2) emits intermediate results before
all `mc_samples` are in. The early-stage probability estimate uses the
**Wilson score interval** for stable confidence bounds on small-N
proportion estimates:

```
p_hat        = k / n
center       = (p_hat + z² / (2n)) / (1 + z² / n)
margin       = z * sqrt( (p_hat * (1 - p_hat) + z² / (4n)) / n ) / (1 + z² / n)
```

At z = 1.96 (95% CI) and n = 5, the interval is ~±35 pp around p_hat.
By n = 20 it shrinks to ~±20 pp. The streaming UI shows these bounds
shrinking as the trace progresses.

## Risk-Adjusted Cost Bands

`DivEquiv { min, expected, max }` triples encode pessimistic /
expected / optimistic cost. The risk slider lerps between them:

```
risk_adjusted(c, risk) = c.expected + risk * (c.max - c.expected)
                                                                       (cautious bias)
```

The advisor's scorer inverts this so a "greedy" user (risk=1) gets the
*lowest* cost (expected); a cautious user (risk=0) gets the
*highest* (max). See [`crates/advisor/src/scorer.rs`](../crates/advisor/src/scorer.rs).

## Variance Penalty

The scorer also subtracts `mu * (cost.max - cost.min)` from the
utility — penalizing wide cost bands so that two recommendations with
the same mean cost but different variance rank in favor of the
narrower-band one.

Default `mu = 0.05`. Boosting it makes the advisor more conservative
about cost uncertainty.

## EV Stopping Rule (per /docs/34 §13.3)

Standard rule:

```
if 2 * expected_attempt_cost > target_value: STOP
```

The advisor surfaces this as rule R482 (tagged `meta`) and applies it
in `Goal::abandon_criteria` when the user provides a budget that
encodes this directly via `CostSpent` predicates.

## References

- Belton, "Figure Out the Odds of ANY Craft - Modifier Weights Explained" — total weight / target weight = expected attempts.
- Reddit r/pathofexile, "The mathematical way to choose when to stop crafting" — geometric stopping rules.
- Wikipedia, [Wilson score interval](https://en.wikipedia.org/wiki/Binomial_proportion_confidence_interval#Wilson_score_interval).
