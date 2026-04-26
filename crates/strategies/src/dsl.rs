//! Strategy DSL types.
//!
//! A [`Strategy`] is a multi-step crafting recipe with branch outcomes and
//! recovery sub-trees up to 3 levels deep (per planning). Strategies are
//! deserialized from TOML or JSON.
//!
//! ## TOML shape (canonical)
//!
//! ```toml
//! id = "3xt1-es-body-armour"
//! name = "Triple T1 Energy Shield Body Armour"
//! patch_min = "0.4.0"
//! item_classes = ["BodyArmour"]
//! attribute_pools = ["Int", "DexInt", "StrInt"]
//! source = { kind = "user", credit = "project-author" }
//!
//! [preconditions]
//! ilvl = { op = "gte", value = 82 }
//! rarity = "normal"
//! corrupted = false
//!
//! [target]
//! prefixes = [
//!   { concept = "EnergyShield", count = 3, allow_hybrid = true, min_tier = 1 }
//! ]
//! suffixes = [
//!   { concept_any = ["FireResistance","ColdResistance","LightningResistance"], min_count = 2, min_tier = 1 }
//! ]
//!
//! [[steps]]
//! id = "S1-perfect-transmute"
//! action = { kind = "apply_currency", currency = "PerfectOrbOfTransmutation" }
//! on_success = "S3-evaluate"
//! on_failure = "S2-restart"
//!
//! [[steps]]
//! id = "S2-restart"
//! action = { kind = "abandon", reason = "no_t1_es_after_transmute" }
//!
//! ...
//! ```

use poc2_engine::ids::{ConceptId, CurrencyId, ItemClassId, OmenId};
use poc2_engine::item::{AffixType, Rarity};
use poc2_engine::item_class::AttributePool;
use poc2_engine::patch::PatchVersion;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

// ---------------------------------------------------------------------------
// Newtype IDs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct StrategyId(pub String);

impl From<&str> for StrategyId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct StepId(pub String);

impl From<&str> for StepId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

// ---------------------------------------------------------------------------
// Source citation
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum Source {
    /// Engine-internal seed (no external citation).
    #[default]
    Internal,
    /// Authored by a community member / streamer / guide.
    Community {
        credit: String,
        reference: Option<String>,
    },
    /// Authored by the project's author themselves.
    User {
        credit: String,
        reference: Option<String>,
    },
}

// ---------------------------------------------------------------------------
// Predicates over item state
// ---------------------------------------------------------------------------

/// Comparison op for numeric predicates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CmpOp {
    Eq,
    Ne,
    Lt,
    Lte,
    Gt,
    Gte,
}

/// Numeric or comparison predicate over a single value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ValuePredicate {
    pub op: CmpOp,
    pub value: i64,
}

impl ValuePredicate {
    pub fn matches(&self, v: i64) -> bool {
        match self.op {
            CmpOp::Eq => v == self.value,
            CmpOp::Ne => v != self.value,
            CmpOp::Lt => v < self.value,
            CmpOp::Lte => v <= self.value,
            CmpOp::Gt => v > self.value,
            CmpOp::Gte => v >= self.value,
        }
    }
}

/// Numeric or comparison predicate over a single floating-point value.
///
/// Used for divine-equivalent cost / sale-price comparisons where integer
/// math would lose meaningful precision (a 0.05 div threshold matters when
/// the basic-orb prices are ~0.01-0.03 div).
///
/// Equality / inequality use a small absolute tolerance (1e-9) — exact
/// `f64::PartialEq` would be brittle for values derived from real-world
/// price feeds.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FloatValuePredicate {
    pub op: CmpOp,
    pub value: f64,
}

impl FloatValuePredicate {
    pub fn matches(&self, v: f64) -> bool {
        const TOL: f64 = 1e-9;
        match self.op {
            CmpOp::Eq => (v - self.value).abs() < TOL,
            CmpOp::Ne => (v - self.value).abs() >= TOL,
            CmpOp::Lt => v < self.value,
            CmpOp::Lte => v <= self.value,
            CmpOp::Gt => v > self.value,
            CmpOp::Gte => v >= self.value,
        }
    }
}

/// A predicate over the [`Item`](poc2_engine::Item) state.
///
/// Strategy preconditions and step `target_check` fields are expressed as
/// [`ItemPredicate`]s, evaluated by the advisor / strategy executor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemPredicate {
    /// Item-level constraint.
    Ilvl(ValuePredicate),
    /// Rarity constraint (exact).
    Rarity(Rarity),
    /// Corrupted state.
    Corrupted(bool),
    /// Sanctified state.
    Sanctified(bool),
    /// Mirrored state.
    Mirrored(bool),
    /// Item-class equality.
    ItemClass(ItemClassId),
    /// Item-class is one of the given set.
    ItemClassAny(Vec<ItemClassId>),
    /// Attribute-pool equality.
    AttributePool(AttributePool),
    /// Attribute-pool is one of the given set.
    AttributePoolAny(Vec<AttributePool>),
    /// Number of mods of an affix type matches the predicate.
    AffixCount {
        affix: AffixType,
        count: ValuePredicate,
    },
    /// True iff at least one prefix or suffix has the given concept in its
    /// concept_set (handles hybrids correctly).
    HasConcept {
        concept: ConceptId,
        affix: Option<AffixType>,
        min_tier: Option<u8>,
    },
    /// True iff the item has at least one fractured mod.
    HasFractured(bool),
    /// True iff the item carries a hidden desecrated mod slot.
    HasHiddenDesecrated(bool),
    /// True iff the item carries at least one revealed desecrated mod
    /// (a [`poc2_engine::ModRoll`] of kind [`poc2_engine::ModKind::Desecrated`]).
    HasDesecratedRevealed(bool),
    /// True iff the item is currently bound by Hinekora's Lock.
    HasHinekoraLock(bool),
    /// Total explicit prefix + suffix count matches the predicate.
    /// (Implicits and enchantments are not included.)
    ModCount(ValuePredicate),
    /// Item quality value matches the predicate (0..=30 typical).
    /// Untagged and tagged-quality both contribute the same value.
    Quality(ValuePredicate),
    /// True iff the user's stash holds at least the specified count
    /// of the named currency. Always returns false when no [`StashView`]
    /// is attached to the [`PredicateContext`].
    StashHas {
        currency: poc2_engine::ids::CurrencyId,
        count: ValuePredicate,
    },
    /// Cumulative cost spent on the craft so far, in divine-equivalent.
    /// Always returns false unless the [`PredicateContext`] carries a
    /// `cost_so_far_div` value.
    CostSpent(FloatValuePredicate),
    /// Estimated sale price of the current item state, in
    /// divine-equivalent. Always returns false unless the
    /// [`PredicateContext`] carries an `expected_sale_price_div` value.
    ExpectedSalePrice(FloatValuePredicate),
    /// Logical conjunction of subpredicates.
    All(Vec<ItemPredicate>),
    /// Logical disjunction of subpredicates.
    Any(Vec<ItemPredicate>),
    /// Logical negation.
    Not(Box<ItemPredicate>),
    /// Always true — useful as an explicit "no precondition".
    Always,
    /// Always false — useful for testing rejection paths.
    Never,
}

// ---------------------------------------------------------------------------
// Target — the goal the strategy is working toward
// ---------------------------------------------------------------------------

/// Specification of a desired mod on the item.
///
/// A `TargetSpec` is satisfied when at least `count` mods on the item
/// match the concept (or any concept in `concept_any`), at the given
/// affix slot if specified, optionally with `min_tier` constraint.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TargetSpec {
    /// Single concept to match (mutually exclusive with `concept_any`).
    pub concept: Option<ConceptId>,
    /// Match if any concept in this list is satisfied.
    #[serde(default)]
    pub concept_any: Vec<ConceptId>,
    /// Required affix slot (None = either prefix or suffix).
    pub affix: Option<AffixType>,
    /// How many distinct matching mods are required (default 1).
    #[serde(default = "default_count")]
    pub count: u8,
    /// Minimum tier number (1 = best).
    pub min_tier: Option<u8>,
    /// Whether hybrid mods (mods producing multiple concepts) count.
    /// Default true — the user's "T1 ES flat or hybrid" pattern.
    #[serde(default = "default_true")]
    pub allow_hybrid: bool,
}

fn default_count() -> u8 {
    1
}
fn default_true() -> bool {
    true
}

/// Target = list of `TargetSpec`s, all of which must be satisfied for
/// the strategy to be considered successful.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Target {
    #[serde(default)]
    pub prefixes: Vec<TargetSpec>,
    #[serde(default)]
    pub suffixes: Vec<TargetSpec>,
    /// Free-form constraints not expressible as prefix/suffix specs
    /// (e.g., "must be ilvl 82", "must not be corrupted").
    #[serde(default)]
    pub constraints: Vec<ItemPredicate>,
}

// ---------------------------------------------------------------------------
// Action — what a step actually does
// ---------------------------------------------------------------------------

/// One concrete action performed by a strategy step.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum Action {
    /// Apply a currency. Optionally pre-activate omens (engine consumes
    /// them as part of the apply).
    ApplyCurrency {
        currency: CurrencyId,
        #[serde(default)]
        omens: Vec<OmenId>,
    },
    /// Activate one omen for the next omen-consuming action, without
    /// applying any currency. Useful when a strategy wants to bind an
    /// omen ahead of a downstream `ApplyCurrency` step (e.g. activating
    /// Omen of Abyssal Echoes before walking to the Well of Souls,
    /// keeping the activation traceable to its own step in the graph).
    ActivateOmen { omen: OmenId },
    /// Apply Hinekora's Lock (pure preview-bind step).
    HinekorasLock,
    /// Reveal a hidden desecrated mod at the Well of Souls.
    ///
    /// `prefer` is a list of concepts the executor should prefer (in
    /// order) when picking from the 3-of-N options.
    ///
    /// `min_acceptable` (added in A.2) lets a strategy say "if NONE of
    /// the offered options match this concept, treat the reveal as a
    /// failure and route through `on_failure`". When combined with
    /// `abandon_if_no_match = true`, the strategy abandons immediately
    /// instead of accepting a junk reveal.
    Reveal {
        #[serde(default)]
        prefer: Vec<ConceptId>,
        /// Whether to consume an Abyssal Echoes omen for a re-roll.
        #[serde(default)]
        use_abyssal_echoes: bool,
        /// Floor concept: if no offered option carries this concept, the
        /// step is treated as a failure (`on_failure`).
        #[serde(default)]
        min_acceptable: Option<ConceptId>,
        /// When `true`, a reveal that fails the `min_acceptable` floor
        /// surfaces as an `Abandon` recommendation rather than routing
        /// through `on_failure`.
        #[serde(default)]
        abandon_if_no_match: bool,
    },
    /// Recombine the current item with another item the player owns.
    ///
    /// `other_item` is an [`ItemPredicate`] the candidate-generator uses
    /// to select the second item from the player's stash. `omens` are
    /// pre-activated for the recombine (e.g. `OmenOfRecombination`).
    ///
    /// In v1, the advisor surfaces this as a recommendation only when
    /// the planner can locate a matching second item — otherwise the
    /// candidate is dropped. Plugin SDK (Phase F) will let plugins
    /// emit richer Recombine candidates with custom selection logic.
    Recombine {
        other_item: ItemPredicate,
        #[serde(default)]
        omens: Vec<OmenId>,
    },
    /// Run a sub-loop until the inner step's `target_check` is satisfied
    /// or `abandon_after_cost_div` is exceeded.
    LoopUntil {
        body: Box<Step>,
        check: ItemPredicate,
        abandon_after_cost_div: Option<u32>,
    },
    /// Linear sequence of nested steps. Useful for grouping related work.
    Sequence(Vec<Step>),
    /// Decision point: surface options to the user / advisor.
    Branch(Vec<Branch>),
    /// Abandon the strategy and report a reason.
    Abandon { reason: String },
    /// Mark the strategy as complete (the target is satisfied).
    Done,
    /// No-op, useful for testing / placeholder steps.
    Noop,
}

/// One branch of an [`Action::Branch`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Branch {
    /// Predicate that selects this branch (first matching wins).
    pub when: ItemPredicate,
    /// Step to execute if this branch is selected.
    pub goto: StepId,
    /// Optional human-readable label.
    pub label: Option<String>,
}

// ---------------------------------------------------------------------------
// Step — a node in the strategy graph
// ---------------------------------------------------------------------------

/// Recovery hint surfaced when a step fails.
///
/// Recovery hints are advisory — they don't drive execution by themselves.
/// The advisor / UI surfaces them to the user as alternatives.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecoveryHint {
    /// Human-readable explanation.
    pub message: String,
    /// Step to navigate to if the user accepts the hint.
    pub goto: Option<StepId>,
    /// Approximate added cost in divines (advisor surfaces this).
    pub added_cost_div: Option<u32>,
}

/// One step in a strategy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Step {
    pub id: StepId,
    pub action: Action,
    /// Predicate evaluated post-action. If satisfied → on_success, else
    /// on_failure. `None` means "always go to on_success".
    #[serde(default)]
    pub target_check: Option<ItemPredicate>,
    pub on_success: Option<StepId>,
    pub on_failure: Option<StepId>,
    #[serde(default)]
    pub recovery: SmallVec<[RecoveryHint; 3]>,
    /// Free-form description; surfaced in advisor explanations.
    pub note: Option<String>,
}

// ---------------------------------------------------------------------------
// Strategy — the top-level container
// ---------------------------------------------------------------------------

/// A multi-step crafting strategy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Strategy {
    pub id: StrategyId,
    pub name: String,
    #[serde(default)]
    pub source: Source,
    pub patch_min: Option<PatchVersion>,
    pub patch_max: Option<PatchVersion>,
    #[serde(default)]
    pub item_classes: Vec<ItemClassId>,
    #[serde(default)]
    pub attribute_pools: Vec<AttributePool>,
    /// Predicates that must hold on the input item before the strategy
    /// can run.
    #[serde(default)]
    pub preconditions: Vec<ItemPredicate>,
    /// What the strategy is trying to achieve.
    #[serde(default)]
    pub target: Target,
    /// Predicates that, when satisfied, mean the strategy should be
    /// abandoned (e.g., "if I've spent > N divines and no progress").
    #[serde(default)]
    pub abandon_criteria: Vec<ItemPredicate>,
    /// Step graph. Steps reference each other by id; the entry point is
    /// the first step.
    pub steps: Vec<Step>,
    /// Approximate expected cost range in divines.
    pub expected_cost_div: Option<(u32, u32)>,
    /// Approximate success probability range.
    pub expected_success_prob: Option<(f32, f32)>,
    /// How confident is the source's claims for this strategy?
    #[serde(default)]
    pub confidence: Confidence,
    /// Free-form notes (rendered in advisor explanations).
    pub note: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    /// Verified via test fixtures or in-game observation.
    Verified,
    /// Community-derived (e.g., from a streamer's VOD).
    Community,
    /// Experimental / theoretical (no in-game verification yet).
    #[default]
    Experimental,
}

impl Strategy {
    /// Look up a step by id. O(n) — strategies typically have ≤ 30 steps.
    pub fn step(&self, id: &StepId) -> Option<&Step> {
        self.steps.iter().find(|s| &s.id == id)
    }

    /// Entry-point step (first in the `steps` array).
    pub fn entry(&self) -> Option<&Step> {
        self.steps.first()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn value_predicate_matches() {
        let p = ValuePredicate {
            op: CmpOp::Gte,
            value: 82,
        };
        assert!(p.matches(82));
        assert!(p.matches(85));
        assert!(!p.matches(81));
    }

    #[test]
    fn target_spec_defaults() {
        let json = r#"{"concept": "EnergyShield"}"#;
        let s: TargetSpec = serde_json::from_str(json).unwrap();
        assert_eq!(s.count, 1);
        assert!(s.allow_hybrid);
        assert_eq!(
            s.concept.as_ref().map(poc2_engine::ConceptId::as_str),
            Some("EnergyShield")
        );
    }

    #[test]
    fn confidence_default_is_experimental() {
        assert_eq!(Confidence::default(), Confidence::Experimental);
    }

    #[test]
    fn predicate_serde_round_trip() {
        let p = ItemPredicate::All(vec![
            ItemPredicate::Ilvl(ValuePredicate {
                op: CmpOp::Gte,
                value: 82,
            }),
            ItemPredicate::Rarity(Rarity::Normal),
            ItemPredicate::Corrupted(false),
        ]);
        let s = serde_json::to_string(&p).unwrap();
        let back: ItemPredicate = serde_json::from_str(&s).unwrap();
        assert_eq!(back, p);
    }

    // ------------------------------------------------------------------
    // A.2 — DSL action extensions
    // ------------------------------------------------------------------

    #[test]
    fn activate_omen_action_serde_round_trip() {
        let a = Action::ActivateOmen {
            omen: poc2_engine::ids::OmenId::from("OmenOfAbyssalEchoes"),
        };
        let s = serde_json::to_string(&a).unwrap();
        let back: Action = serde_json::from_str(&s).unwrap();
        assert_eq!(back, a);
    }

    #[test]
    fn reveal_action_backward_compatible_with_minimal_toml() {
        // Existing strategies (e.g. 3xt1-es-body-armour.toml) use
        // `action = { kind = "reveal", prefer = [...] }` without the
        // new min_acceptable / abandon_if_no_match fields. Serde must
        // accept that shape via #[serde(default)].
        let toml_str = r#"
            kind = "reveal"
            prefer = ["EnergyShield"]
            use_abyssal_echoes = true
        "#;
        let action: Action = toml::from_str(toml_str).expect("legacy reveal shape parses");
        let Action::Reveal {
            prefer,
            use_abyssal_echoes,
            min_acceptable,
            abandon_if_no_match,
        } = action
        else {
            panic!("expected Reveal");
        };
        assert_eq!(prefer.len(), 1);
        assert!(use_abyssal_echoes);
        assert!(min_acceptable.is_none());
        assert!(!abandon_if_no_match);
    }

    #[test]
    fn reveal_with_floor_serde_round_trip() {
        let a = Action::Reveal {
            prefer: vec![ConceptId::from("EnergyShield")],
            use_abyssal_echoes: true,
            min_acceptable: Some(ConceptId::from("EnergyShield")),
            abandon_if_no_match: true,
        };
        let s = serde_json::to_string(&a).unwrap();
        let back: Action = serde_json::from_str(&s).unwrap();
        assert_eq!(back, a);
    }

    #[test]
    fn recombine_action_serde_round_trip() {
        let a = Action::Recombine {
            other_item: ItemPredicate::HasFractured(true),
            omens: vec![poc2_engine::ids::OmenId::from("OmenOfRecombination")],
        };
        let s = serde_json::to_string(&a).unwrap();
        let back: Action = serde_json::from_str(&s).unwrap();
        assert_eq!(back, a);
    }
}
