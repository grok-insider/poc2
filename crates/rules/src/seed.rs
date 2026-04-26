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

    // ---- Catalyst rules (jewelry-specific) ------------------------------

    out.push(Rule {
        id: RuleId::from("R090-flesh-catalyst-on-life-rare-jewelry"),
        category: Category::Other,
        when: ItemPredicate::All(vec![
            ItemPredicate::Rarity(Rarity::Rare),
            ItemPredicate::ItemClassAny(vec![
                poc2_engine::ids::ItemClassId::from("Ring"),
                poc2_engine::ids::ItemClassId::from("Amulet"),
                poc2_engine::ids::ItemClassId::from("Belt"),
            ]),
            ItemPredicate::Corrupted(false),
            ItemPredicate::HasConcept {
                concept: poc2_engine::ConceptId::from("Life"),
                affix: None,
                min_tier: None,
            },
        ]),
        then: smallvec![Suggestion {
            action: SuggestionAction::ApplyCurrency {
                currency: CurrencyId::from("FleshCatalyst"),
                omens: vec![],
            },
            note:
                "Rare jewelry with a life mod: Flesh Catalyst boosts life-tagged mods by +5%/apply (cap 20%)."
                    .into(),
            priority: 110,
        }],
        explanation: "Catalysts multiply tag-matching mod values; quality is consumed by Catalysing Exaltation later.".into(),
        source: "/docs/33-strategy-library.md sec 18 (Catalysing Exaltation)".into(),
        confidence: Confidence::Community,
    });

    out.push(Rule {
        id: RuleId::from("R091-reaver-catalyst-on-attack-rare-jewelry"),
        category: Category::Other,
        when: ItemPredicate::All(vec![
            ItemPredicate::Rarity(Rarity::Rare),
            ItemPredicate::ItemClassAny(vec![
                poc2_engine::ids::ItemClassId::from("Ring"),
                poc2_engine::ids::ItemClassId::from("Amulet"),
            ]),
            ItemPredicate::Corrupted(false),
            ItemPredicate::HasConcept {
                concept: poc2_engine::ConceptId::from("AttackDamage"),
                affix: None,
                min_tier: None,
            },
        ]),
        then: smallvec![Suggestion {
            action: SuggestionAction::ApplyCurrency {
                currency: CurrencyId::from("ReaverCatalyst"),
                omens: vec![],
            },
            note:
                "Rare jewelry with attack-damage mods: Reaver Catalyst boosts attack-tagged mods."
                    .into(),
            priority: 110,
        }],
        explanation: "Catalysts pin quality to a tag; Reaver targets attack mods.".into(),
        source: "/docs/33-strategy-library.md sec 18".into(),
        confidence: Confidence::Community,
    });

    out.push(Rule {
        id: RuleId::from("R092-unstable-catalyst-on-caster-rare-jewelry"),
        category: Category::Other,
        when: ItemPredicate::All(vec![
            ItemPredicate::Rarity(Rarity::Rare),
            ItemPredicate::ItemClassAny(vec![
                poc2_engine::ids::ItemClassId::from("Ring"),
                poc2_engine::ids::ItemClassId::from("Amulet"),
            ]),
            ItemPredicate::Corrupted(false),
            ItemPredicate::HasConcept {
                concept: poc2_engine::ConceptId::from("SpellDamage"),
                affix: None,
                min_tier: None,
            },
        ]),
        then: smallvec![Suggestion {
            action: SuggestionAction::ApplyCurrency {
                currency: CurrencyId::from("UnstableCatalyst"),
                omens: vec![],
            },
            note: "Rare jewelry with caster mods: Unstable Catalyst boosts caster-tagged mods."
                .into(),
            priority: 110,
        }],
        explanation: "Catalyst-quality boosts spell mods on jewelry pre-Exalt slam.".into(),
        source: "/docs/33-strategy-library.md sec 18".into(),
        confidence: Confidence::Community,
    });

    // ---- Vaal-corruption finishers --------------------------------------

    out.push(Rule {
        id: RuleId::from("R100-vaal-corrupt-with-lock-on-mirror-tier"),
        category: Category::Vaal,
        when: ItemPredicate::All(vec![
            ItemPredicate::Rarity(Rarity::Rare),
            ItemPredicate::Corrupted(false),
            ItemPredicate::Mirrored(false),
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
                    op: CmpOp::Eq,
                    value: 3,
                },
            },
            ItemPredicate::HasHinekoraLock(false),
        ]),
        then: smallvec![Suggestion {
            action: SuggestionAction::ApplyHinekorasLock,
            note: "Mirror-tier item is one Vaal away from done. Lock first to preview the corruption outcome.".into(),
            priority: 230,
        }],
        explanation: "Lock-then-Vaal turns the most volatile finisher into a known outcome.".into(),
        source: "/docs/34-heuristics-rulebook.md sec 10".into(),
        confidence: Confidence::Verified,
    });

    // ---- Magic-stage Annul-Aug recovery ---------------------------------

    out.push(Rule {
        id: RuleId::from("R110-annul-magic-stage-when-one-junk-mod"),
        category: Category::WhittleVsAnnul,
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
            ItemPredicate::Corrupted(false),
        ]),
        then: smallvec![Suggestion {
            action: SuggestionAction::ApplyCurrency {
                currency: CurrencyId::from("OrbOfAnnulment"),
                omens: vec![],
            },
            note:
                "Magic with 2 mods, one junk: Annul is 50/50 to remove the junk. Cycle Aug after."
                    .into(),
            priority: 130,
        }],
        explanation: "Magic-stage Annul-Aug spam refines a base before Regal.".into(),
        source: "/docs/33-strategy-library.md sec 3".into(),
        confidence: Confidence::Community,
    });

    // ---- Side-targeted Erasure -----------------------------------------

    out.push(Rule {
        id: RuleId::from("R120-sinistral-erasure-when-prefix-side-junk"),
        category: Category::WhittleVsAnnul,
        when: ItemPredicate::All(vec![
            ItemPredicate::Rarity(Rarity::Rare),
            ItemPredicate::HasFractured(true),
            ItemPredicate::AffixCount {
                affix: AffixType::Suffix,
                count: ValuePredicate {
                    op: CmpOp::Gte,
                    value: 3,
                },
            },
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
                omens: vec![poc2_engine::ids::OmenId::from("OmenOfSinistralErasure")],
            },
            note:
                "Suffixes are locked, prefixes have junk. Sinistral Erasure removes only prefixes."
                    .into(),
            priority: 200,
        }],
        explanation: "Side-targeted erasure cleans one side without risking the other.".into(),
        source: "/docs/33-strategy-library.md sec 8".into(),
        confidence: Confidence::Community,
    });

    out.push(Rule {
        id: RuleId::from("R121-dextral-erasure-when-suffix-side-junk"),
        category: Category::WhittleVsAnnul,
        when: ItemPredicate::All(vec![
            ItemPredicate::Rarity(Rarity::Rare),
            ItemPredicate::HasFractured(true),
            ItemPredicate::AffixCount {
                affix: AffixType::Prefix,
                count: ValuePredicate {
                    op: CmpOp::Gte,
                    value: 3,
                },
            },
            ItemPredicate::AffixCount {
                affix: AffixType::Suffix,
                count: ValuePredicate {
                    op: CmpOp::Gte,
                    value: 2,
                },
            },
        ]),
        then: smallvec![Suggestion {
            action: SuggestionAction::ApplyCurrency {
                currency: CurrencyId::from("ChaosOrb"),
                omens: vec![poc2_engine::ids::OmenId::from("OmenOfDextralErasure")],
            },
            note: "Prefixes are locked, suffixes have junk. Dextral Erasure removes only suffixes."
                .into(),
            priority: 200,
        }],
        explanation: "Side-targeted erasure cleans one side without risking the other.".into(),
        source: "/docs/33-strategy-library.md sec 8".into(),
        confidence: Confidence::Community,
    });

    // ---- Light-omen Annul for desecrated cleanup ------------------------

    out.push(Rule {
        id: RuleId::from("R130-omen-of-light-on-rare-with-bad-desecrated"),
        category: Category::ExaltVsDesecrate,
        when: ItemPredicate::All(vec![
            ItemPredicate::Rarity(Rarity::Rare),
            ItemPredicate::HasHiddenDesecrated(false),
            // Has at least one revealed desecrated mod (fractured=false but
            // we lack a HasDesecrated predicate — proxy via ilvl 82 + rare
            // for now). Strategy authors can refine when the predicate
            // surface gains HasDesecrated.
        ]),
        then: smallvec![Suggestion {
            action: SuggestionAction::ApplyCurrency {
                currency: CurrencyId::from("OrbOfAnnulment"),
                omens: vec![poc2_engine::ids::OmenId::from("OmenOfLight")],
            },
            note: "If a desecrated mod is unwanted, Omen of Light + Annul removes ONLY desecrated mods.".into(),
            priority: 90,
        }],
        explanation: "Omen of Light is the keystone trick for desecrated-mod cleanup.".into(),
        source: "/docs/33-strategy-library.md sec 9".into(),
        confidence: Confidence::Community,
    });

    // ---- Sanctification finisher ----------------------------------------

    // ---- Hinekora's Lock decision rules ---------------------------------

    out.push(Rule {
        id: RuleId::from("R150-hinekora-lock-before-vaal-on-mirror-tier"),
        category: Category::HinekoraLock,
        when: ItemPredicate::All(vec![
            ItemPredicate::Rarity(Rarity::Rare),
            ItemPredicate::Corrupted(false),
            ItemPredicate::Sanctified(false),
            ItemPredicate::HasHinekoraLock(false),
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
            action: SuggestionAction::ApplyHinekorasLock,
            note: "6-mod rare ready for high-stakes finisher (Vaal/Sanctify). Lock first to preview the outcome.".into(),
            priority: 220,
        }],
        explanation: "Lock-before-finisher converts the riskiest single-click step into a known outcome.".into(),
        source: "/docs/34-heuristics-rulebook.md sec 3.1".into(),
        confidence: Confidence::Verified,
    });

    out.push(Rule {
        id: RuleId::from("R151-hinekora-cannot-be-applied-to-corrupted"),
        category: Category::HinekoraLock,
        when: ItemPredicate::All(vec![
            ItemPredicate::Corrupted(true),
            ItemPredicate::HasHinekoraLock(false),
        ]),
        then: smallvec![Suggestion {
            action: SuggestionAction::Guidance,
            note: "Hinekora's Lock cannot be applied to corrupted items.".into(),
            priority: 30,
        }],
        explanation: "Lock requires unmodifiable=false; corrupted is immune.".into(),
        source: "/docs/34-heuristics-rulebook.md sec 3.3".into(),
        confidence: Confidence::Verified,
    });

    out.push(Rule {
        id: RuleId::from("R152-hinekora-bench-recombinator-no-preview"),
        category: Category::HinekoraLock,
        when: ItemPredicate::HasHinekoraLock(true),
        then: smallvec![Suggestion {
            action: SuggestionAction::Guidance,
            note: "Lock CANNOT preview crafting bench, recombinator, beastcrafting, or Corruption Altar. Apply Lock only when the next currency is a regular orb.".into(),
            priority: 40,
        }],
        explanation: "Reminder of the Lock's coverage limits.".into(),
        source: "/docs/34-heuristics-rulebook.md sec 3.5".into(),
        confidence: Confidence::Verified,
    });

    // ---- Exalt-vs-Desecrate refinement (sec 4.x) ------------------------

    out.push(Rule {
        id: RuleId::from("R160-perfect-exalt-with-homogenising-on-isolated-rare"),
        category: Category::ExaltVsDesecrate,
        when: ItemPredicate::All(vec![
            ItemPredicate::Rarity(Rarity::Rare),
            ItemPredicate::Corrupted(false),
            ItemPredicate::Sanctified(false),
            ItemPredicate::AffixCount {
                affix: AffixType::Prefix,
                count: ValuePredicate {
                    op: CmpOp::Lte,
                    value: 2,
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
                currency: CurrencyId::from("PerfectExaltedOrb"),
                omens: vec![poc2_engine::ids::OmenId::from("OmenOfSinistralExaltation")],
            },
            note: "Suffixes complete (3/3) but prefix slot open. Sinistral Exaltation forces the slam to land prefix; Perfect Exalt guarantees min-mod-level 50.".into(),
            priority: 215,
        }],
        explanation: "Open-prefix-only is the canonical isolated Exalt setup.".into(),
        source: "/docs/34-heuristics-rulebook.md sec 4.3".into(),
        confidence: Confidence::Verified,
    });

    out.push(Rule {
        id: RuleId::from("R161-dextral-exalt-on-prefix-locked-rare"),
        category: Category::ExaltVsDesecrate,
        when: ItemPredicate::All(vec![
            ItemPredicate::Rarity(Rarity::Rare),
            ItemPredicate::Corrupted(false),
            ItemPredicate::Sanctified(false),
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
                    value: 2,
                },
            },
        ]),
        then: smallvec![Suggestion {
            action: SuggestionAction::ApplyCurrency {
                currency: CurrencyId::from("PerfectExaltedOrb"),
                omens: vec![poc2_engine::ids::OmenId::from("OmenOfDextralExaltation")],
            },
            note: "Prefixes complete; suffix slot open. Dextral Exaltation + Perfect Exalt is deterministic for slot.".into(),
            priority: 215,
        }],
        explanation: "Mirror of the Sinistral case.".into(),
        source: "/docs/34-heuristics-rulebook.md sec 4.3".into(),
        confidence: Confidence::Verified,
    });

    out.push(Rule {
        id: RuleId::from("R162-bone-on-isolated-armor-prefix"),
        category: Category::ExaltVsDesecrate,
        when: ItemPredicate::All(vec![
            ItemPredicate::Rarity(Rarity::Rare),
            ItemPredicate::ItemClass(poc2_engine::ids::ItemClassId::from("BodyArmour")),
            ItemPredicate::AffixCount {
                affix: AffixType::Suffix,
                count: ValuePredicate {
                    op: CmpOp::Eq,
                    value: 3,
                },
            },
            ItemPredicate::AffixCount {
                affix: AffixType::Prefix,
                count: ValuePredicate {
                    op: CmpOp::Lte,
                    value: 2,
                },
            },
            ItemPredicate::HasHiddenDesecrated(false),
        ]),
        then: smallvec![Suggestion {
            action: SuggestionAction::ApplyCurrency {
                currency: CurrencyId::from("AncientRib"),
                omens: vec![poc2_engine::ids::OmenId::from("OmenOfSinistralNecromancy")],
            },
            note: "Body armour + open prefix slot: Ancient Rib + Sinistral Necromancy guarantees a high-tier desecrated prefix.".into(),
            priority: 200,
        }],
        explanation: "Bone path competes with Exalt; prefer it when the desecrated mod pool is desirable.".into(),
        source: "/docs/34-heuristics-rulebook.md sec 4.4".into(),
        confidence: Confidence::Community,
    });

    out.push(Rule {
        id: RuleId::from("R163-abyssal-echoes-before-reveal"),
        category: Category::ExaltVsDesecrate,
        when: ItemPredicate::HasHiddenDesecrated(true),
        then: smallvec![Suggestion {
            action: SuggestionAction::ApplyCurrency {
                currency: CurrencyId::from("OmenOfAbyssalEchoes"),
                omens: vec![],
            },
            note: "Activate Omen of Abyssal Echoes BEFORE Reveal — gives 6 options total instead of 3.".into(),
            priority: 235,
        }],
        explanation: "Cheap insurance against bad reveal options.".into(),
        source: "/docs/34-heuristics-rulebook.md sec 4.9".into(),
        confidence: Confidence::Verified,
    });

    // ---- Whittle / Annul nuance (sec 5.x) ------------------------------

    out.push(Rule {
        id: RuleId::from("R170-perfect-chaos-with-whittling-for-guaranteed-t1"),
        category: Category::WhittleVsAnnul,
        when: ItemPredicate::All(vec![
            ItemPredicate::Rarity(Rarity::Rare),
            ItemPredicate::HasFractured(true),
            ItemPredicate::Corrupted(false),
            ItemPredicate::AffixCount {
                affix: AffixType::Prefix,
                count: ValuePredicate {
                    op: CmpOp::Gte,
                    value: 2,
                },
            },
            ItemPredicate::AffixCount {
                affix: AffixType::Suffix,
                count: ValuePredicate {
                    op: CmpOp::Gte,
                    value: 2,
                },
            },
        ]),
        then: smallvec![Suggestion {
            action: SuggestionAction::ApplyCurrency {
                currency: CurrencyId::from("PerfectChaosOrb"),
                omens: vec![poc2_engine::ids::OmenId::from("OmenOfWhittling")],
            },
            note: "Perfect Chaos + Whittling: removes the lowest-mod-level mod and adds a guaranteed min-mod-level 50 (T1) replacement.".into(),
            priority: 195,
        }],
        explanation: "Combines Whittling's targeting precision with Perfect Chaos's tier guarantee.".into(),
        source: "/docs/34-heuristics-rulebook.md sec 5.6".into(),
        confidence: Confidence::Community,
    });

    out.push(Rule {
        id: RuleId::from("R171-whittle-warning-essence-mods-low-mlvl"),
        category: Category::WhittleVsAnnul,
        when: ItemPredicate::All(vec![
            ItemPredicate::Rarity(Rarity::Rare),
            ItemPredicate::Corrupted(false),
        ]),
        then: smallvec![Suggestion {
            action: SuggestionAction::Guidance,
            note: "Warning: Essence-applied mods often have LOW mod-level despite being powerful. Whittling may unintentionally hit them.".into(),
            priority: 30,
        }],
        explanation: "VULKK rulebook safety note; pair with stat-pin in UI before commit.".into(),
        source: "/docs/34-heuristics-rulebook.md sec 5.7".into(),
        confidence: Confidence::Community,
    });

    // ---- Stop-vs-Continue (sec 6.x) -------------------------------------

    out.push(Rule {
        id: RuleId::from("R180-sell-unrevealed-4mod-jewel"),
        category: Category::Pricing,
        when: ItemPredicate::All(vec![
            ItemPredicate::ItemClass(poc2_engine::ids::ItemClassId::from("Jewel")),
            ItemPredicate::Rarity(Rarity::Rare),
            ItemPredicate::HasHiddenDesecrated(true),
            ItemPredicate::AffixCount {
                affix: AffixType::Prefix,
                count: ValuePredicate {
                    op: CmpOp::Gte,
                    value: 2,
                },
            },
        ]),
        then: smallvec![Suggestion {
            action: SuggestionAction::Guidance,
            note: "4-mod unrevealed jewels are in high demand. Consider listing without revealing — buyer prefers the gamble.".into(),
            priority: 65,
        }],
        explanation: "Per /docs/34 sec 6.5 — unrevealed jewel exit captures premium.".into(),
        source: "/docs/34-heuristics-rulebook.md sec 6.5".into(),
        confidence: Confidence::Community,
    });

    out.push(Rule {
        id: RuleId::from("R181-hero-mod-on-mid-rest-sell-as-base"),
        category: Category::Pricing,
        when: ItemPredicate::All(vec![
            ItemPredicate::Rarity(Rarity::Rare),
            ItemPredicate::HasFractured(true),
            ItemPredicate::Corrupted(false),
            ItemPredicate::AffixCount {
                affix: AffixType::Prefix,
                count: ValuePredicate {
                    op: CmpOp::Lte,
                    value: 2,
                },
            },
            ItemPredicate::AffixCount {
                affix: AffixType::Suffix,
                count: ValuePredicate {
                    op: CmpOp::Lte,
                    value: 2,
                },
            },
        ]),
        then: smallvec![Suggestion {
            action: SuggestionAction::Guidance,
            note: "Single fractured hero mod on a mostly-empty item: sell as a recombinator-ready craft base.".into(),
            priority: 60,
        }],
        explanation: "Single-hero-mod fractured items are recombinator fodder with their own market.".into(),
        source: "/docs/34-heuristics-rulebook.md sec 6.6".into(),
        confidence: Confidence::Community,
    });

    // ---- Pricing exit (sec 7.x) -----------------------------------------

    out.push(Rule {
        id: RuleId::from("R190-sell-magic-with-2-tier1-mods"),
        category: Category::Pricing,
        when: ItemPredicate::All(vec![
            ItemPredicate::Rarity(Rarity::Magic),
            ItemPredicate::Ilvl(ValuePredicate {
                op: CmpOp::Gte,
                value: 80,
            }),
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
            note: "ilvl 80+ Magic with 2 mods: many crafters pay a premium for clean Regal-ready bases. Consider listing.".into(),
            priority: 65,
        }],
        explanation: "0.4 specifically: post-Homogenising loss makes 2-mod magics MORE valuable.".into(),
        source: "/docs/34-heuristics-rulebook.md sec 7.1".into(),
        confidence: Confidence::Verified,
    });

    out.push(Rule {
        id: RuleId::from("R191-fractured-base-premium-listing"),
        category: Category::Pricing,
        when: ItemPredicate::All(vec![
            ItemPredicate::Rarity(Rarity::Rare),
            ItemPredicate::HasFractured(true),
            ItemPredicate::Corrupted(false),
            ItemPredicate::Sanctified(false),
        ]),
        then: smallvec![Suggestion {
            action: SuggestionAction::Guidance,
            note: "Fractured bases command premiums. Either continue if your recipe is ready, or sell now to lock in profit.".into(),
            priority: 55,
        }],
        explanation: "Fracture is permanent value; further crafting risks bricking the now-valuable item.".into(),
        source: "/docs/34-heuristics-rulebook.md sec 7.2".into(),
        confidence: Confidence::Community,
    });

    // ---- Item base selection (sec 9.x) — guidance only -----------------

    out.push(Rule {
        id: RuleId::from("R200-ilvl-82-required-for-t1-mlvl-80-mods"),
        category: Category::BaseSelection,
        when: ItemPredicate::All(vec![
            ItemPredicate::Ilvl(ValuePredicate {
                op: CmpOp::Lt,
                value: 82,
            }),
            ItemPredicate::Ilvl(ValuePredicate {
                op: CmpOp::Gte,
                value: 75,
            }),
        ]),
        then: smallvec![Suggestion {
            action: SuggestionAction::Guidance,
            note: "ilvl < 82: T1 versions of mlvl-80+ mods (T1 phys%, T1 +3 spell skills, T1 max life on chest) are GATED. Replace with ilvl 82 base if you need them.".into(),
            priority: 75,
        }],
        explanation: "ilvl 82 unlocks the highest-tier mods — base selection matters.".into(),
        source: "/docs/34-heuristics-rulebook.md sec 9.1".into(),
        confidence: Confidence::Verified,
    });

    out.push(Rule {
        id: RuleId::from("R201-low-ilvl-leveling-floor"),
        category: Category::BaseSelection,
        when: ItemPredicate::Ilvl(ValuePredicate {
            op: CmpOp::Lt,
            value: 75,
        }),
        then: smallvec![Suggestion {
            action: SuggestionAction::Guidance,
            note: "ilvl < 75: leveling-tier base. For endgame crafts, replace with ilvl 75+ to open mid/high-tier mods.".into(),
            priority: 80,
        }],
        explanation: "VULKK base-pick floor for endgame.".into(),
        source: "/docs/34-heuristics-rulebook.md sec 9.3".into(),
        confidence: Confidence::Verified,
    });

    // ---- Vaal corruption decisions (sec 10.x) --------------------------

    out.push(Rule {
        id: RuleId::from("R210-vaal-rare-warning-irreplaceable"),
        category: Category::Vaal,
        when: ItemPredicate::All(vec![
            ItemPredicate::Rarity(Rarity::Rare),
            ItemPredicate::Corrupted(false),
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
            action: SuggestionAction::Guidance,
            note: "VULKK warning: 'Don't Corrupt something you cannot replace or have a replacement for.' Have a backup or use Hinekora's Lock first.".into(),
            priority: 40,
        }],
        explanation: "6-mod completed rare is the highest-stakes Vaal target.".into(),
        source: "/docs/34-heuristics-rulebook.md sec 10.1".into(),
        confidence: Confidence::Verified,
    });

    out.push(Rule {
        id: RuleId::from("R211-vaal-armour-or-martial-for-extra-socket"),
        category: Category::Vaal,
        when: ItemPredicate::All(vec![
            ItemPredicate::ItemClassAny(vec![
                poc2_engine::ids::ItemClassId::from("BodyArmour"),
                poc2_engine::ids::ItemClassId::from("Helmet"),
                poc2_engine::ids::ItemClassId::from("Gloves"),
                poc2_engine::ids::ItemClassId::from("Boots"),
                poc2_engine::ids::ItemClassId::from("Shield"),
                poc2_engine::ids::ItemClassId::from("Focus"),
                poc2_engine::ids::ItemClassId::from("OneHandSword"),
                poc2_engine::ids::ItemClassId::from("OneHandMace"),
                poc2_engine::ids::ItemClassId::from("TwoHandSword"),
                poc2_engine::ids::ItemClassId::from("TwoHandMace"),
                poc2_engine::ids::ItemClassId::from("Bow"),
                poc2_engine::ids::ItemClassId::from("Crossbow"),
            ]),
            ItemPredicate::Rarity(Rarity::Rare),
            ItemPredicate::Corrupted(false),
        ]),
        then: smallvec![Suggestion {
            action: SuggestionAction::ApplyCurrency {
                currency: CurrencyId::from("VaalOrb"),
                omens: vec![],
            },
            note: "Armour / martial weapon: Vaal has 1-in-4 chance to add an extra socket past the limit. Higher EV than caster items.".into(),
            priority: 65,
        }],
        explanation: "Per /docs/34 sec 10.3 — extra-socket Vaal outcome is exclusive to armour + martial weapons.".into(),
        source: "/docs/34-heuristics-rulebook.md sec 10.3".into(),
        confidence: Confidence::Verified,
    });

    out.push(Rule {
        id: RuleId::from("R212-vaal-caster-jewelry-warning"),
        category: Category::Vaal,
        when: ItemPredicate::All(vec![
            ItemPredicate::ItemClassAny(vec![
                poc2_engine::ids::ItemClassId::from("Wand"),
                poc2_engine::ids::ItemClassId::from("Staff"),
                poc2_engine::ids::ItemClassId::from("Sceptre"),
                poc2_engine::ids::ItemClassId::from("Ring"),
                poc2_engine::ids::ItemClassId::from("Amulet"),
                poc2_engine::ids::ItemClassId::from("Belt"),
            ]),
            ItemPredicate::Rarity(Rarity::Rare),
            ItemPredicate::Corrupted(false),
        ]),
        then: smallvec![Suggestion {
            action: SuggestionAction::Guidance,
            note: "Caster weapon / jewelry / belt: NO extra-socket Vaal outcome. Lower EV — only Vaal cheap replaceable items.".into(),
            priority: 35,
        }],
        explanation: "Per /docs/34 sec 10.4 — these classes can't get sockets from Vaal.".into(),
        source: "/docs/34-heuristics-rulebook.md sec 10.4".into(),
        confidence: Confidence::Verified,
    });

    out.push(Rule {
        id: RuleId::from("R213-vaal-prep-quality-and-sockets-first"),
        category: Category::Vaal,
        when: ItemPredicate::All(vec![
            ItemPredicate::Rarity(Rarity::Rare),
            ItemPredicate::Corrupted(false),
            ItemPredicate::Sanctified(false),
            ItemPredicate::Mirrored(false),
        ]),
        then: smallvec![Suggestion {
            action: SuggestionAction::Guidance,
            note: "Pre-Vaal checklist: apply quality, runes, all desired exalts FIRST. Vaal locks the item permanently.".into(),
            priority: 50,
        }],
        explanation: "Sequencing matters — Vaal is one-way.".into(),
        source: "/docs/34-heuristics-rulebook.md sec 10.11".into(),
        confidence: Confidence::Verified,
    });

    out.push(Rule {
        id: RuleId::from("R214-twice-corrupt-50pct-destroy-warning"),
        category: Category::Vaal,
        when: ItemPredicate::All(vec![
            ItemPredicate::Rarity(Rarity::Rare),
            ItemPredicate::Corrupted(true),
        ]),
        then: smallvec![Suggestion {
            action: SuggestionAction::Guidance,
            note: "Twice-corruption (Architect's Orb / Vaal Cultivation) has a 50% destroy chance. Buy a pre-twice-corrupted version of items you can't lose.".into(),
            priority: 35,
        }],
        explanation: "Per /docs/34 sec 10.9 — destroy outcome is total.".into(),
        source: "/docs/34-heuristics-rulebook.md sec 10.9".into(),
        confidence: Confidence::Verified,
    });

    out.push(Rule {
        id: RuleId::from("R141-architects-orb-on-corrupted-rare"),
        category: Category::Vaal,
        when: ItemPredicate::All(vec![
            ItemPredicate::Rarity(Rarity::Rare),
            ItemPredicate::Corrupted(true),
            ItemPredicate::Sanctified(false),
            ItemPredicate::Mirrored(false),
        ]),
        then: smallvec![Suggestion {
            action: SuggestionAction::ApplyCurrency {
                currency: CurrencyId::from("ArchitectsOrb"),
                omens: vec![],
            },
            note: "Corrupted Rare can be re-corrupted via Architect's Orb (0.4 Fate of the Vaal). Use sparingly — adds Vaal-mod outcomes including destruction.".into(),
            priority: 70,
        }],
        explanation: "0.4 Fate of the Vaal mechanic: Architect's Orb double-corrupts.".into(),
        source: "/docs/13-patch-0.4-changes.md".into(),
        confidence: Confidence::Community,
    });

    out.push(Rule {
        id: RuleId::from("R140-sanctify-mirror-tier-finisher"),
        category: Category::StopVsContinue,
        when: ItemPredicate::All(vec![
            ItemPredicate::Rarity(Rarity::Rare),
            ItemPredicate::Corrupted(false),
            ItemPredicate::Sanctified(false),
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
                currency: CurrencyId::from("DivineOrb"),
                omens: vec![poc2_engine::ids::OmenId::from("OmenOfSanctification")],
            },
            note: "6-mod rare ready for Sanctification. Locks the item permanently — only do this when satisfied.".into(),
            priority: 60,
        }],
        explanation: "Sanctified items are uncraftable but typically more valuable.".into(),
        source: "/docs/11-game-mechanics.md".into(),
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
        assert!(rules.len() >= 40, "got {} rules", rules.len());
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
