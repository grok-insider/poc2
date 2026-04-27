//! Strategy executor.
//!
//! Walks a [`Strategy`]'s step graph, evaluating `target_check` predicates
//! against the current item state and reporting the next [`Action`] the
//! advisor should propose. The executor itself does NOT run actions — that's
//! the engine's job. The executor only decides which step we're in and what
//! the next decision is.
//!
//! ## Two operating modes
//!
//! 1. **Single-step**: `next_recommendation(strategy, state, item, registry)`
//!    returns the action to propose right now. The advisor / UI takes over
//!    from there.
//!
//! 2. **Multi-step (dry run)**: `dry_run(strategy, item, registry, max_steps)`
//!    walks the graph deterministically (target_check decides each branch),
//!    returning the sequence of actions a perfect-RNG run would emit. Useful
//!    for the advisor's beam-search heuristic prior.

use poc2_engine::item::Item;
use poc2_engine::registry::ModRegistry;

use crate::dsl::{Action, ItemPredicate, Step, StepId, Strategy};
use crate::predicate::{eval, eval_all, PredicateContext};

/// Minimal mutable state the executor threads through a strategy run.
#[derive(Debug, Clone)]
pub struct ExecutionState {
    /// Step the executor is currently positioned at. `None` means "before
    /// the entry step has been chosen".
    pub current: Option<StepId>,
    /// Number of times we've evaluated this step (loop counter for
    /// LoopUntil-style steps).
    pub iterations: u32,
}

impl ExecutionState {
    pub fn new() -> Self {
        Self {
            current: None,
            iterations: 0,
        }
    }
}

impl Default for ExecutionState {
    fn default() -> Self {
        Self::new()
    }
}

/// What the executor reports for the current step.
#[derive(Debug, Clone, PartialEq)]
pub enum ExecutionResult<'a> {
    /// Recommend this action be applied to the item.
    Action { step: &'a Step, action: &'a Action },
    /// Strategy reports completion.
    Done { step: &'a Step },
    /// Strategy reports abandonment with a reason.
    Abandon { step: &'a Step, reason: String },
    /// Step references an unknown step id — a bug in the strategy.
    DanglingReference { from: &'a Step, to: StepId },
    /// Strategy is out of steps to take (target_check satisfied with no
    /// on_success target). Treated as Done.
    EndOfStrategy,
    /// Pre-target_check satisfied: strategy considers the goal already
    /// met before this step ran.
    AlreadySatisfied { step: &'a Step },
}

/// Resolve the entry point into the strategy. Honors preconditions:
/// returns `Err` describing the failing predicate(s) when a precondition
/// is unmet.
pub fn enter<'a>(
    strategy: &'a Strategy,
    item: &Item,
    registry: &ModRegistry,
) -> Result<&'a Step, EnterError> {
    let ctx = PredicateContext::new(registry);
    if !eval_all(&strategy.preconditions, item, &ctx) {
        let failing: Vec<&ItemPredicate> = strategy
            .preconditions
            .iter()
            .filter(|p| !eval(p, item, &ctx))
            .collect();
        return Err(EnterError::PreconditionFailed {
            count: failing.len(),
        });
    }
    if strategy.steps.is_empty() {
        return Err(EnterError::NoSteps);
    }
    Ok(&strategy.steps[0])
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnterError {
    PreconditionFailed { count: usize },
    NoSteps,
}

/// Single-step recommendation: given a current `ExecutionState` (or `None`
/// to enter the strategy fresh), report the next action to take.
pub fn next_recommendation<'a>(
    strategy: &'a Strategy,
    state: &ExecutionState,
    item: &Item,
    registry: &ModRegistry,
) -> ExecutionResult<'a> {
    // Resolve the current step.
    let step = match &state.current {
        Some(id) => match strategy.step(id) {
            Some(s) => s,
            None => {
                return ExecutionResult::DanglingReference {
                    from: strategy.steps.first().unwrap_or(&strategy.steps[0]),
                    to: id.clone(),
                }
            }
        },
        None => match strategy.steps.first() {
            Some(s) => s,
            None => return ExecutionResult::EndOfStrategy,
        },
    };

    // If the step has a target_check that's already true on entry, the
    // strategy considers the goal met — surface that to the advisor so
    // it can advance to on_success without re-running the action.
    let ctx = PredicateContext::new(registry);
    if let Some(check) = &step.target_check {
        if eval(check, item, &ctx) {
            return ExecutionResult::AlreadySatisfied { step };
        }
    }

    match &step.action {
        Action::Done => ExecutionResult::Done { step },
        Action::Abandon { reason } => ExecutionResult::Abandon {
            step,
            reason: reason.clone(),
        },
        action => ExecutionResult::Action { step, action },
    }
}

/// Advance the execution state after the engine reports the action's outcome.
///
/// `applied_ok` is the success/failure signal from the engine's `apply()`
/// (or the executor's own evaluation of the post-action target_check).
/// Returns the new state. If the strategy has no further step to advance
/// to, the state's `current` becomes `None`.
pub fn advance(strategy: &Strategy, state: &ExecutionState, applied_ok: bool) -> ExecutionState {
    let Some(current_id) = state.current.clone() else {
        // Fresh start: advance into the entry step.
        let next = strategy.steps.first().map(|s| s.id.clone());
        return ExecutionState {
            current: next,
            iterations: 0,
        };
    };
    let Some(step) = strategy.step(&current_id) else {
        return ExecutionState {
            current: None,
            iterations: state.iterations,
        };
    };
    let next = if applied_ok {
        step.on_success.clone()
    } else {
        step.on_failure.clone()
    };
    let iterations = if next.as_ref() == Some(&current_id) {
        // Loop: bump iteration count.
        state.iterations + 1
    } else {
        0
    };
    ExecutionState {
        current: next,
        iterations,
    }
}

/// Deterministic dry run: walk the strategy graph using the executor's
/// `target_check`-based branch decision, NOT actually applying any
/// currency. Returns the sequence of `(StepId, &Action)` pairs encountered
/// up to `max_steps` or a terminal node.
///
/// Useful for:
/// - Advisor's beam-search prior (what does this strategy want to do?)
/// - Strategy debugging / linting
/// - Tests that validate strategy graphs without engine state evolution
pub fn dry_run<'a>(
    strategy: &'a Strategy,
    item: &Item,
    registry: &ModRegistry,
    max_steps: u32,
) -> Vec<DryRunStep<'a>> {
    let mut out = Vec::new();
    let mut state = ExecutionState::new();
    state.current = strategy.steps.first().map(|s| s.id.clone());
    for _ in 0..max_steps {
        let Some(id) = state.current.clone() else {
            break;
        };
        let Some(step) = strategy.step(&id) else {
            out.push(DryRunStep {
                step_id: id.clone(),
                action: None,
                terminal: TerminalKind::Dangling,
            });
            break;
        };
        match &step.action {
            Action::Done => {
                out.push(DryRunStep {
                    step_id: id,
                    action: Some(&step.action),
                    terminal: TerminalKind::Done,
                });
                break;
            }
            Action::Abandon { reason } => {
                out.push(DryRunStep {
                    step_id: id,
                    action: Some(&step.action),
                    terminal: TerminalKind::Abandoned(reason.clone()),
                });
                break;
            }
            _ => {}
        }
        let ctx = PredicateContext::new(registry);
        let satisfied = step
            .target_check
            .as_ref()
            .is_some_and(|p| eval(p, item, &ctx));
        out.push(DryRunStep {
            step_id: id,
            action: Some(&step.action),
            terminal: TerminalKind::None,
        });
        state = advance(strategy, &state, satisfied);
    }
    out
}

#[derive(Debug, Clone, PartialEq)]
pub struct DryRunStep<'a> {
    pub step_id: StepId,
    pub action: Option<&'a Action>,
    pub terminal: TerminalKind,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TerminalKind {
    None,
    Done,
    Abandoned(String),
    Dangling,
}

#[cfg(test)]
mod tests {
    use poc2_engine::ids::ItemClassId;
    use poc2_engine::item::{QualityKind, Rarity};
    use poc2_engine::patch::PatchVersion;
    use smallvec::smallvec;

    use super::*;
    use crate::dsl::{
        Action, CmpOp, Confidence, ItemPredicate, Source, Step, StepId, Strategy, StrategyId,
        Target, ValuePredicate,
    };

    fn empty_item() -> Item {
        Item {
            base: ItemClassId::from("BodyArmour").as_str().into(),
            ilvl: 82,
            rarity: Rarity::Normal,
            corrupted: false,
            sanctified: false,
            mirrored: false,
            quality: 0,
            quality_kind: QualityKind::Untagged,
            implicits: smallvec![],
            prefixes: smallvec![],
            suffixes: smallvec![],
            enchantments: smallvec![],
            hidden_desecrated: None,
            sockets: smallvec![],
            hinekora_lock: None,
        }
    }

    fn empty_registry() -> ModRegistry {
        ModRegistry::from_mods(vec![], vec![])
    }

    fn three_step_strategy() -> Strategy {
        Strategy {
            id: StrategyId::from("test"),
            name: "test".into(),
            source: Source::default(),
            patch_min: Some(PatchVersion::PATCH_0_4_0),
            patch_max: None,
            item_classes: vec![],
            attribute_pools: vec![],
            preconditions: vec![ItemPredicate::Ilvl(ValuePredicate {
                op: CmpOp::Gte,
                value: 82,
            })],
            target: Target::default(),
            abandon_criteria: vec![],
            steps: vec![
                Step {
                    id: StepId::from("S1"),
                    action: Action::ApplyCurrency {
                        currency: poc2_engine::ids::CurrencyId::from("OrbOfTransmutation"),
                        omens: vec![],
                    },
                    target_check: None,
                    on_success: Some(StepId::from("S2")),
                    on_failure: Some(StepId::from("S3")),
                    recovery: smallvec![],
                    note: None,
                },
                Step {
                    id: StepId::from("S2"),
                    action: Action::Done,
                    target_check: None,
                    on_success: None,
                    on_failure: None,
                    recovery: smallvec![],
                    note: None,
                },
                Step {
                    id: StepId::from("S3"),
                    action: Action::Abandon {
                        reason: "transmute missed".into(),
                    },
                    target_check: None,
                    on_success: None,
                    on_failure: None,
                    recovery: smallvec![],
                    note: None,
                },
            ],
            expected_cost_div: None,
            expected_success_prob: None,
            confidence: Confidence::default(),
            note: None,
        }
    }

    #[test]
    fn enter_succeeds_when_preconditions_met() {
        let s = three_step_strategy();
        let item = empty_item();
        let reg = empty_registry();
        let entry = enter(&s, &item, &reg).unwrap();
        assert_eq!(entry.id.0, "S1");
    }

    #[test]
    fn enter_fails_when_precondition_unmet() {
        let s = three_step_strategy();
        let mut item = empty_item();
        item.ilvl = 75;
        let reg = empty_registry();
        let r = enter(&s, &item, &reg);
        assert!(matches!(
            r,
            Err(EnterError::PreconditionFailed { count: 1 })
        ));
    }

    #[test]
    fn next_recommendation_returns_action_for_step() {
        let s = three_step_strategy();
        let item = empty_item();
        let reg = empty_registry();
        let state = ExecutionState::new();
        let r = next_recommendation(&s, &state, &item, &reg);
        match r {
            ExecutionResult::Action { step, .. } => assert_eq!(step.id.0, "S1"),
            other => panic!("expected Action, got {other:?}"),
        }
    }

    #[test]
    fn advance_follows_on_success_and_on_failure() {
        let s = three_step_strategy();
        let mut state = ExecutionState::new();
        state.current = Some(StepId::from("S1"));
        let succ = advance(&s, &state, true);
        assert_eq!(succ.current.as_ref().map(|i| i.0.as_str()), Some("S2"));
        let fail = advance(&s, &state, false);
        assert_eq!(fail.current.as_ref().map(|i| i.0.as_str()), Some("S3"));
    }

    #[test]
    fn dry_run_walks_to_terminal() {
        let s = three_step_strategy();
        let item = empty_item();
        let reg = empty_registry();
        // No target_check on S1, so dry_run takes on_failure (advance with
        // applied_ok=false). That goes S1 -> S3 (Abandon).
        let trail = dry_run(&s, &item, &reg, 10);
        assert_eq!(trail.len(), 2);
        assert_eq!(trail[0].step_id.0, "S1");
        assert!(matches!(trail[1].terminal, TerminalKind::Abandoned(_)));
    }

    #[test]
    fn next_recommendation_reports_done_at_done_step() {
        let s = three_step_strategy();
        let item = empty_item();
        let reg = empty_registry();
        let state = ExecutionState {
            current: Some(StepId::from("S2")),
            iterations: 0,
        };
        let r = next_recommendation(&s, &state, &item, &reg);
        assert!(matches!(r, ExecutionResult::Done { .. }));
    }
}
