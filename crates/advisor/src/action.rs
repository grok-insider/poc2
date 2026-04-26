//! Unified [`AdvisorAction`] enum.
//!
//! Both rules and strategies emit "next-step actions". They use slightly
//! different shapes:
//!
//! - [`poc2_strategies::Action`] is the strategy DSL action with
//!   `LoopUntil` / `Sequence` / `Branch` for control flow.
//! - [`poc2_rules::SuggestionAction`] is a single-step suggestion from
//!   the rule engine.
//!
//! The advisor folds both into a single `AdvisorAction` value that
//! carries only the leaf-level steps it cares about: apply currency,
//! apply lock, reveal, abandon, stop. Control-flow (Sequence / Branch /
//! LoopUntil / Done) is unwrapped by the candidate generator at the
//! point of emission.

use poc2_engine::ids::{ConceptId, CurrencyId, OmenId};
use serde::{Deserialize, Serialize};

/// One concrete next-step action the advisor can recommend.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AdvisorAction {
    /// Apply a currency, optionally pre-activating the listed omens.
    ApplyCurrency {
        currency: CurrencyId,
        #[serde(default)]
        omens: Vec<OmenId>,
    },
    /// Bind Hinekora's Lock to the item (preview the next operation).
    ApplyHinekorasLock,
    /// Reveal a hidden desecrated mod at the Well of Souls.
    /// `prefer` is the priority order for picking from the offered options.
    Reveal {
        #[serde(default)]
        prefer: Vec<ConceptId>,
        #[serde(default)]
        use_abyssal_echoes: bool,
    },
    /// The item is good enough; stop and sell or equip.
    Stop,
    /// Cut losses and abandon the craft.
    Abandon { reason: String },
    /// Free-form non-mutating guidance ("budget caution: stop after
    /// 10 div on this base"). Surfaced to the user but does not advance
    /// the planner.
    Guidance { note: String },
}

impl AdvisorAction {
    /// True if this action mutates the item (vs. surfacing guidance / stopping).
    #[must_use]
    pub fn is_mutating(&self) -> bool {
        matches!(
            self,
            AdvisorAction::ApplyCurrency { .. }
                | AdvisorAction::ApplyHinekorasLock
                | AdvisorAction::Reveal { .. }
        )
    }

    /// True if this action is terminal (stops further planning at this branch).
    #[must_use]
    pub fn is_terminal(&self) -> bool {
        matches!(self, AdvisorAction::Stop | AdvisorAction::Abandon { .. })
    }

    /// Convenience: extract the currency id, if any.
    #[must_use]
    pub fn currency_id(&self) -> Option<&CurrencyId> {
        match self {
            AdvisorAction::ApplyCurrency { currency, .. } => Some(currency),
            _ => None,
        }
    }

    /// Convenience: extract the omen list, if any.
    #[must_use]
    pub fn omens(&self) -> &[OmenId] {
        match self {
            AdvisorAction::ApplyCurrency { omens, .. } => omens,
            _ => &[],
        }
    }
}

/// Lift a strategy [`Action`] into an [`AdvisorAction`].
///
/// Returns `None` for control-flow actions (Sequence / Branch / LoopUntil)
/// — the candidate generator unwraps those structurally rather than
/// representing them as advisor steps. Returns `None` for `Noop` because
/// the advisor never proposes a no-op.
#[must_use]
pub fn from_strategy_action(action: &poc2_strategies::Action) -> Option<AdvisorAction> {
    use poc2_strategies::Action;
    match action {
        Action::ApplyCurrency { currency, omens } => Some(AdvisorAction::ApplyCurrency {
            currency: currency.clone(),
            omens: omens.clone(),
        }),
        Action::HinekorasLock => Some(AdvisorAction::ApplyHinekorasLock),
        Action::Reveal {
            prefer,
            use_abyssal_echoes,
        } => Some(AdvisorAction::Reveal {
            prefer: prefer.clone(),
            use_abyssal_echoes: *use_abyssal_echoes,
        }),
        Action::Done => Some(AdvisorAction::Stop),
        Action::Abandon { reason } => Some(AdvisorAction::Abandon {
            reason: reason.clone(),
        }),
        Action::LoopUntil { .. } | Action::Sequence(_) | Action::Branch(_) | Action::Noop => None,
    }
}

/// Lift a rule [`SuggestionAction`] into an [`AdvisorAction`].
#[must_use]
pub fn from_rule_action(action: &poc2_rules::SuggestionAction) -> AdvisorAction {
    use poc2_rules::SuggestionAction;
    match action {
        SuggestionAction::ApplyCurrency { currency, omens } => AdvisorAction::ApplyCurrency {
            currency: currency.clone(),
            omens: omens.clone(),
        },
        SuggestionAction::ApplyHinekorasLock => AdvisorAction::ApplyHinekorasLock,
        SuggestionAction::Reveal => AdvisorAction::Reveal {
            prefer: Vec::new(),
            use_abyssal_echoes: false,
        },
        SuggestionAction::StopAndSell => AdvisorAction::Stop,
        SuggestionAction::Abandon { reason } => AdvisorAction::Abandon {
            reason: reason.clone(),
        },
        SuggestionAction::Guidance => AdvisorAction::Guidance {
            note: String::new(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lift_strategy_apply_currency() {
        let s = poc2_strategies::Action::ApplyCurrency {
            currency: CurrencyId::from("ChaosOrb"),
            omens: vec![],
        };
        let a = from_strategy_action(&s).unwrap();
        assert!(matches!(a, AdvisorAction::ApplyCurrency { .. }));
        assert!(a.is_mutating());
        assert!(!a.is_terminal());
    }

    #[test]
    fn lift_strategy_done_to_stop() {
        let a = from_strategy_action(&poc2_strategies::Action::Done).unwrap();
        assert!(a.is_terminal());
    }

    #[test]
    fn lift_strategy_control_flow_returns_none() {
        assert!(from_strategy_action(&poc2_strategies::Action::Sequence(vec![])).is_none());
        assert!(from_strategy_action(&poc2_strategies::Action::Noop).is_none());
    }

    #[test]
    fn lift_rule_actions() {
        let a = from_rule_action(&poc2_rules::SuggestionAction::ApplyCurrency {
            currency: CurrencyId::from("RegalOrb"),
            omens: vec![OmenId::from("OmenOfDextralExaltation")],
        });
        assert_eq!(a.omens().len(), 1);
        assert_eq!(
            a.currency_id().map(poc2_engine::CurrencyId::as_str),
            Some("RegalOrb")
        );
    }

    #[test]
    fn lift_rule_guidance() {
        let a = from_rule_action(&poc2_rules::SuggestionAction::Guidance);
        assert!(!a.is_mutating());
        assert!(!a.is_terminal());
    }
}
