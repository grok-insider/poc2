//! Candidate-action generation.
//!
//! The candidate generator is the heart of the advisor's first stage:
//! given a state, it enumerates the (small, plausible) set of next-step
//! actions to expand in the beam search. Three sources contribute:
//!
//! 1. **Rules** ([`poc2_rules`]) — every rule whose `when` predicate
//!    fires emits one or more suggestions. Cheap (forward chain), high
//!    signal.
//! 2. **Strategies** ([`poc2_strategies`]) — every strategy in the
//!    registry whose preconditions match emits the action of its current
//!    entry step. Multi-step lookahead happens via the planner re-running
//!    the generator at deeper depths.
//! 3. **Heuristics** — a small, hard-coded fallback set so that the
//!    advisor still produces something useful when both rules and
//!    strategies fall silent (e.g., "Normal item with no rules firing
//!    → suggest Transmute").
//!
//! The generator filters by stash availability when [`Stash::unlimited`]
//! is false, so the advisor never recommends an action the user can't
//! take.

use poc2_engine::currency::CurrencyResolver;
use poc2_engine::ids::CurrencyId;
use poc2_engine::item::{Item, ModRoll, Rarity};
use poc2_engine::patch::PatchVersion;
use poc2_engine::registry::ModRegistry;
use poc2_rules::RuleSet;
use poc2_strategies::{eval_all, PredicateContext, Step, Strategy, StrategyRegistry};

use crate::action::{from_rule_action, from_strategy_action, AdvisorAction};
use crate::goal::Goal;
use crate::recommendation::{ConceptCriterion, RecommendationSource, StopPredicate};
use crate::stash::Stash;

/// Currencies the advisor must never recommend, even if a rule or
/// strategy proposes them. Per the v2 plan (`docs/80-crafter-helper-v2-plan.md`,
/// Phase A.3): Orb of Alchemy generates uncontrolled randomness and is
/// strictly worse than the controlled Trans → Aug → Regal/Essence chain.
fn is_blocked_currency(id: &str) -> bool {
    matches!(id, "OrbOfAlchemy")
}

/// Per-action engine precondition gate. Returns `true` when an action is
/// applicable to `item` according to the engine's `Currency::can_apply_to`
/// or the action's class-level rules. Drops illegal currency steps
/// (e.g., Exalted Orb on a Normal item) before they reach scoring.
fn passes_engine_preconditions(
    action: &AdvisorAction,
    item: &Item,
    resolver: &dyn CurrencyResolver,
) -> bool {
    match action {
        AdvisorAction::ApplyCurrency { currency, .. } => {
            if is_blocked_currency(currency.as_str()) {
                return false;
            }
            match resolver.resolve(currency) {
                Some(currency_obj) => currency_obj.can_apply_to(item).is_ok(),
                // Unknown currency id: don't accidentally surface it.
                None => false,
            }
        }
        // Reveal/Recombine/HinekorasLock are gated by their own predicates
        // upstream; the advisor leaves them alone here.
        _ => true,
    }
}

/// One candidate action plus the source / prior info the planner needs.
#[derive(Debug, Clone)]
pub struct Candidate {
    /// Concrete action.
    pub action: AdvisorAction,
    /// Where it came from.
    pub source: RecommendationSource,
    /// Source's confidence in this action being correct, in `[0, 1]`.
    /// Higher = more sure. Used as a soft prior during scoring.
    pub prior: f64,
    /// Source's priority signal (0..=255 roughly). Used for tie-breaking.
    pub priority: u32,
    /// Free-form rationale string the source attached.
    pub rationale: String,
}

/// Build the candidate set for a given state.
///
/// Returns deduplicated candidates: if multiple sources propose the same
/// `(currency, omens)` action, the highest-priority one wins.
///
/// `ctx` carries the registry plus optional cost / stash / valuator /
/// expected-sale-price data used by [`poc2_strategies::ItemPredicate`]
/// variants such as `CostSpent`, `StashHas`, and `ExpectedSalePrice`.
/// `stash` is also passed separately because action-affordability is a
/// runtime check (not a predicate) and may differ in v1.x when "buyable
/// now" prices feed into the affordability calculus.
///
/// When `goal` is supplied (Phase B.2), the candidate generator emits
/// tier-fix recommendations (Divine / Fracture) on items whose keeper
/// mods are already at top tier with high-roll values. Pass `None` to
/// disable that path (e.g., for tests that only want the rule/strategy
/// surface).
#[must_use]
#[allow(clippy::too_many_arguments)] // mirrors the planner's PlanInput contract
pub fn generate_candidates_with_goal(
    item: &Item,
    ctx: &PredicateContext<'_>,
    rules: &RuleSet,
    strategies: &StrategyRegistry,
    resolver: &dyn CurrencyResolver,
    stash: &Stash,
    patch: PatchVersion,
    goal: Option<&Goal>,
    registry: &ModRegistry,
) -> Vec<Candidate> {
    let mut out = collect_candidates_pre_goal(item, ctx, rules, strategies, stash, patch);

    // Phase B.2: emit tier-fix candidates when a keeper mod is already
    // at the configured min_tier with a high roll. Divine refines
    // values; Fracturing locks the keeper before risky steps.
    if let Some(goal) = goal {
        let tier_fix_cands = tier_fix_candidates(item, goal, registry);
        out.extend(tier_fix_cands);
    }

    // Phase B.6: emit one recommendation per legal `(bone, omen)`
    // reveal pair when the item carries a hidden desecrated slot. The
    // OutcomeDialog's bone-reveal sub-control reads `bone`/`omen` off
    // the action so the user can select among the proposed pairs.
    if item.hidden_desecrated.is_some() {
        out.extend(omen_aware_reveal_candidates(item, goal));
    }

    // Phase B.4-loop: when the item is a Rare with at least one mod and
    // the goal can plausibly be reached via Chaos-spam (or Annul-to-1
    // followed by Chaos), surface a single Recurring step instead of
    // letting the beam stamp out N depth-1 Chaos clones.
    if let Some(goal) = goal {
        if let Some(c) = recurring_chaos_candidate(item, goal) {
            out.push(c);
        }
    }

    finalize_candidates(out, item, resolver)
}

/// Backwards-compatible call site. Emits the same candidate set as
/// [`generate_candidates_with_goal`] but skips Phase B.2 tier-fix
/// recommendations because the legacy callers don't know the goal.
#[must_use]
pub fn generate_candidates(
    item: &Item,
    ctx: &PredicateContext<'_>,
    rules: &RuleSet,
    strategies: &StrategyRegistry,
    resolver: &dyn CurrencyResolver,
    stash: &Stash,
    patch: PatchVersion,
) -> Vec<Candidate> {
    let out = collect_candidates_pre_goal(item, ctx, rules, strategies, stash, patch);
    finalize_candidates(out, item, resolver)
}

/// Rule + strategy + heuristic-fallback emission, before tier-fix /
/// engine-gate / dedup. Shared by [`generate_candidates`] and
/// [`generate_candidates_with_goal`] (which adds B.2 tier-fix on top).
fn collect_candidates_pre_goal(
    item: &Item,
    ctx: &PredicateContext<'_>,
    rules: &RuleSet,
    strategies: &StrategyRegistry,
    stash: &Stash,
    patch: PatchVersion,
) -> Vec<Candidate> {
    let mut out: Vec<Candidate> = Vec::new();

    // ------- Rule-emitted candidates -------------------------------------
    for r in poc2_rules::evaluate_with_ctx(rules, item, ctx) {
        let action = from_rule_action(&r.suggestion.action);
        if !is_action_affordable(&action, stash) {
            continue;
        }
        let confidence = r.rule.confidence;
        let source = RecommendationSource::Rule {
            id: r.rule.id.0.clone(),
            confidence,
        };
        let prior = match confidence {
            poc2_rules::Confidence::Verified => 0.9,
            poc2_rules::Confidence::Community => 0.7,
            poc2_rules::Confidence::Experimental => 0.5,
        };
        out.push(Candidate {
            action,
            source,
            prior,
            priority: r.suggestion.priority,
            rationale: r.suggestion.note.clone(),
        });
    }

    // ------- Strategy-emitted candidates ---------------------------------
    let class = poc2_engine::ids::ItemClassId::from(item.base.as_str());
    for strategy in strategies.for_class(&class, patch) {
        if !eval_all(&strategy.preconditions, item, ctx) {
            continue;
        }
        // Walk past any leading noop / sequence / branch / loop steps so
        // strategies whose first step is a sanity-check noop (e.g.,
        // 3xt1-es-body-armour's S1-validate-base) still surface their
        // first actionable currency move as the depth-1 candidate.
        let Some((step, action)) = first_actionable_step(strategy) else {
            continue;
        };
        if !is_action_affordable(&action, stash) {
            continue;
        }
        let prior = match strategy.confidence {
            poc2_strategies::Confidence::Verified => 0.9,
            poc2_strategies::Confidence::Community => 0.7,
            poc2_strategies::Confidence::Experimental => 0.5,
        };
        let priority = match strategy.expected_success_prob {
            Some((_, hi)) => (hi * 255.0).round() as u32,
            None => 100,
        };
        out.push(Candidate {
            action,
            source: RecommendationSource::Strategy {
                id: strategy.id.0.clone(),
                step: step.id.0.clone(),
            },
            prior,
            priority,
            rationale: strategy.name.clone(),
        });
    }

    // ------- Heuristic fallback ------------------------------------------
    if out.is_empty() {
        for c in heuristic_fallback(item) {
            if is_action_affordable(&c.action, stash) {
                out.push(c);
            }
        }
    }

    out
}

/// Apply the engine precondition gate, dedup by action, and sort by
/// priority. Shared tail of both candidate-generator entry points.
fn finalize_candidates(
    mut out: Vec<Candidate>,
    item: &Item,
    resolver: &dyn CurrencyResolver,
) -> Vec<Candidate> {
    out.retain(|c| passes_engine_preconditions(&c.action, item, resolver));
    out.sort_by_key(|c| std::cmp::Reverse(c.priority));
    let mut seen: ahash::AHashSet<AdvisorAction> = ahash::AHashSet::new();
    out.retain(|c| seen.insert(c.action.clone()));
    out
}

/// Phase B.2 — emit Divine + Fracture recommendations when an item
/// already carries a target-concept mod at the goal's `min_tier` whose
/// roll is at the high end of its range. This is the
/// "T1 keeper at max → fracture before the next risky step" hint.
fn tier_fix_candidates(item: &Item, goal: &Goal, registry: &ModRegistry) -> Vec<Candidate> {
    let mut out = Vec::new();

    // For each visible non-fractured mod on the item, check if it
    // satisfies any target spec. If yes, decide between Divine (push to
    // max) and Fracturing (lock at max).
    for roll in item.prefixes.iter().chain(item.suffixes.iter()) {
        if roll.is_fractured {
            continue;
        }
        let Some(def) = registry.get(&roll.mod_id) else {
            continue;
        };
        let satisfies = goal
            .target
            .prefixes
            .iter()
            .chain(goal.target.suffixes.iter())
            .any(|spec| {
                let concept_match = if let Some(c) = &spec.concept {
                    def.concept_set.iter().any(|x| x == c)
                } else if !spec.concept_any.is_empty() {
                    def.concept_set
                        .iter()
                        .any(|x| spec.concept_any.iter().any(|y| x == y))
                } else {
                    false
                };
                concept_match && (spec.allow_hybrid || !def.is_hybrid())
            });
        if !satisfies {
            continue;
        }

        let percent = roll_max_percent(roll, def);
        if percent.is_none() {
            continue;
        }
        let percent = percent.unwrap();

        if percent < 0.95 {
            // Below max — Divine to push values higher.
            out.push(Candidate {
                action: AdvisorAction::ApplyCurrency {
                    currency: CurrencyId::from("DivineOrb"),
                    omens: vec![],
                },
                source: RecommendationSource::Heuristic {
                    name: "tier-fix-divine".into(),
                },
                prior: 0.7,
                priority: 70,
                rationale: format!(
                    "Keeper '{}' rolled at {:.0}% of max — Divine refines toward T1 max value.",
                    def.id.as_str(),
                    percent * 100.0
                ),
            });
        } else if can_fracture(item) {
            // At max — Fracture locks the keeper before the next risky step.
            out.push(Candidate {
                action: AdvisorAction::ApplyCurrency {
                    currency: CurrencyId::from("FracturingOrb"),
                    omens: vec![],
                },
                source: RecommendationSource::Heuristic {
                    name: "tier-fix-fracture".into(),
                },
                prior: 0.85,
                priority: 95,
                rationale: format!(
                    "Keeper '{}' is max-rolled — Fracture before risky steps to lock the keeper.",
                    def.id.as_str(),
                ),
            });
        }
    }
    out
}

/// Returns the roll's distance from min/max as a `[0.0, 1.0]` fraction
/// (1.0 = max-rolled). Returns `None` when the mod has no stat ranges
/// or all stats are degenerate (min == max), since "tier-fix" doesn't
/// apply to discrete-valued mods.
fn roll_max_percent(roll: &ModRoll, def: &poc2_engine::ModDefinition) -> Option<f64> {
    if roll.values.is_empty() || def.stats.is_empty() {
        return None;
    }
    let mut sum = 0.0;
    let mut samples = 0usize;
    for (val, stat) in roll.values.iter().zip(def.stats.iter()) {
        let span = stat.max - stat.min;
        if span.abs() < 1e-12 {
            continue;
        }
        let pct = ((val - stat.min) / span).clamp(0.0, 1.0);
        sum += pct;
        samples += 1;
    }
    if samples == 0 {
        None
    } else {
        Some(sum / samples as f64)
    }
}

/// Fracturing requires ≥ 4 visible mods on a Rare item.
fn can_fracture(item: &Item) -> bool {
    if item.rarity != Rarity::Rare {
        return false;
    }
    let visible = item.prefixes.len() + item.suffixes.len();
    visible >= 4
        && item
            .prefixes
            .iter()
            .chain(item.suffixes.iter())
            .all(|m| !m.is_fractured)
}

/// Phase B.6 — enumerate legal `(bone, omen)` reveal pairs and emit
/// one Candidate per pair so the user sees explicit options ranked by
/// `expected_progress / (bone_price + omen_price)`.
///
/// The bone × omen catalogue is small enough that emitting the full
/// product is cheap — a few dozen pairs total — and the planner's
/// downstream variance/cost penalties handle ranking. The
/// candidate's `prefer` list inherits from the goal's target
/// concepts so the simulator picks consistently.
fn omen_aware_reveal_candidates(item: &Item, goal: Option<&Goal>) -> Vec<Candidate> {
    let mut out = Vec::new();
    let prefer = goal_prefer_concepts(goal);

    // Bones that can apply to any class. Real bone selection is class-
    // gated by the engine's `Bone::can_apply_to`; we emit the catalogue
    // here and let the engine reject mismatches at apply time.
    let bones: &[&str] = &[
        "GnawedRib",
        "GnawedJawbone",
        "GnawedCollarbone",
        "PreservedRib",
        "PreservedJawbone",
        "PreservedCollarbone",
        "PreservedCranium",
        "AncientRib",
        "AncientJawbone",
        "AncientCollarbone",
    ];

    // Omens that meaningfully condition the reveal pool. The plan
    // enumerates these in §11.3:
    //   Sinistral/Dextral Necromancy → force prefix/suffix
    //   Blackblooded → unlock Amanamu pool
    //   Liege         → unlock Kurgal pool
    //   Sovereign     → unlock Ulaman pool
    //   EchoesOfTheAbyss → reveal yields two mods
    let omens: &[Option<&str>] = &[
        None,
        Some("OmenOfSinistralNecromancy"),
        Some("OmenOfDextralNecromancy"),
        Some("OmenOfBlackblooded"),
        Some("OmenOfTheLiege"),
        Some("OmenOfTheSovereign"),
        Some("OmenOfEchoesOfTheAbyss"),
    ];

    for bone in bones {
        for omen in omens {
            let bone_id = CurrencyId::from(*bone);
            let omen_id = omen.map(poc2_engine::ids::OmenId::from);
            let use_abyssal_echoes = matches!(omen, Some("OmenOfEchoesOfTheAbyss"));
            // Rationale text composed from bone + omen names.
            let rationale = match omen {
                Some(o) => format!(
                    "Reveal with {bone} bone gated by {o}; pool biased toward goal concepts.",
                ),
                None => format!("Reveal with {bone} bone — unconditioned reveal."),
            };
            // Prior favours omen-conditioned reveals by a small margin
            // because they materially shift the reveal distribution.
            let prior = if omen.is_some() { 0.6 } else { 0.4 };
            // Priority lower than rules / strategies so the catalogue
            // doesn't dominate when an explicit reveal rule is firing.
            let priority = if omen.is_some() { 60 } else { 40 };
            out.push(Candidate {
                action: AdvisorAction::Reveal {
                    prefer: prefer.clone(),
                    use_abyssal_echoes,
                    min_acceptable: None,
                    abandon_if_no_match: false,
                    bone: Some(bone_id),
                    omen: omen_id,
                },
                source: RecommendationSource::Heuristic {
                    name: "phase-b6-bone-omen-pair".into(),
                },
                prior,
                priority,
                rationale,
            });
        }
    }

    // Avoid emitting the full Cartesian on every node — keep at most
    // the top 8 pairs ranked by prior to prevent the UI from being
    // flooded. The planner's score then ranks the survivors.
    out.sort_by(|a, b| {
        b.prior
            .partial_cmp(&a.prior)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    out.truncate(8);

    // Suppress completely if the item doesn't carry the expected
    // hidden-desecrated state — the caller already gates on that, but
    // keeping the assertion local makes the helper safe to call from
    // tests.
    if item.hidden_desecrated.is_none() {
        return Vec::new();
    }
    out
}

/// Phase B.4-loop — detect the "Rare with N mods, chaos until the goal
/// is met" pattern. When applicable, emit a single Candidate carrying
/// an `AdvisorAction::Recurring` whose inner sequence is `[ChaosOrb]`.
/// The planner's loop-collapse pass refines the iteration estimate.
///
/// Conditions for emission:
/// - Item is Rare (Chaos's only legal rarity).
/// - Item has at least one rolled mod (Chaos requires something to remove).
/// - Goal carries at least one target spec (so we have a stop condition).
fn recurring_chaos_candidate(item: &Item, goal: &Goal) -> Option<Candidate> {
    if item.rarity != Rarity::Rare {
        return None;
    }
    if item.prefixes.is_empty() && item.suffixes.is_empty() {
        return None;
    }
    if goal.target.prefixes.is_empty() && goal.target.suffixes.is_empty() {
        return None;
    }
    let stop = stop_predicate_from_goal(goal);
    let inner = vec![AdvisorAction::ApplyCurrency {
        currency: CurrencyId::from("ChaosOrb"),
        omens: vec![],
    }];
    Some(Candidate {
        action: AdvisorAction::Recurring { inner, stop },
        source: RecommendationSource::Heuristic {
            name: "phase-b4-recurring-chaos".into(),
        },
        prior: 0.55,
        priority: 75,
        rationale: "Chaos-spam loop until the goal is reached.".into(),
    })
}

/// Lift a `Goal` into a [`StopPredicate`] usable as the exit condition
/// for a recurring step. Reuses concept + tier + affix info from the
/// target specs verbatim.
pub(crate) fn stop_predicate_from_goal(goal: &Goal) -> StopPredicate {
    let mut concepts = Vec::new();
    let mut push = |spec: &poc2_strategies::TargetSpec, affix: poc2_engine::AffixType| {
        let target_concept = spec
            .concept
            .clone()
            .or_else(|| spec.concept_any.first().cloned());
        if let Some(c) = target_concept {
            concepts.push(ConceptCriterion {
                concept: c,
                min_tier: spec.min_tier.unwrap_or(1),
                affix: Some(affix),
            });
        }
    };
    for spec in &goal.target.prefixes {
        push(spec, poc2_engine::AffixType::Prefix);
    }
    for spec in &goal.target.suffixes {
        push(spec, poc2_engine::AffixType::Suffix);
    }
    StopPredicate {
        concepts,
        max_mods: None,
    }
}

fn goal_prefer_concepts(goal: Option<&Goal>) -> Vec<poc2_engine::ConceptId> {
    let Some(goal) = goal else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for spec in goal
        .target
        .prefixes
        .iter()
        .chain(goal.target.suffixes.iter())
    {
        if let Some(c) = &spec.concept {
            if !out.contains(c) {
                out.push(c.clone());
            }
        }
        for c in &spec.concept_any {
            if !out.contains(c) {
                out.push(c.clone());
            }
        }
    }
    out
}

/// Walk a strategy's step graph from `entry()` forward, skipping
/// non-actionable steps (Noop / Sequence / Branch / LoopUntil), until
/// we find a step whose `action` lifts cleanly into an
/// [`AdvisorAction`] via [`from_strategy_action`]. Follows
/// `on_success` chains and bails after a small number of hops to
/// avoid pathological graphs.
///
/// Returns the first actionable `(step, lifted_action)` pair, or
/// `None` when the entire graph is non-actionable.
fn first_actionable_step(strategy: &Strategy) -> Option<(&Step, AdvisorAction)> {
    const MAX_HOPS: usize = 16;
    let mut current = strategy.entry()?;
    for _ in 0..MAX_HOPS {
        if let Some(action) = from_strategy_action(&current.action) {
            return Some((current, action));
        }
        // Non-actionable step: follow `on_success` to the next step
        // (the failure branch is irrelevant here because we haven't
        // simulated anything yet — we're just looking for a node we
        // can recommend).
        let next_id = current.on_success.as_ref()?;
        current = strategy.step(next_id)?;
    }
    None
}

/// Stash-affordability filter. Non-currency actions are always affordable.
fn is_action_affordable(action: &AdvisorAction, stash: &Stash) -> bool {
    match action {
        AdvisorAction::ApplyCurrency { currency, omens } => stash.can_afford(currency, omens),
        _ => true,
    }
}

/// Heuristic fallback set when nothing else fires. Intentionally small —
/// these are the "obvious next moves" given an item's rarity and slot
/// fill state.
fn heuristic_fallback(item: &Item) -> Vec<Candidate> {
    let mut out = Vec::new();
    let mk = |currency: &str, name: &str, prior: f64, priority: u32, rationale: &str| Candidate {
        action: AdvisorAction::ApplyCurrency {
            currency: CurrencyId::from(currency),
            omens: vec![],
        },
        source: RecommendationSource::Heuristic { name: name.into() },
        prior,
        priority,
        rationale: rationale.into(),
    };

    match item.rarity {
        Rarity::Normal => {
            out.push(mk(
                "OrbOfTransmutation",
                "fallback-transmute-on-normal",
                0.5,
                90,
                "Normal item: Transmute promotes to Magic.",
            ));
        }
        Rarity::Magic => {
            if item.prefixes.is_empty() || item.suffixes.is_empty() {
                out.push(mk(
                    "OrbOfAugmentation",
                    "fallback-aug-on-magic",
                    0.5,
                    85,
                    "Magic with empty slot: Augment fills it.",
                ));
            }
            out.push(mk(
                "RegalOrb",
                "fallback-regal-on-magic",
                0.5,
                80,
                "Magic with both slots filled: Regal promotes to Rare.",
            ));
        }
        Rarity::Rare => {
            // No clear default — surface guidance instead of guessing.
            out.push(Candidate {
                action: AdvisorAction::Guidance {
                    note: "Rare item; further moves depend on goal.".into(),
                },
                source: RecommendationSource::Heuristic {
                    name: "fallback-rare-guidance".into(),
                },
                prior: 0.3,
                priority: 50,
                rationale: "No matching rule fired; strategy library is silent.".into(),
            });
        }
        Rarity::Unique => {}
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use poc2_engine::ids::ItemClassId;
    use poc2_engine::item::{QualityKind, Rarity};
    use poc2_engine::registry::ModRegistry;
    use smallvec::smallvec;

    fn empty_item(rarity: Rarity) -> Item {
        Item {
            base: ItemClassId::from("BodyArmour").as_str().into(),
            ilvl: 82,
            rarity,
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

    fn default_resolver() -> poc2_engine::DefaultCurrencyResolver {
        poc2_engine::DefaultCurrencyResolver::new()
    }

    #[test]
    fn rules_fire_for_normal_with_seed_rule() {
        let reg = ModRegistry::from_mods(vec![]);
        let rules = RuleSet::from_rules(poc2_rules::seed_rules());
        let strategies = StrategyRegistry::default();
        let stash = Stash::unlimited();
        let resolver = default_resolver();
        let item = empty_item(Rarity::Normal);
        let ctx = PredicateContext::new(&reg).with_stash(&stash);
        let cands = generate_candidates(
            &item,
            &ctx,
            &rules,
            &strategies,
            &resolver,
            &stash,
            PatchVersion::PATCH_0_4_0,
        );
        assert!(!cands.is_empty(), "rules should fire on Normal ilvl 82");
        // Highest-priority candidate should be the Perfect Transmute (R001).
        let top = &cands[0];
        assert!(
            matches!(&top.source, RecommendationSource::Rule { id, .. } if id.starts_with("R001"))
        );
    }

    #[test]
    fn fallback_emits_when_no_rules_match() {
        // Sanctified items should bypass most rules but emit at least guidance.
        let reg = ModRegistry::from_mods(vec![]);
        let rules = RuleSet::default();
        let strategies = StrategyRegistry::default();
        let stash = Stash::unlimited();
        let resolver = default_resolver();
        let mut item = empty_item(Rarity::Magic);
        item.prefixes.clear();
        let ctx = PredicateContext::new(&reg).with_stash(&stash);
        let cands = generate_candidates(
            &item,
            &ctx,
            &rules,
            &strategies,
            &resolver,
            &stash,
            PatchVersion::PATCH_0_4_0,
        );
        assert!(!cands.is_empty());
        // Should suggest Augment.
        assert!(cands
            .iter()
            .any(|c| matches!(&c.source, RecommendationSource::Heuristic { .. })));
    }

    #[test]
    fn stash_filters_unavailable_actions() {
        let reg = ModRegistry::from_mods(vec![]);
        let rules = RuleSet::from_rules(poc2_rules::seed_rules());
        let strategies = StrategyRegistry::default();
        // Empty stash → no affordable actions.
        let stash = Stash::new();
        let resolver = default_resolver();
        let item = empty_item(Rarity::Normal);
        let ctx = PredicateContext::new(&reg).with_stash(&stash);
        let cands = generate_candidates(
            &item,
            &ctx,
            &rules,
            &strategies,
            &resolver,
            &stash,
            PatchVersion::PATCH_0_4_0,
        );
        // Some candidates may still emerge as Stop / Abandon / Guidance.
        for c in &cands {
            assert!(
                !c.action.is_mutating()
                    || stash.can_afford(
                        c.action.currency_id().expect("mutating without currency"),
                        c.action.omens()
                    )
            );
        }
    }

    #[test]
    fn cost_spent_predicate_threads_through_ctx() {
        // A rule that fires only when CostSpent > 5 div should fire when
        // the ctx carries cost > 5 and not fire when it doesn't.
        use poc2_engine::ids::CurrencyId;
        use poc2_strategies::{CmpOp, FloatValuePredicate, ItemPredicate};

        let reg = ModRegistry::from_mods(vec![]);
        let stash = Stash::unlimited();
        let strategies = StrategyRegistry::default();
        let item = empty_item(Rarity::Normal);

        // Build a tiny ruleset: one rule with CostSpent > 5 → Guidance.
        let rule = poc2_rules::Rule {
            id: poc2_rules::RuleId::from("test-cost-rule"),
            category: poc2_rules::Category::Budget,
            when: ItemPredicate::CostSpent(FloatValuePredicate {
                op: CmpOp::Gt,
                value: 5.0,
            }),
            then: smallvec::smallvec![poc2_rules::Suggestion {
                action: poc2_rules::SuggestionAction::Abandon {
                    reason: "budget exceeded".into(),
                },
                note: "test".into(),
                priority: 100,
                tag: None,
            }],
            explanation: "test".into(),
            source: "test".into(),
            confidence: poc2_rules::Confidence::Verified,
        };
        let _ = CurrencyId::from("ChaosOrb"); // silence unused warning
        let rules = RuleSet::from_rules(vec![rule]);

        let resolver = default_resolver();
        let cheap_ctx = PredicateContext::new(&reg)
            .with_stash(&stash)
            .with_cost(2.0);
        let cands_cheap = generate_candidates(
            &item,
            &cheap_ctx,
            &rules,
            &strategies,
            &resolver,
            &stash,
            PatchVersion::PATCH_0_4_0,
        );
        assert!(
            !cands_cheap
                .iter()
                .any(|c| matches!(c.action, AdvisorAction::Abandon { .. })),
            "abandon rule should not fire below threshold"
        );

        let pricey_ctx = PredicateContext::new(&reg)
            .with_stash(&stash)
            .with_cost(10.0);
        let cands_pricey = generate_candidates(
            &item,
            &pricey_ctx,
            &rules,
            &strategies,
            &resolver,
            &stash,
            PatchVersion::PATCH_0_4_0,
        );
        assert!(
            cands_pricey
                .iter()
                .any(|c| matches!(c.action, AdvisorAction::Abandon { .. })),
            "abandon rule should fire above threshold"
        );
    }
}
