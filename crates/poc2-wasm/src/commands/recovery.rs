//! `recovery` command — recovery hints for a strategy step (Phase B.2).
//!
//! Ported from the Tauri desktop `recovery_hints` command. Pure compute over
//! the [`StrategyRegistry`]: given a strategy id + step id, surface the step's
//! hand-authored recovery hints and a summary of the next action that would run
//! on failure. No clipboard / app-handle / disk concerns.

use poc2_strategies::{Action, StepId, StrategyId, StrategyRegistry};
use serde::Serialize;

/// One recovery option attached to a step.
#[derive(Debug, Serialize)]
pub struct RecoveryHintView {
    /// Human-readable explanation of the recovery option.
    pub message: String,
    /// Step id the user would jump to if they accept this hint
    /// (None when the hint is purely advisory).
    pub goto_step_id: Option<String>,
    /// Estimated additional cost in divines (None when not estimated).
    pub added_cost_div: Option<u32>,
    /// Strategy + step ids the hint came from, for display.
    pub strategy_id: String,
    pub step_id: String,
}

/// Recovery view for a single strategy step.
#[derive(Debug, Serialize)]
pub struct RecoveryStepView {
    pub step_id: String,
    /// Action description for the goto step (when goto_step_id is set).
    /// Helps the user understand what they'd be applying next.
    pub next_action_summary: Option<String>,
    /// All hints attached to the step.
    pub hints: Vec<RecoveryHintView>,
}

/// Collect the recovery hints for `step_id` within `strategy_id`.
///
/// Mirrors the desktop `recovery_hints` command but borrows the
/// [`StrategyRegistry`] (owned by the WASM engine state) instead of reading the
/// bundle's strategy table behind a lock.
pub fn recovery_hints(
    strategies: &StrategyRegistry,
    strategy_id: String,
    step_id: String,
) -> Result<RecoveryStepView, String> {
    let strategy = strategies
        .get(&StrategyId(strategy_id.clone()))
        .ok_or_else(|| format!("unknown strategy: {strategy_id}"))?;
    let target_step_id = StepId(step_id.clone());
    let step = strategy
        .step(&target_step_id)
        .ok_or_else(|| format!("strategy {strategy_id} has no step {step_id}"))?;

    let authored = poc2_advisor::collect_strategy_hints(step);
    let mut hints = Vec::with_capacity(authored.len());
    for hint in &authored {
        hints.push(RecoveryHintView {
            message: hint.message.clone(),
            goto_step_id: hint.goto.as_ref().map(|s| s.0.clone()),
            added_cost_div: hint.added_cost_div,
            strategy_id: strategy_id.clone(),
            step_id: step_id.clone(),
        });
    }

    let next_action_summary = step.on_failure.as_ref().and_then(|sid| {
        strategy.step(sid).map(|next| match &next.action {
            Action::ApplyCurrency { currency, omens } => {
                if omens.is_empty() {
                    format!("Apply {currency}")
                } else {
                    format!(
                        "Apply {currency} with omens [{}]",
                        omens
                            .iter()
                            .map(poc2_engine::ids::OmenId::as_str)
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                }
            }
            Action::ActivateOmen { omen } => format!("Activate omen {omen}"),
            Action::HinekorasLock => "Apply Hinekora's Lock".into(),
            Action::Reveal { .. } => "Reveal at Well of Souls".into(),
            Action::Recombine { .. } => "Recombine with second item".into(),
            Action::Done => "Done".into(),
            Action::Abandon { reason } => format!("Abandon: {reason}"),
            Action::Noop => "(no-op)".into(),
            Action::LoopUntil { .. } | Action::Sequence(_) | Action::Branch(_) => {
                "(control-flow)".into()
            }
        })
    });

    Ok(RecoveryStepView {
        step_id,
        next_action_summary,
        hints,
    })
}
