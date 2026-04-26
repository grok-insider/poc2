//! Seed rule catalogue — 15 hand-coded heuristics covering the most
//! actionable rules from /docs/34-heuristics-rulebook.md.
//!
//! This is a starting set, not the full ~120-rule library. The remaining
//! rules will be encoded as data-driven TOML files (one per rule)
//! shipped in the data bundle.

use poc2_engine::ids::CurrencyId;
use poc2_engine::item::{AffixType, Rarity};
use smallvec::smallvec;

use crate::rule::{Category, Confidence, Rule, RuleId, Suggestion, SuggestionAction};
use poc2_strategies::{CmpOp, ItemPredicate, ValuePredicate};

/// Build the seed rule list.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn seed_rules() -> Vec<Rule> {
    let mut out = Vec::new();

    // ---- Rarity-progression suggestions ---------------------------------

    out.push(Rule {
        id: RuleId::from("R001-perfect-transmute-on-normal"),
        category: Category::BaseSelection,
        when: ItemPredicate::All(vec![
            ItemPredicate::Rarity(Rarity::Normal),
            ItemPredicate::Ilvl(ValuePredicate {
                op: CmpOp::Gte,
                value: 82,
            }),
            ItemPredicate::Corrupted(false),
            ItemPredicate::Mirrored(false),
        ]),
        then: smallvec![Suggestion {
            action: SuggestionAction::ApplyCurrency {
                currency: CurrencyId::from("PerfectOrbOfTransmutation"),
                omens: vec![],
            },
            note: "Normal ilvl 82 base. Perfect Transmute guarantees a required-level >= 70 mod."
                .into(),
            priority: 200,
        }],
        explanation: "Normal -> Magic step uses Perfect Transmutation when budget allows.".into(),
        source: "Tarke apprentice blueprint".into(),
        confidence: Confidence::Community,
    });

    out.push(Rule {
        id: RuleId::from("R002-augment-magic-1-mod"),
        category: Category::Other,
        when: ItemPredicate::All(vec![
            ItemPredicate::Rarity(Rarity::Magic),
            ItemPredicate::AffixCount {
                affix: AffixType::Prefix,
                count: ValuePredicate {
                    op: CmpOp::Lte,
                    value: 1,
                },
            },
            ItemPredicate::AffixCount {
                affix: AffixType::Suffix,
                count: ValuePredicate {
                    op: CmpOp::Lte,
                    value: 0,
                },
            },
        ]),
        then: smallvec![Suggestion {
            action: SuggestionAction::ApplyCurrency {
                currency: CurrencyId::from("PerfectOrbOfAugmentation"),
                omens: vec![],
            },
            note:
                "Magic with one prefix and an empty suffix. Perfect Augment fills the empty slot."
                    .into(),
            priority: 180,
        }],
        explanation: "Aug fills the open Magic slot.".into(),
        source: "Mosey beginner-to-apprentice".into(),
        confidence: Confidence::Community,
    });

    out.push(Rule {
        id: RuleId::from("R003-regal-ready"),
        category: Category::Other,
        when: ItemPredicate::All(vec![
            ItemPredicate::Rarity(Rarity::Magic),
            ItemPredicate::AffixCount {
                affix: AffixType::Prefix,
                count: ValuePredicate {
                    op: CmpOp::Eq,
                    value: 1,
                },
            },
            ItemPredicate::AffixCount {
                affix: AffixType::Suffix,
                count: ValuePredicate {
                    op: CmpOp::Eq,
                    value: 1,
                },
            },
        ]),
        then: smallvec![Suggestion {
            action: SuggestionAction::ApplyCurrency {
                currency: CurrencyId::from("PerfectRegalOrb"),
                omens: vec![],
            },
            note: "Magic with both slots filled. Perfect Regal promotes to Rare with a third high-tier mod.".into(),
            priority: 190,
        }],
        explanation: "Regal promotes Magic to Rare.".into(),
        source: "Tarke apprentice blueprint".into(),
        confidence: Confidence::Community,
    });

    // ---- Fracture timing -------------------------------------------------

    out.push(Rule {
        id: RuleId::from("R010-fracture-at-4-mods"),
        category: Category::Fracture,
        when: ItemPredicate::All(vec![
            ItemPredicate::Rarity(Rarity::Rare),
            ItemPredicate::AffixCount {
                affix: AffixType::Prefix,
                count: ValuePredicate {
                    op: CmpOp::Gte,
                    value: 3,
                },
            },
            ItemPredicate::HasHiddenDesecrated(true),
            ItemPredicate::HasFractured(false),
        ]),
        then: smallvec![Suggestion {
            action: SuggestionAction::ApplyCurrency {
                currency: CurrencyId::from("FracturingOrb"),
                omens: vec![],
            },
            note: "3 visible prefixes + 1 hidden desecrated = 4 mods. 1/3 chance to lock a target prefix; consider Hinekora's Lock first.".into(),
            priority: 250,
        }],
        explanation: "Fracture-eligibility hits its sweet spot at exactly 4 mods.".into(),
        source: "/docs/34-heuristics-rulebook.md sec. 2".into(),
        confidence: Confidence::Verified,
    });

    out.push(Rule {
        id: RuleId::from("R011-hinekoras-lock-before-fracture"),
        category: Category::HinekoraLock,
        when: ItemPredicate::All(vec![
            ItemPredicate::Rarity(Rarity::Rare),
            ItemPredicate::AffixCount {
                affix: AffixType::Prefix,
                count: ValuePredicate {
                    op: CmpOp::Gte,
                    value: 3,
                },
            },
            ItemPredicate::HasHiddenDesecrated(true),
            ItemPredicate::HasFractured(false),
            ItemPredicate::HasHinekoraLock(false),
        ]),
        then: smallvec![Suggestion {
            action: SuggestionAction::ApplyHinekorasLock,
            note: "Apply Hinekora's Lock before Fracture so you can preview which mod gets locked."
                .into(),
            priority: 260,
        }],
        explanation: "Lock-before-risky-step is a high-value pattern at fracture time.".into(),
        source: "Mosey, Tarke streams".into(),
        confidence: Confidence::Community,
    });

    // ---- Recovery / abandon ----------------------------------------------

    out.push(Rule {
        id: RuleId::from("R020-abandon-corrupted-without-target"),
        category: Category::Abandonment,
        when: ItemPredicate::Corrupted(true),
        then: smallvec![Suggestion {
            action: SuggestionAction::Abandon {
                reason: "Item is corrupted; further crafting paths are restricted.".into(),
            },
            note: "Corrupted items are mostly read-only. Only Architect's Orb / Vaal Cultivation Orb can modify them.".into(),
            priority: 50,
        }],
        explanation: "Corrupted state is largely a one-way door.".into(),
        source: "/docs/11-game-mechanics.md".into(),
        confidence: Confidence::Verified,
    });

    out.push(Rule {
        id: RuleId::from("R021-sanctified-stop"),
        category: Category::StopVsContinue,
        when: ItemPredicate::Sanctified(true),
        then: smallvec![Suggestion {
            action: SuggestionAction::StopAndSell,
            note: "Sanctified items are uncraftable. Sell or equip.".into(),
            priority: 50,
        }],
        explanation: "Sanctification is the natural exit for mirror-tier crafts.".into(),
        source: "/docs/11-game-mechanics.md".into(),
        confidence: Confidence::Verified,
    });

    // ---- Vaal corruption finishing ---------------------------------------

    out.push(Rule {
        id: RuleId::from("R030-vaal-finish-with-corruption-omen"),
        category: Category::Vaal,
        when: ItemPredicate::All(vec![
            ItemPredicate::Rarity(Rarity::Rare),
            ItemPredicate::Corrupted(false),
            ItemPredicate::Mirrored(false),
            ItemPredicate::AffixCount {
                affix: AffixType::Prefix,
                count: ValuePredicate {
                    op: CmpOp::Eq,
                    value: 3,
                },
            },
            ItemPredicate::AffixCount {
                affix: AffixType::Suffix,
                count: ValuePredicate {
                    op: CmpOp::Eq,
                    value: 3,
                },
            },
        ]),
        then: smallvec![Suggestion {
            action: SuggestionAction::ApplyCurrency {
                currency: CurrencyId::from("VaalOrb"),
                omens: vec![poc2_engine::ids::OmenId::from("OmenOfCorruption")],
            },
            note: "Item is fully crafted. Vaal Orb + Omen of Corruption removes the no-change outcome.".into(),
            priority: 80,
        }],
        explanation: "Mirror-tier finishing step.".into(),
        source: "Tarke late-league streams".into(),
        confidence: Confidence::Community,
    });

    // ---- Bone + Necromancy heuristics ------------------------------------

    out.push(Rule {
        id: RuleId::from("R040-bone-with-dextral-necromancy"),
        category: Category::ExaltVsDesecrate,
        when: ItemPredicate::All(vec![
            ItemPredicate::Rarity(Rarity::Rare),
            ItemPredicate::AffixCount {
                affix: AffixType::Prefix,
                count: ValuePredicate {
                    op: CmpOp::Eq,
                    value: 3,
                },
            },
            ItemPredicate::AffixCount {
                affix: AffixType::Suffix,
                count: ValuePredicate {
                    op: CmpOp::Lte,
                    value: 1,
                },
            },
            ItemPredicate::HasHiddenDesecrated(false),
            ItemPredicate::HasFractured(false),
        ]),
        then: smallvec![Suggestion {
            action: SuggestionAction::ApplyCurrency {
                currency: CurrencyId::from("PreservedRib"),
                omens: vec![poc2_engine::ids::OmenId::from("OmenOfDextralNecromancy")],
            },
            note: "Prefixes complete. Preserved Rib + Dextral Necromancy adds a hidden suffix while preserving the open suffix slot for free choice at reveal.".into(),
            priority: 220,
        }],
        explanation: "User's worked-example pattern: bone hidden mod into the side you control.".into(),
        source: "User project-author worked example".into(),
        confidence: Confidence::Verified,
    });

    out.push(Rule {
        id: RuleId::from("R041-reveal-with-abyssal-echoes"),
        category: Category::ExaltVsDesecrate,
        when: ItemPredicate::HasHiddenDesecrated(true),
        then: smallvec![Suggestion {
            action: SuggestionAction::Reveal,
            note: "Reveal at the Well of Souls. Pair with Omen of Abyssal Echoes for a 3+3 choice."
                .into(),
            priority: 230,
        }],
        explanation: "After bone, reveal converts the hidden mod into a real one.".into(),
        source: "User worked example, step 8".into(),
        confidence: Confidence::Verified,
    });

    // ---- Whittle / Annul nuance -----------------------------------------

    out.push(Rule {
        id: RuleId::from("R050-whittle-when-low-tier-prefix-survives"),
        category: Category::WhittleVsAnnul,
        when: ItemPredicate::All(vec![
            ItemPredicate::Rarity(Rarity::Rare),
            ItemPredicate::HasFractured(true),
            ItemPredicate::AffixCount {
                affix: AffixType::Prefix,
                count: ValuePredicate {
                    op: CmpOp::Gte,
                    value: 2,
                },
            },
        ]),
        then: smallvec![Suggestion {
            action: SuggestionAction::ApplyCurrency {
                currency: CurrencyId::from("ChaosOrb"),
                omens: vec![poc2_engine::ids::OmenId::from("OmenOfWhittling")],
            },
            note: "Fracture is locked in. Chaos + Whittling surgically removes the lowest-required-level mod.".into(),
            priority: 210,
        }],
        explanation: "Whittling targets cleanup precisely.".into(),
        source: "Goratha live-craft".into(),
        confidence: Confidence::Community,
    });

    // ---- Magic-stage exit (sell early) ----------------------------------

    out.push(Rule {
        id: RuleId::from("R060-magic-stage-exit-on-2-good-mods"),
        category: Category::Pricing,
        when: ItemPredicate::All(vec![
            ItemPredicate::Rarity(Rarity::Magic),
            ItemPredicate::AffixCount {
                affix: AffixType::Prefix,
                count: ValuePredicate {
                    op: CmpOp::Eq,
                    value: 1,
                },
            },
            ItemPredicate::AffixCount {
                affix: AffixType::Suffix,
                count: ValuePredicate {
                    op: CmpOp::Eq,
                    value: 1,
                },
            },
        ]),
        then: smallvec![Suggestion {
            action: SuggestionAction::Guidance,
            note: "Magic with two desirable mods. Consider listing the magic-stage item — many builds buy these for further crafting.".into(),
            priority: 60,
        }],
        explanation: "Magic-stage exit captures profit without risking Regal/Exalt RNG.".into(),
        source: "Goratha profit-craft analysis".into(),
        confidence: Confidence::Community,
    });

    // ---- Crystallisation suggestion -------------------------------------

    out.push(Rule {
        id: RuleId::from("R070-essence-with-crystallisation-suffix-swap"),
        category: Category::ExaltVsDesecrate,
        when: ItemPredicate::All(vec![
            ItemPredicate::Rarity(Rarity::Rare),
            ItemPredicate::HasFractured(true),
            ItemPredicate::AffixCount {
                affix: AffixType::Prefix,
                count: ValuePredicate {
                    op: CmpOp::Eq,
                    value: 3,
                },
            },
            ItemPredicate::AffixCount {
                affix: AffixType::Suffix,
                count: ValuePredicate {
                    op: CmpOp::Gte,
                    value: 1,
                },
            },
            ItemPredicate::Corrupted(false),
        ]),
        then: smallvec![Suggestion {
            action: SuggestionAction::ApplyCurrency {
                currency: CurrencyId::from("PerfectEssenceOfSeeking"),
                omens: vec![poc2_engine::ids::OmenId::from("OmenOfDextralCrystallisation")],
            },
            note: "All 3 prefixes present and at least one suffix. Perfect Essence + Dextral Crystallisation swaps a bad suffix for the Seeking suffix without touching prefixes.".into(),
            priority: 200,
        }],
        explanation: "User's worked-example culminating step.".into(),
        source: "User project-author worked example".into(),
        confidence: Confidence::Verified,
    });

    // ---- Budget / sanity guidance ---------------------------------------

    out.push(Rule {
        id: RuleId::from("R080-respect-fracture-immutability"),
        category: Category::Recovery,
        when: ItemPredicate::HasFractured(true),
        then: smallvec![Suggestion {
            action: SuggestionAction::Guidance,
            note: "Fractured mod is locked: Annul/Chaos cannot remove it; Divine cannot reroll it. Plan around it.".into(),
            priority: 70,
        }],
        explanation: "Reminder that fracture is permanent.".into(),
        source: "/docs/11-game-mechanics.md".into(),
        confidence: Confidence::Verified,
    });

    out.push(Rule {
        id: RuleId::from("R081-divine-before-fracture"),
        category: Category::Fracture,
        when: ItemPredicate::All(vec![
            ItemPredicate::Rarity(Rarity::Rare),
            ItemPredicate::AffixCount {
                affix: AffixType::Prefix,
                count: ValuePredicate {
                    op: CmpOp::Gte,
                    value: 2,
                },
            },
            ItemPredicate::HasHiddenDesecrated(true),
            ItemPredicate::HasFractured(false),
        ]),
        then: smallvec![Suggestion {
            action: SuggestionAction::ApplyCurrency {
                currency: CurrencyId::from("DivineOrb"),
                omens: vec![],
            },
            note: "Divine first to maximize values BEFORE fracture locks them.".into(),
            priority: 240,
        }],
        explanation: "Divine-then-fracture maximizes locked-mod values.".into(),
        source: "User worked example, step 7".into(),
        confidence: Confidence::Verified,
    });

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_rules_load() {
        let rules = seed_rules();
        assert!(rules.len() >= 15, "got {} rules", rules.len());
    }

    #[test]
    fn seed_rule_ids_are_unique() {
        let rules = seed_rules();
        let mut seen = std::collections::HashSet::new();
        for r in &rules {
            assert!(seen.insert(r.id.0.clone()), "duplicate rule id: {}", r.id.0);
        }
    }
}
