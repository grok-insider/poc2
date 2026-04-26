//! Rule type definitions.

use poc2_engine::ids::{CurrencyId, OmenId};
use poc2_strategies::ItemPredicate;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RuleId(pub String);

impl From<&str> for RuleId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

/// One rule.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rule {
    pub id: RuleId,
    pub category: Category,
    pub when: ItemPredicate,
    pub then: SmallVec<[Suggestion; 4]>,
    pub explanation: String,
    pub source: String,
    #[serde(default)]
    pub confidence: Confidence,
}

/// Category labels mirror /docs/34-heuristics-rulebook.md sections.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Abandonment,
    Fracture,
    HinekoraLock,
    ExaltVsDesecrate,
    WhittleVsAnnul,
    StopVsContinue,
    Pricing,
    Budget,
    BaseSelection,
    Vaal,
    Market,
    Recovery,
    Ev,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    Verified,
    #[default]
    Community,
    Experimental,
}

/// One suggestion produced by a rule.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Suggestion {
    pub action: SuggestionAction,
    /// Free-form text rendered alongside the recommendation.
    pub note: String,
    /// Higher = more important when ranked against other suggestions
    /// of the same category. Default 100.
    #[serde(default = "default_priority")]
    pub priority: u32,
    /// Optional UI category for non-actionable suggestions. Surfaced
    /// values (A.5):
    ///
    /// - `"league_advice"`: market-awareness rule, surfaced as a tip
    ///   in the Settings panel rather than as a top-N recommendation.
    /// - `"meta"`: confidence / EV note that doesn't drive action.
    /// - `"warning"`: high-stakes caution that should be surfaced
    ///   prominently.
    ///
    /// `None` means the suggestion is a regular actionable
    /// recommendation that goes through the advisor's ranking.
    #[serde(default)]
    pub tag: Option<String>,
}

fn default_priority() -> u32 {
    100
}

/// What the rule wants the user to do.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum SuggestionAction {
    /// Apply a currency, optionally with omens pre-activated.
    ApplyCurrency {
        currency: CurrencyId,
        #[serde(default)]
        omens: Vec<OmenId>,
    },
    /// Pre-bind one omen for the next omen-consuming action without
    /// applying any currency yourself. Mirrors
    /// [`poc2_strategies::Action::ActivateOmen`] (added in A.2).
    ActivateOmen { omen: OmenId },
    /// Apply Hinekora's Lock to make the next operation deterministic-preview.
    ApplyHinekorasLock,
    /// Reveal at the Well of Souls.
    Reveal,
    /// Stop crafting; the item is good enough to sell / equip.
    StopAndSell,
    /// Abandon the craft; cut losses.
    Abandon { reason: String },
    /// Generic guidance with no concrete action ("budget rule fired").
    Guidance,
}

/// A collection of rules. Built once at engine startup; queries are
/// linear-scan O(n) over rules. With ~120 rules and microsecond predicate
/// evaluations, this is well under 1ms per query — fast enough.
#[derive(Debug, Clone, Default)]
pub struct RuleSet {
    rules: Vec<Rule>,
}

impl RuleSet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_rules(rules: Vec<Rule>) -> Self {
        Self { rules }
    }

    pub fn len(&self) -> usize {
        self.rules.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Rule> {
        self.rules.iter()
    }

    pub fn push(&mut self, rule: Rule) {
        self.rules.push(rule);
    }
}
