# Recovery Flows (M3 + M4)

> Companion to [`33-strategy-library.md`](33-strategy-library.md) and
> [`34-heuristics-rulebook.md`](34-heuristics-rulebook.md). Documents
> the recovery branching encoded in seed strategies and rules.
>
> Recovery is a first-class concept in the engine: every strategy step
> may carry a `recovery: SmallVec<[RecoveryHint; 3]>` field, and §12 of
> /docs/34 codifies the canonical step-failure responses as standalone
> rules (R460-R469).

## Recovery DAG Shape

A `RecoveryHint` is:

```rust
pub struct RecoveryHint {
    pub message: String,            // human-readable explanation
    pub goto: Option<StepId>,       // where to jump to if accepted
    pub added_cost_div: Option<u32>, // estimated cost of the recovery itself
}
```

Recovery hints attach to a specific step. When the engine reports a
step failure (`apply_currency` returned an error, or post-action
target_check failed), the advisor surfaces the step's recovery hints
as alternatives the user can accept or decline.

## Canonical 3-Deep Examples

### Example 1: Triple T1 ES Body Armour (`3xt1-es-body-armour.toml`)

Step S7b (Fracture) recovery DAG:

```
S7b-fracture
   ├─ on_success → S8-reveal
   ├─ on_failure → S7c-fracture-fail (Abandon)
   ├─ recovery hint #1
   │     "Continue with whichever mod was fractured;
   │      the craft can still finish at lower tier."
   │     goto = S8-reveal, added_cost_div = 0
   └─ recovery hint #2
         "Hinekora's Lock can preview before commit."
         added_cost_div = 5
```

Three options surface to the user: hard-abandon (S7c), accept-the-loss
continue (hint #1), or pre-Lock the next attempt (hint #2). The
advisor ranks them by `expected_success_prob × (1 - added_cost_div /
budget)` and surfaces the top two as buttons in the RecoveryPanel UI
(Phase B.2).

### Example 2: Belton's Four-T1 Rubric (`beltons-four-t1-rubric.toml`)

Step S2 (Fracture) carries one recovery hint and one explicit on_failure:

```
S2-fracture
   ├─ on_success → S3-chaos-tag-match
   ├─ on_failure → S2b-bricked-fracture (Abandon)
   └─ recovery hint #1
         "Bricked fracture (locked the wrong mod):
          vendor for 0.5–2 div recovery; restart
          on a fresh 4-mod base."
         added_cost_div = 2
```

S3 (Chaos-tag-match) has a stop-loss recovery:

```
S3-chaos-tag-match
   ├─ on_success → S4-activate-necromancy
   ├─ on_failure → S3-chaos-tag-match (loop)
   └─ recovery hint #1
         "Stop chaos spam after ~30–50 div without hitting
          the second target. Sell as 2-mod fracture seed (#17)."
         added_cost_div = 0
```

S7 (Reveal) has a multi-pool fallback:

```
S7-reveal
   ├─ on_success → S8-exalt-fourth
   ├─ on_failure → S8-exalt-fourth     (continue with whatever revealed)
   └─ recovery hint #1
         "All 6 reveal options bad: pick least-junky and strip
          via Omen of Light + Annul (#9), then re-bone with
          a Lord omen (#15)."
         added_cost_div = 12
```

S8 (4th-mod Exalt) has the most elaborate recovery: a sub-loop
through S8b (side-locked Annul), plus extension hints for 5T1
upgrade:

```
S8-exalt-fourth
   ├─ on_success → S9-done
   ├─ on_failure → S8b-recover (Annul → re-Exalt)
   ├─ recovery hint #1
   │     "Bad 4th-mod slam: Sinistral/Dextral Annulment
   │      (50/50 to remove the trash) then retry the Exalt."
   │     added_cost_div = 8
   └─ recovery hint #2
         "Continue to 5T1 via Whittle (#7) — the lowest
          mod-level mod becomes the deterministic Whittle
          target."
         added_cost_div = 15
```

### Example 3: Bones with Abyssal Echoes (`bones-with-abyssal-echoes.toml`)

S4 (Reveal) has two recoveries — one shallow (light + annul cleanup)
and one switch-strategy:

```
S4-reveal
   ├─ on_success → S5-done
   ├─ on_failure → S5-done
   ├─ recovery hint #1
   │     "All 6 options bad: pick the least-junky, then
   │      strip via Omen of Light + Annul. Then re-bone."
   │     goto = S5-done, added_cost_div = 4
   └─ recovery hint #2
         "Out of Echoes omens: switch to an Abyss Lord omen
          (#15) for a Lord-pool guarantee."
         added_cost_div = 8
```

## Cross-Cutting Recovery Rules (§12)

Some failure modes don't attach cleanly to one strategy — they're
universal patterns. /docs/34 §12 codifies these as standalone rules
(R460-R469) which fire from the rule engine independent of any
strategy:

| Failure mode | Rule | Recovery action |
|---|---|---|
| Bricked fracture | R460 | Sell as fractured base |
| Bad reveal | R461 | Light + Annul, re-bone |
| Bad reveal, no Light | R462 | Side-targeted Annul |
| Vaal bricked | R463 | Salvage / partial-corrupt sell |
| Wrong essence mod | R464 | Re-essence with different family |
| Bad Annul | R465 | Re-Exalt with side-lock |
| Twice-corrupt destroyed | R466 | NO RECOVERY (warning) |
| Suffix full, need prefix | R467 | Sinistral Annul on fractured suffix |
| Sanctified bad roll | R468 | NO RECOVERY (warning) |
| Boots step-1 fail | R469 | Re-cycle Annul → Aug |

These fire whenever the relevant precondition holds, regardless of
which strategy the user is mid-execution on. The advisor treats them
as additional candidates in the beam-search frontier.

## Failure Detection

The engine reports step failure in two ways:

1. **Hard failure**: `apply_currency` returns `Err(EngineError)`. The
   currency couldn't be applied (wrong rarity / state / class). The
   simulator's `SimulationOutcome::success` field is `false`.
2. **Soft failure**: `apply_currency` returned `Ok(())` but the step's
   `target_check` predicate is not satisfied on the post-state. The
   strategy executor routes via `on_failure` rather than `on_success`.

Both surfaces feed into the advisor's `expand_with_candidate` — the
node's `accumulated_prob` is multiplied by either `0.95` (success) or
`0.05` (failure) under the v1 deterministic-RNG model. Phase C.1's
Monte Carlo aggregator replaces this binary signal with a real
estimate from N samples.

## Recovery Panel UI (Phase B.2)

The RecoveryPanel.svelte component is visible only when
`lastActionResult.success === false`. It displays:

```
┌────────────────────────────────────────────────┐
│ Last action failed: Fracturing Orb missed.    │
│                                                │
│ Recovery options                               │
│ ───────────────────                            │
│ ┌────────────────────────────────────────────┐ │
│ │ 1. Continue at lower tier      cost: free  │ │
│ │    Continue with whichever mod was         │ │
│ │    fractured; the craft can still finish   │ │
│ │    at lower tier.                          │ │
│ │ [Apply this recovery]                      │ │
│ └────────────────────────────────────────────┘ │
│                                                │
│ ┌────────────────────────────────────────────┐ │
│ │ 2. Pre-Lock next attempt       cost: 5d    │ │
│ │    Hinekora's Lock can preview before      │ │
│ │    commit.                                 │ │
│ │ [Apply this recovery]                      │ │
│ └────────────────────────────────────────────┘ │
└────────────────────────────────────────────────┘
```

Clicking "Apply this recovery" calls a new Tauri command
`apply_recovery_hint(strategy_id, step_id, hint_idx)` which advances
the executor to the hint's `goto` step and updates the cost tracker.

## Future Work

- **Plugin-emitted recoveries** (Phase F.4) — community plugins can
  register additional recovery flows for niche bases / build types.
- **Cost-aware recovery ranking** — current ranking is based on
  `added_cost_div` only; M5+ extends with full EV calculation against
  the partial-state value.
- **Multi-step recoveries** — currently a hint is a single goto;
  extending to short sub-sequences (e.g. "annul → aug → annul") would
  let the executor encode common 3-deep recoveries inline.
