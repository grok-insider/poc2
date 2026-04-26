//! Recovery branching.
//!
//! When a step fails (engine returns an error or the post-state is
//! "worse than before"), the advisor surfaces *recovery hints* — the
//! [`RecoveryHint`](poc2_strategies::RecoveryHint) entries from the
//! strategy step plus advisor-generated alternatives.
//!
//! v1 implementation is intentionally thin: we surface the strategy's
//! hand-authored hints as-is. M4 follow-up adds advisor-generated
//! recoveries (e.g., "Annul + Aug back to a usable Magic" when a
//! Regal failed).

use poc2_strategies::RecoveryHint;

/// Collect recovery hints from a strategy step.
#[must_use]
pub fn collect_strategy_hints(step: &poc2_strategies::Step) -> Vec<RecoveryHint> {
    step.recovery.iter().cloned().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use poc2_engine::ids::CurrencyId;
    use poc2_strategies::{Action, Step, StepId};
    use smallvec::smallvec;

    #[test]
    fn collects_authored_hints() {
        let step = Step {
            id: StepId::from("S1"),
            action: Action::ApplyCurrency {
                currency: CurrencyId::from("ChaosOrb"),
                omens: vec![],
            },
            target_check: None,
            on_success: None,
            on_failure: None,
            recovery: smallvec![
                RecoveryHint {
                    message: "Try X".into(),
                    goto: None,
                    added_cost_div: Some(5),
                },
                RecoveryHint {
                    message: "Try Y".into(),
                    goto: None,
                    added_cost_div: Some(10),
                },
            ],
            note: None,
        };
        let hints = collect_strategy_hints(&step);
        assert_eq!(hints.len(), 2);
    }
}
