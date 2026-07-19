//! Seed rule catalogue — loaded from `seed_rules/*.toml`.
//!
//! Per A.5 of the v1 execution plan, the seed rules are stored as
//! TOML files (one per /docs/34 section), embedded into the binary via
//! `include_str!`. The loader walks the embedded list, parses each file,
//! and concatenates into a single `Vec<Rule>`.
//!
//! Authors add new rules by:
//! 1. Editing the appropriate `crates/rules/seed_rules/<section>.toml`.
//! 2. Bumping the section's version (commit). The next bundle build
//!    picks up the new rule via the `include_str!` re-evaluation.
//!
//! Sections (matching /docs/34-heuristics-rulebook.md):
//!   00_progression   - rarity-progression defaults (R001-R003)
//!   01_abandonment   - §1, sunk-cost / cost-vs-sale guardrails
//!   02_fracture      - §2, Fracturing Orb timing
//!   03_hinekora_lock - §3, Lock decision rules
//!   04_exalt_vs_desecrate - §4, Exalt-vs-bone choice
//!   05_whittle_vs_annul   - §5, Annul vs Whittle vs Erasure
//!   06_stop_vs_continue   - §6, partial-success exits
//!   07_pricing       - §7, listing exits
//!   08_budget        - §8, bankroll discipline
//!   09_base_selection - §9, ilvl gating
//!   10_vaal          - §10, Vaal corruption decisions
//!   11_market        - §11, league-cycle market awareness
//!   12_recovery      - §12, step-failure recoveries
//!   13_confidence_ev - §13, EV math principles
//!   99_catalysts     - jewelry-catalyst recommendations (off-spec)

use crate::loader::load_rules_str;
use crate::rule::Rule;

/// Embedded seed rule TOMLs. Tuple is `(section_name, toml_str)`.
const SEED_RULE_FILES: &[(&str, &str)] = &[
    (
        "00_progression",
        include_str!("../seed_rules/00_progression.toml"),
    ),
    (
        "01_abandonment",
        include_str!("../seed_rules/01_abandonment.toml"),
    ),
    (
        "02_fracture",
        include_str!("../seed_rules/02_fracture.toml"),
    ),
    (
        "03_hinekora_lock",
        include_str!("../seed_rules/03_hinekora_lock.toml"),
    ),
    (
        "04_exalt_vs_desecrate",
        include_str!("../seed_rules/04_exalt_vs_desecrate.toml"),
    ),
    (
        "05_whittle_vs_annul",
        include_str!("../seed_rules/05_whittle_vs_annul.toml"),
    ),
    (
        "06_stop_vs_continue",
        include_str!("../seed_rules/06_stop_vs_continue.toml"),
    ),
    ("07_pricing", include_str!("../seed_rules/07_pricing.toml")),
    ("08_budget", include_str!("../seed_rules/08_budget.toml")),
    (
        "09_base_selection",
        include_str!("../seed_rules/09_base_selection.toml"),
    ),
    ("10_vaal", include_str!("../seed_rules/10_vaal.toml")),
    ("11_market", include_str!("../seed_rules/11_market.toml")),
    (
        "12_recovery",
        include_str!("../seed_rules/12_recovery.toml"),
    ),
    (
        "13_confidence_ev",
        include_str!("../seed_rules/13_confidence_ev.toml"),
    ),
    (
        "14_genesis_tree",
        include_str!("../seed_rules/14_genesis_tree.toml"),
    ),
    (
        "99_catalysts",
        include_str!("../seed_rules/99_catalysts.toml"),
    ),
];

/// Build the seed rule list by parsing every embedded TOML file in
/// section order. Panics if an embedded file fails to parse — these
/// are compile-time-tested fixtures, so a parse failure is a build
/// bug, not a runtime condition.
#[must_use]
pub fn seed_rules() -> Vec<Rule> {
    let mut out = Vec::new();
    for (section, toml_str) in SEED_RULE_FILES {
        match load_rules_str(toml_str) {
            Ok(rules) => out.extend(rules),
            Err(e) => panic!("seed_rules section `{section}` failed to parse: {e}"),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_rules_load() {
        let rules = seed_rules();
        assert!(rules.len() >= 70, "got {} rules", rules.len());
    }

    #[test]
    fn seed_rule_ids_are_unique() {
        let rules = seed_rules();
        let mut seen = std::collections::HashSet::new();
        for r in &rules {
            assert!(seen.insert(r.id.0.clone()), "duplicate rule id: {}", r.id.0);
        }
    }

    #[test]
    fn every_section_loads_at_least_one_rule() {
        // The whole catalogue should have rules from every section.
        // We assert section non-emptiness by parsing each file directly.
        for (section, toml_str) in SEED_RULE_FILES {
            let rules = load_rules_str(toml_str)
                .unwrap_or_else(|e| panic!("section {section} parse failed: {e}"));
            assert!(
                !rules.is_empty(),
                "section `{section}` has no rules (placeholder file?)"
            );
        }
    }

    #[test]
    fn tagged_guidance_has_expected_categories() {
        // §11 (market) and §13 (ev) rules should all be tagged guidance.
        let rules = seed_rules();
        let market_rules: Vec<_> = rules
            .iter()
            .filter(|r| matches!(r.category, crate::rule::Category::Market))
            .collect();
        assert!(!market_rules.is_empty());
        for r in &market_rules {
            for s in &r.then {
                assert!(
                    s.tag.is_some(),
                    "market rule {} should have a tag (league_advice)",
                    r.id.0
                );
            }
        }

        let ev_rules: Vec<_> = rules
            .iter()
            .filter(|r| matches!(r.category, crate::rule::Category::Ev))
            .collect();
        assert!(!ev_rules.is_empty());
        for r in &ev_rules {
            for s in &r.then {
                assert!(
                    s.tag.is_some(),
                    "ev rule {} should have a tag (meta)",
                    r.id.0
                );
            }
        }
    }
}
