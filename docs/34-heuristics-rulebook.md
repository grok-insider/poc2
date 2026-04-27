# PoE2 0.4 "Fate of the Vaal" — Crafter Decision Heuristics Rulebook

A production-rule-style synthesis of the real-time decision rules used by skilled crafters. Each rule is structured as `IF <trigger> THEN <action> BECAUSE <rationale> [SOURCE]` so it can be lifted directly into an Advisor inference engine. Where authoritative sources contradict, both rules are listed and tagged with a confidence note.

> Conventions used in triggers:
> - `prefix_count` / `suffix_count` ∈ {0..3}; `mod_count = prefix_count + suffix_count`
> - Magic items can have at most 1 prefix and 1 suffix. Rare items can have at most 3 prefixes and 3 suffixes.
> - `mod_group` / modifier family is exclusive across an item: different tiers of the same family cannot appear together.
> - `ilvl` = item level; `mlvl(mod)` = the item-level requirement of a specific modifier (NOT its tier number)
> - `cost(craft_so_far)` measured in divines (`d`) or exalts (`ex`)
> - `expected_sale_price` = current poe.ninja / trade median for the resulting item
> - `value(item)` = current resale value of the item *as it stands now*

Sources keys used inline:
- **MOBA-OMEN** = Mobalytics, Lolcohol, "Omen Crafting" guide
- **MOBA-FRAC** = Mobalytics, sirgog, "Fracturing Orbs" guide
- **MOBA-ISOL** = Mobalytics, sirgog, "Isolation Crafting" guide
- **MOBA-VAAL** = Mobalytics, sirgog, "Vaal Orbs and Corrupting"
- **MOBA-BOOTS** = Mobalytics, Lolcohol, "50% Movement Speed Boot Craft"
- **MOBA-ABYSS** = Mobalytics, Lolcohol, "Abyss & Desecration Crafting"
- **MAXROLL-CORR** = Maxroll, Tenkiei, "Corruption Outcomes"
- **VULKK** = vulkk.com, RubyRose, "Crafting Recommendations and Tips"
- **AOEAH-GMC** = aoeah.com, "0.3 Guaranteed Mods Crafting Guide" (process carries to 0.4)
- **AOEAH-2X** = aoeah.com, "How To Double Corrupt in PoE 2 0.4"
- **REDDIT-BUDGET** = r/Poe2BudgetCraftGuide, "Crafting in 0.4 in general"
- **POE-WIKI-HL** = PoE Wiki, "Hinekora's Lock"
- **MEDIUM-DESEC** = Lidamnmy, "Desecration Crafting Guide"

---

## 1. Abandonment Heuristics — When to Scrap a Craft

| # | Trigger condition | Decision rule | Rationale | Source |
|---|---|---|---|---|
| 1.1 | After step 2 (Transmute + Augment), neither modifier is desired | Reforge 3-to-1 OR vendor/disenchant; do NOT Regal | Regal locks in two trash mods on a Rare and burns recovery potential. Cheaper to start over with another magic. | VULKK ("if we hit two undesirable stats, we can set the item aside for a 3-to-1") |
| 1.2 | After Regal, fewer than 2 of 3 mods are usable | Don't Exalt further; reforge or trash | "If at least two of the three Modifiers on the item are good enough, we can move on to Step 3. If not, we can set it aside for a 3-to-1." | VULKK |
| 1.3 | Fracture roll locked the WRONG modifier (~75% of attempts on 4-mod item) | Stop. Do not chaos/exalt further on this item. Sell as fractured base or use as recombinator fodder. | "75% of the time you'll fail and 'lock on' one of the less potent modifiers. At this point, you will be unable to usefully continue crafting this item." | MOBA-FRAC |
| 1.4 | Boot craft step 1: spent N greater aug cycles trying to hit 30% MS, no success | Continue only if MS prefix is the ONLY remaining target; otherwise re-roll base | The boot guide treats step 1 as the gating filter — every step after step 1 is deterministic. Failing step 1 is the *only* place to abandon. | MOBA-BOOTS |
| 1.5 | Cumulative cost > 3× expected sale price | Cut losses; sell whatever state the item is in | Sunk-cost trap. Most public crafters explicitly frame "do not chase" as the #1 mistake. | VULKK ("Don't Corrupt something you cannot replace"), inferred community norm |
| 1.6 | Cumulative cost > expected sale price AND craft is < 50% complete | Abandon | Even on hit, expected ROI is negative. | Implicit in REDDIT-BUDGET, where guides repeatedly state crafting "loses value early in league" if budget exceeds margin |
| 1.7 | Single craft cost > 1/20 of net worth ("Tarke rule") | Don't start the craft; scale down the target | Bankroll management — never bet so much that one brick zeroes your league. (Folk rule from Tarke Cat streams; not directly cited in scraped pages but consistently echoed in r/Poe2BudgetCraftGuide.) | Community / Tarke (unconfirmed in scrape) |
| 1.8 | Boots: didn't hit ≥30% MS by step 1 fail OR can't roll T1 evasion suffix | If using deterministic recipe, *only* abandon at step 1; never abandon mid-Perfect-Exalt | The Lolcohol boot recipe is 100% deterministic from step 2 onward — abandoning later means throwing away guaranteed value. | MOBA-BOOTS |
| 1.9 | Item already corrupted, sanctified, or has a Desecrated mod | Cannot be desecrated again; if craft requires desecration, abandon as crafting target | "Items with Desecrated Modifiers cannot be Desecrated again." | MOBA-ABYSS |
| 1.10 | Hinekora's Lock preview shows bad roll for ALL candidate currencies | Trade away the locked item or reforge sockets/quality (preserves preview) — don't waste the lock | Quality/socket changes don't consume the foreseeing state, so they're a "free" reroll if everything previewed is bad. | POE-WIKI-HL ("As using currencies which change quality, socket number… removes the foreseeing state, it allows for a low-risk method of rerolling other previewed outcomes.") |

---

## 2. Fracture Timing Heuristics

| # | Trigger | Rule | Rationale | Source |
|---|---|---|---|---|
| 2.1 | Item has fewer than 4 explicit mods | DO NOT fracture | Fracturing Orb requires "a rare item with four or more explicit modifiers." | MOBA-FRAC |
| 2.2 | Item has exactly 4 explicit mods, only 1 of which is the target ("hero mod") | Fracture is OPTIMAL here (1-in-4 = 25% to lock the right one) | Fewer "junk" mods = best probability. | MOBA-FRAC ("ideally used upon rare items… that have exactly four explicit modifiers and at least one extremely good modifier") |
| 2.3 | Item has 4 mods, 2 of which are great but incompatible (e.g. T1 phys% + T1 flat cold) | Fracture ANY of them — both outcomes are "good" | "Fracturing Orbs are even better if the item already has two excellent modifiers, but they do not work well together." Two winning faces on the die. | MOBA-FRAC |
| 2.4 | Item has 5–6 mods | Avoid fracture on this item | Probability to lock the hero mod drops to 1/5 or 1/6; brick-rate exceeds 80%. | MOBA-FRAC (implied by 4-mod recommendation) |
| 2.5 | Item already has a Desecrated mod | Desecrated mods are EXCLUDED from the fracture pool — use this to your advantage | The AOEAH +4 minion sceptre recipe explicitly uses a draw bone before fracturing because "the veiled mod cannot be fractured, so you have a 1-in-3 chance to fracture the minion skill gem mod." | AOEAH-GMC |
| 2.6 | About to fracture a hero mod with 4 mods on item, can afford a Hinekora's Lock | USE Hinekora's Lock first | One Lock < cost of multiple Fracturing Orbs in EV; preview lets you avoid a guaranteed brick. Especially true for high-value items where Lock < 3× target reduction in EV. | POE-WIKI-HL + MOBA-FRAC (general principle) |
| 2.7 | Want a fractured hero mod but mod can be locked into a useless slot first | Use desecration / draw bone to "fill" 3rd slot before fracture, raising hero-mod fracture odds | The AOEAH +4 sceptre recipe: get +3/+4 minion mod + 2 others, then desecrate (un-fracturable), then fracture → 1/3 chance instead of 1/4 or 1/5. | AOEAH-GMC |
| 2.8 | Goal is to enable later isolation crafting (Sinistral Annul + fractured prefix) | Fracture an unwanted prefix to "ban" it from annul, locking your good prefix | Isolation principle: "If the item has only five modifiers, you know before using the Annul what the result will be." | MOBA-ISOL |

---

## 3. Hinekora's Lock Decision Rules

| # | Trigger | Rule | Rationale | Source |
|---|---|---|---|---|
| 3.1 | Next currency is high-variance (Fracturing Orb, Vaal Orb, Annul on 5–6 mod item) AND item value > 3× Lock price | Apply Hinekora's Lock first | Preview lets you avoid bricks; same-name currency previews are deterministic per stack. | POE-WIKI-HL |
| 3.2 | Multiple candidate currencies exist (Divine, Tailoring, Sacred) for the next step | Apply Lock → preview ALL of them in turn → use whichever is best | "Having several potential useful currencies to craft the item with can increase the cost-effectiveness of using a Hinekora's Lock." | POE-WIKI-HL |
| 3.3 | Item is corrupted | DO NOT apply Lock | "Hinekora's Lock cannot be used on… items that are corrupted, mirrored or unmodifiable." | POE-WIKI-HL |
| 3.4 | Lock previews are all bad | Re-roll quality, sockets, or anointment to consume the lock without modifying explicits | "Using currencies which change quality, socket number, colours, links, or implicit modifiers removes the foreseeing state… a low-risk method of rerolling other previewed outcomes." | POE-WIKI-HL |
| 3.5 | About to perform a non-locked craft (Crafting Bench, Recombinator, Corruption Altar) | Lock provides NO preview here | "Foreseeing items cannot preview results of crafts done using the Crafting Bench, Recombinator, beastcrafting, or the Corruption Altar." | POE-WIKI-HL |
| 3.6 | Cost(Lock) > 1/3 × cost(item-as-replacement) | Don't bother — just take the gamble or buy a finished item | EV math from MOBA-VAAL ("4 Vaals + 4 items ≈ 3 Vaals + 3 items + 3 Omens") generalises to any preview-tax decision. | MOBA-VAAL (analogy) |

---

## 4. Exalt-vs-Desecrate Choice

| # | Trigger | Rule | Rationale | Source |
|---|---|---|---|---|
| 4.1 | Open suffix slot, target is a regular (non-Lich/Kurgal/Ulaman/Amanamu) mod | Exalt with appropriate Omen (Dextral + Homogenising) | Exalt slams are cheaper and more controllable for normal mods than wasting a bone. | MOBA-OMEN, AOEAH-GMC |
| 4.2 | Target is a unique Desecrated-only mod (Kurgal/Ulaman/Amanamu/Lich) | Use Abyssal Bone + targeted Omen (Blackblooded/Liege/Sovereign) | These mods cannot drop from Exalt slams under any circumstances. | MOBA-OMEN, MOBA-ABYSS |
| 4.3 | Item has 1 open prefix, 0 open suffixes, target mod is a prefix | Bone OR Sinistral Exalt — both work. Bone wins if you need higher mlvl guarantees | "If your Rare item has full Suffixes but an open Prefix, a Desecrated Prefix will be added" (deterministic slot from bone). | MOBA-ABYSS |
| 4.4 | Item has 1 open prefix and 1 open suffix, NEED a prefix | Use Sinistral Necromancy Omen + bone (forces prefix) OR Sinistral Exalt | Both deterministic for slot. Choose based on whether you need the desecrated-only pool. | MOBA-ABYSS |
| 4.5 | Item is FULL (3p+3s, 6 mods) and you want to "cycle" a mod | Use bone — it removes a random mod and adds a desecrated of the same affix-side | "If your Rare item has 6 modifiers, the Bone will remove a random modifier and then add a Desecrated Prefix or Suffix (matching the removed type)." Risky — random removal. | MOBA-ABYSS |
| 4.6 | Want a guaranteed T1 mod added | Use Perfect Exalt (mlvl ≥ 50) + Homogenising Omen | Perfect Exalt + Homogenising is the workhorse of isolation crafting. | MOBA-ISOL, MOBA-BOOTS (step 4) |
| 4.7 | Need a specific mod and existing mod tags ALREADY isolate it | Plain Exalt + Homogenising (no Perfect needed) | If only one tag-matching mod exists in the pool, the Exalt is fully deterministic without the Perfect tax. | MOBA-OMEN, AOEAH-GMC (gloves example: only chaos res suffix → must roll elemental res) |
| 4.8 | Have an Essence of the Abyss (Mark of the Abyssal Lord) | Apply it BEFORE the bone to guarantee the next desecrated mod is high-tier | "When Desecrating an item with this modifier, the Mark will always be removed and replaced with a Desecrated modifier… of a higher Tier." | MOBA-ABYSS |
| 4.9 | Want a desecrated mod but worried about the reveal | Activate Omen of Abyssal Echoes BEFORE going to Well of Souls | "The next time you Reveal Desecrated Modifiers, you can reroll the options once." Cheap insurance. | MOBA-OMEN, MOBA-BOOTS (step 5) |
| 4.10 | Bones are cheap, Exalts are scarce (early league) | Lean on bones for filling slots | Bones drop from Abyssal Fissures, scale with Atlas tree. Often more abundant than Greater/Perfect Exalts in week 1–2. | MEDIUM-DESEC, REDDIT-BUDGET |

---

## 5. Whittle-vs-Annul Choice

| # | Trigger | Rule | Rationale | Source |
|---|---|---|---|---|
| 5.1 | One specific unwanted mod is the LOWEST mlvl on the item | Use Omen of Whittling + Chaos Orb | Whittling targets the lowest-mlvl mod — deterministic if it's unique-low. | MOBA-OMEN, VULKK |
| 5.2 | Multiple T2 mods on item, only one shares the lowest mlvl | Whittling will ONLY hit that one | "Spell Crit modifier has the 'lowest level modifier', despite it being the same tier as other modifiers on the item." | MOBA-OMEN |
| 5.3 | Unwanted mod is a SUFFIX, locked prefixes are good | Sinistral Annul Omen (with optional fractured prefix) > Whittling | Annul leaves slot open for re-Exalt; Chaos+Whittle replaces immediately, possibly with another bad mod. | MOBA-FRAC, MOBA-ISOL |
| 5.4 | Item has fractured prefix + Sinistral Annul Omen + 2 other prefixes | Annul will deterministically hit the non-fractured prefix you don't want | The canonical isolation craft. | MOBA-ISOL, MOBA-FRAC |
| 5.5 | Want to remove an unwanted desecrated mod | Use Omen of Light + Annul | "Your next Orb of Annulment will remove only Desecrated Modifiers." Deterministic for the desecrated mod. | MOBA-OMEN, AOEAH-GMC |
| 5.6 | "Perfect Chaos" trick: want to replace a low-mlvl mod with a guaranteed T1 | Use **Perfect Chaos Orb** + Omen of Whittling (or Erasure) on item where remaining mods are all high mlvl | Perfect Chaos forces mlvl ≥ 50; combined with Whittling targeting the low-mlvl junk, the replacement is guaranteed-T1. (Same isolation logic as Perfect Exalt + Homogenising.) | MOBA-ISOL (general principle), VULKK (Whittling+Chaos) |
| 5.7 | Whittling preview shows it would hit a powerful Essence-applied mod | DO NOT use Whittling — Essence mods are often "low-level" by mlvl | "Many powerful Modifiers added through Essences or other means may be considered much lower-level requirement and be hit." | VULKK |
| 5.8 | Unsure which mod Whittling targets | Pin the item, then HOVER with Chaos Orb to see highlight | "If you are unsure of which Modifier is being replaced, pin your item stats, then hover with a Chaos Orb to be certain you're not losing a valuable stat." | VULKK |

---

## 6. Stop-vs-Continue on Partial Success

| # | Trigger | Rule | Rationale | Source |
|---|---|---|---|---|
| 6.1 | 4 of 6 desired mods hit, all remaining steps are deterministic | CONTINUE | Deterministic = guaranteed value-add. | MOBA-BOOTS, AOEAH-GMC |
| 6.2 | 4 of 6 hit, but next step is RNG with > 30% brick chance AND value(item-now) ≥ 0.5 × value(target) | SELL NOW | Risk-adjusted EV is usually below current sale price by step 4–5. | REDDIT-BUDGET (general crafting flip philosophy) |
| 6.3 | T2 hit on a slot where T1 cost is 5×+ | Settle for T2; finish other slots | Diminishing returns; T2 vs T1 rarely justifies 5× spend except for mirror-tier crafts. | Implicit in budget-craft community guidance |
| 6.4 | About to Sanctify: have you achieved all 6 desired mods? | Only Sanctify if YES | Sanctify rolls each mod independently 0.78×–1.22×; ~2.1% chance all-positive on a 6-mod item. Only worth it on a true keeper. | VULKK ("~2.1% Chance of all 6 modifiers being positive or neutral"), MAXROLL-CORR |
| 6.5 | Got "good enough" 4-mod jewel via desecration, demand is high | SELL UNREVEALED 4-mod jewels | "After reaching three or four mods, you can sell unrevealed four-mod jewels for profit, as they are in high demand." | AOEAH-GMC |
| 6.6 | Hit a hero mod (e.g. +3 minion gems, +2 spell skills) but rest of item is mid | Sell as a craft-base; price reflects fractured/single-mod scarcity | Single-hero-mod items are recombinator fodder — there's a market. | MOBA-FRAC (implied), AOEAH-GMC |

---

## 7. Pricing Exit Heuristics — When to Sell

| # | Trigger | Rule | Rationale | Source |
|---|---|---|---|---|
| 7.1 | Magic-stage item with 2× T1 mods that synergize (e.g. ilvl 82 + T1 prefix + T1 suffix) | Sell at magic stage to other crafters | Crafters pay a premium for clean 2-mod magic bases (Regal-ready). | AOEAH-GMC (fracturing recipes start from magic-stage hits), REDDIT-BUDGET |
| 7.2 | Successfully fractured an excellent mod | Sell as a fractured base (premium) OR continue if you have the recipe ready | Fractured bases command premiums; further crafting risks bricking the now-valuable item. | MOBA-FRAC, AOEAH-GMC (jewelry +3 base flip) |
| 7.3 | Hit T1 chaos res ring at 55%+ via Ulaman desecration but no quality applied | Quality with breach catalysts BEFORE listing — quality multiplies the sell price | "Add quality to your finished ring — breach rings can go up to 40%. With a perfect roll, you can reach up to 68% chaos resistance." | AOEAH-GMC |
| 7.4 | Item is mid-craft, you're out of currency | Offer as a "craft service" — buyer brings the rest of the currency, pays a craft fee | Common community practice in trade league. | Implicit in REDDIT-BUDGET trade culture |
| 7.5 | Bricked craft on a popular base (e.g. ilvl 82 Siren Scale Gloves) | Sell as ilvl 82 base — base scarcity is the real value | "Item Level 82+" bases are themselves currency-grade. | AOEAH-GMC, search-result reddit "white ilvl 82 breach ring profit" |
| 7.6 | Vaal-themed unique with downside, before twice-corrupting | Consider selling first — twice-corrupt is 50/50 destruction | "Some players have shared their heartbreak in losing valuable items… consider buying a twice-corrupted version of the item you want, instead of risking your own." | AOEAH-2X |
| 7.7 | Late league, item value is dropping daily | Sell sooner rather than later; meta shifts | "Crafting in POE2 0.4 looks strong, but it's a trap for most players… crafting loses value early in league." | YouTube title "CRAFTING Is a TRAP" (search result) |

---

## 8. Budget Rules

| # | Rule | Source / rationale |
|---|---|---|
| 8.1 | "1/20 of net worth" — never spend more than 5% of liquid net worth on a single craft attempt | Tarke Cat / community folk rule (echoed across r/Poe2BudgetCraftGuide; not directly cited in scrape — flagged as community heuristic) |
| 8.2 | "Never spend more than 3× expected sale price" before abandoning | Implicit EV rule from MOBA-VAAL ("4 Vaals ≈ 3 Vaals + 3 Omens" — equivalence pricing) and standard expected-value framing |
| 8.3 | Scale to mirror-tier only after consistently producing 100D+ items | "I expect the players who first master the use of these Orbs in trade league to end up with multiple Mirrors of Kalandra from their for-profit crafting sessions." (i.e. mirror-tier is endgame for *mastered* crafters) | MOBA-FRAC |
| 8.4 | Reinvest profit, not principal — keep a reserve equal to your starting bankroll | Standard ARPG flipper guidance; appears in Maxroll currency-flipping resource |
| 8.5 | Track currency velocity: crafts that don't return capital within 24h of trade-league time are losing to opportunity cost | Implied by Maxroll "Flipping With The Currency Exchange" (flip based on time-of-day) |
| 8.6 | If craft uses Hinekora's Lock + Perfect Exalt + Omen of Homogenising Exalt + bone, total deterministic-step cost should be < 0.5× expected sale | Boot recipe and amulet recipe both follow this implicit ratio | MOBA-BOOTS, AOEAH-GMC |

---

## 9. Item Base Selection

| # | Trigger | Rule | Rationale | Source |
|---|---|---|---|---|
| 9.1 | Targeting any T1 mod that requires mlvl ≥ 80 (e.g. T1 % phys, T1 +3 spell skills, T1 maximum life on chest) | Buy ilvl 82 base | "For the highest tiers, search for siren scale gloves with an item level of 82 or higher." | AOEAH-GMC |
| 9.2 | Targeting common mods, ilvl 80 unlocks all needed tiers | ilvl 80 base is fine | "We'll want items that are at least Item Level 75. This opens up most tiers of many Modifiers, save for a few Modifiers that will ask for Item Level 80+." | VULKK |
| 9.3 | Campaign / leveling craft | ilvl 75–76 sufficient | "We'll want items that are at least Item Level 75." | VULKK |
| 9.4 | Build needs +3/+4 to a specific skill type from sceptre | ilvl 78+ rattling sceptre is the FLOOR for +4 minion gems | "It must be item level 78 or higher to allow rolling +4 to minion skill gems." | AOEAH-GMC |
| 9.5 | Want exceptional sockets (extra socket bases) | Use Exceptional bases (Dragonscale Boots, etc.) — rare/expensive | "Exceptional Items, particularly those with extra sockets, to create incredibly powerful items. Be warned, these item bases are incredibly rare and by no means cheap." | VULKK, MOBA-BOOTS (Exceptional Dragonscale) |
| 9.6 | Build is STR-stacker | Pick STR-requirement bases (higher armour rolls) | Standard ARPG itemization (PoE1 carry-over rule); confirmed by VULKK base-pick discussion |
| 9.7 | Build is INT/ES-stacker | Pick INT-requirement bases (Siren Scale, etc.) | AOEAH-GMC explicitly uses Siren Scale Gloves for ES craft |
| 9.8 | Build is DEX/Evasion | Pick DEX bases (Cinched Boots family) | Standard; MOBA-BOOTS uses Dragonscale (DEX/INT hybrid) for evasion+ES |
| 9.9 | Crafting jewelry, want chaos res | Amethyst (chaos res implicit) + Breach (catalyst-quality boost) — recombine | "Tier one all elemental resistance rings cost about 1x. Breach rings with tier one chaos resistance can be found for a range of prices." | AOEAH-GMC |
| 9.10 | Crafting attack weapon, want crit | ilvl 81 Gemini bow base for +bolt-speed implicit (or class equivalent) | "Start with an item level 81 Gemini bow base." | AOEAH-GMC |

---

## 10. Vaal Corruption Decisions

| # | Trigger | Rule | Rationale | Source |
|---|---|---|---|---|
| 10.1 | Item is rare, build-defining, irreplaceable | DO NOT VAAL | "Don't Corrupt something you cannot replace or have a replacement for." | VULKK |
| 10.2 | Have 2+ near-identical items (e.g. two weapons within 1–2% damage) | Vaal one of them | "It is at its best when you have two very similar items… if you Vaal one and the Vaal Orb rolls well, it may pull 5% ahead. And if you roll poorly… well, you had a backup." | MOBA-VAAL |
| 10.3 | Item is armour/martial weapon and missing a socket | Vaal — 1 in 4 chance to add socket past limit | "Helmet/Gloves/Boots/Chestpiece/Shield/Focus/Martial Weapons: +1 Socket (ignoring limit)." | MAXROLL-CORR |
| 10.4 | Item is caster weapon/amulet/belt/ring (cannot get socket) | Vaal is HIGHER risk, lower reward — only Vaal if cheap to replace | "Because you can't get a socket on caster weapons, amulets, belts, or rings, these items are less desirable corruption targets." | MAXROLL-CORR |
| 10.5 | Have an Omen of Corruption AND item value > 3× Omen value | Use the Omen | "I recommend using the Omen of Corruption only if the item you wish to Vaal is worth at least three times as much as the Omen. This is because 4 Vaal Orbs and 4 of your item on average give the same positive results as 3 Vaal Orbs, 3 of your item and 3 Omens of Corruption." | MOBA-VAAL |
| 10.6 | Unique item with single dominant scaling stat (Fireflower +Fire skills, Kaom's life) | Vaal — krangledivine outcome can push past max | "Unique modifier… can be increased to above the maximum value." Worth it on uniques with one big mod. | MAXROLL-CORR, MOBA-VAAL (Fireflower example) |
| 10.7 | Have a tier 15 Waystone | Vaal it — high chance to upgrade to T16 | "When you Corrupt a Waystone (Tier 15), it can increase the Tier by 1." | MAXROLL-CORR |
| 10.8 | Have a Vaal-themed unique (small red faces in tooltip) | 0.4 ONLY: consider twice-corrupt via temple Tier-3 Sacrificial Chamber / Vaal Cultivation Orb | "Vaal Cultivation Orb… For Vaal-themed uniques, up to two modifiers can be replaced with random ones pulled from multiple pools, including unique-specific mods." | AOEAH-2X |
| 10.9 | About to twice-corrupt | Accept that 50% of attempts destroy the item | "There's a 50% chance the item is destroyed outright." | AOEAH-2X |
| 10.10 | Skill gem at level 20 | Vaal it for +1 level (12.5% chance) | "12.5% — Enchant the gem with +1 gem level. This is generally the best outcome." | MOBA-VAAL |
| 10.11 | Pre-corruption checklist | Apply all quality, all sockets, all desired exalts BEFORE Vaal | "Before Vaaling, apply quality and rune sockets if they'll be relevant." Vaal locks the item permanently. | MOBA-VAAL, VULKK |
| 10.12 | Late league (week 4+), have spare corrupting orbs | Higher EV to Vaal because items in market are saturated; Vaal-jackpots stay valuable | Implied by MAXROLL-CORR + market structure (uniques w/ enchants persist as currency-priced even when bases collapse) |
| 10.13 | Early league, items are scarce | Avoid Vaal on uniques; sell them uncorrupted instead | Uncorrupted uniques sell for premium early because corruption-eligible buyers exist (community norm) |

---

## 11. Market Awareness Heuristics

| # | Trigger | Rule | Rationale | Source |
|---|---|---|---|---|
| 11.1 | Picking a build/craft target | Check poe.ninja/poe2 most-played builds first | Meta builds drive demand for specific mod combinations; off-meta = low liquidity. | Implicit standard (MOBA-FRAC suggests targeting "outstanding +3 to level of all minion skills" because of build pressure) |
| 11.2 | League day 1–7 | Craft generic resists/life/MS — universal demand | Every build needs resists; specific scaling mods are devalued before builds gear up. | REDDIT-BUDGET methodology |
| 11.3 | League day 7–21 | Pivot to build-specific god rolls (+3 spell, +4 minion, crit weapons) | Demand peaks as players hit endgame and need upgrades. | REDDIT-BUDGET methodology |
| 11.4 | League day 21+ | Mirror-tier or recombinator-tier only; mid crafts have collapsed | "Crafting loses value early in league" — bulk crafts saturate the market by week 3. | YouTube "CRAFTING Is a TRAP" (titles), REDDIT-BUDGET |
| 11.5 | Niche off-meta build with high crafting need | Profitable IF you can supply consistently | Off-meta crafters dominate that niche due to low competition. Riskier but higher margin. | Inference from market structure |
| 11.6 | Currency exchange shows fracture orbs falling | Currency volatile; "value of fracturing orbs fell from more than 30 divines to less than 20 within the space of an hour or two" | Sell harvest currency fast at league start; hold currency long only if stable. | Search result: YouTube "POE2 0.4.0d" (creator commentary) |
| 11.7 | Consider time-of-day for the trade league | Maxroll: "flip based on time of day. Often times, certain time zones have a difference" | Asia/EU/NA peak hours have predictable swings | Maxroll "Flipping With The Currency Exchange" |

---

## 12. Recovery Heuristics — When a Step Fails

| Failure mode | Recovery rule | Source |
|---|---|---|
| Bricked fracture (locked the wrong mod) | Sell as fractured base (mods still desirable to recombinator users) OR use as recombinator fodder | MOBA-FRAC |
| Bad reveal at Well of Souls | Pick the "least bad" of 3 → Omen of Light + Annul to remove it deterministically → reapply bone | MOBA-OMEN, AOEAH-GMC |
| Bad reveal AND no Omen of Light | If you also have a Sinistral/Dextral Annul Omen, target the side the desecrated mod is on (it's the only mod of that side, often) | MOBA-OMEN |
| Vaal bricked the item | Salvage for sockets/quality currency; if it's a unique with one good mod left, sell as a "partial corrupt" (some buyers want specific corrupted enchants) | MAXROLL-CORR |
| Wrong mod from Perfect Essence | Check tags: another Perfect Essence of a different family on the SAME slot will overwrite. If the existing mod is the "removed" target of the Crystallisation Omen, it can be deleted next pass | VULKK (Crystallisation), AOEAH-GMC (essence sequence) |
| Annul removed wrong mod | Re-Exalt with appropriate Omens; if all your Omens are spent, sell the item as 5-mod and start over | Standard recovery via MOBA-OMEN omen list |
| Twice-corrupt destroyed item | No recovery — buy replacement on market or restart from base | AOEAH-2X |
| Hit T1 mod but suffix is full and you need a prefix slot | Sinistral Annul Omen (prefix-only annul) on a fractured-suffix item, then Exalt | MOBA-ISOL |
| Sanctification rolled badly | NO RECOVERY — Sanctified items cannot be modified further | VULKK ("Sanctification is a final step, and no further changes can be made after") |
| Boot craft step 1 failed (no MS prefix after many augs) | Repeat Annul→Aug cycle; the recipe is designed to be idempotent until MS hits | MOBA-BOOTS |

---

## 13. Confidence / Expected-Value Heuristics

| # | Rule | Source |
|---|---|---|
| 13.1 | Geometric distribution: if a single Chaos Spam attempt has success probability p, expected attempts = 1/p; 50%-confidence attempts = ln(2)/-ln(1-p) ≈ 0.69/p for small p | r/pathofexile "The mathematical way to choose when to stop crafting" (search result) |
| 13.2 | Total weight / target weight = expected attempts when slamming an open slot | YouTube "Figure Out the Odds of ANY Craft - Modifier Weights Explained" (Belton) |
| 13.3 | If 2× expected-attempt cost > value(target), STOP. The next attempt is almost certainly negative-EV given variance | Standard EV stopping rule, derived from r/pathofexile post |
| 13.4 | Trust the math when craft is RNG-only; trust feel when craft has multiple deterministic finishing options | Inference: deterministic recipes (MOBA-BOOTS, AOEAH-GMC) explicitly remove EV calculation by removing variance |
| 13.5 | When previewing with Hinekora's Lock, the Lock's expected savings = (P_brick × value_loss). Apply Lock when this exceeds Lock cost | POE-WIKI-HL (multi-currency preview rationale) |
| 13.6 | Isolation crafting reduces your effective attempts to 1 by banning all other RNG branches; the cost of "banning" (Omens + Perfect tier) should be < expected number of un-isolated attempts × per-attempt cost | MOBA-ISOL |
| 13.7 | For Sanctification: P(all positive) ≈ 0.5^6 ≈ 1.56%, P(all neutral-or-positive) ≈ 2.1%; max-roll all-6 = ~1 in 4.75 billion. Treat Sanctify as a final upgrade gamble, not a craft step | VULKK |
| 13.8 | Vaal Orb on rare gear is a 4-way 25% split. EV is positive only when the "krangledivine"/+socket/+enchant outcomes for THIS slot exceed 50% of current item value | MAXROLL-CORR, MOBA-VAAL |

---

## 14. Compact "Decision Tree" Cheat Sheet for Advisor Engine

```
ENTRY:
  if item.corrupted: → only Vaal-temple twice-corrupt path is open (rule 10.8/10.9)
  if item.sanctified: → STOP (no further crafting possible)

ITEM_PREP:
  if item.rarity == NORMAL: → Transmute (greater/perfect by budget)
  if item.rarity == MAGIC and mod_count == 1: → Augment OR Essence
  if item.rarity == MAGIC and mod_count == 2 and ≥1 mod desired:
      → Regal OR Essence (Essence preferred for guaranteed mod)
  else (2 trash mods): → 3-to-1 reforge (rule 1.1)

RARE_GROW:
  if mod_count < 4 and want target on open slot:
    SELECT_FILL_METHOD:
      if target ∈ desecrated_only_mods: → Bone + targeted Omen (rule 4.2)
      elif target ∈ essence_pool: → Perfect Essence + Crystallisation Omen
      elif tag-isolation possible (rule 4.7): → Plain Exalt + Homogenising
      elif all-T1 needed: → Perfect Exalt + Homogenising + (slot Omen)
      else: → Plain/Greater Exalt + slot Omen
    if next step is HIGH-VARIANCE and value(item) ≥ 3× cost(Hinekora's Lock):
      → Apply Hinekora's Lock first (rule 3.1)

FRACTURE_DECISION:
  precondition: mod_count ≥ 4
  if mod_count == 4 and exactly 1 hero mod: → fracture (best EV) (rule 2.2)
  elif mod_count == 4 and 2 hero mods (incompatible): → fracture (any) (rule 2.3)
  elif mod_count >= 5: → AVOID; abandon-or-sell (rule 2.4)
  if can pre-load with desecrated mod to reduce odds: → do so (rule 2.7)

CLEAN_UP:
  if unwanted mod is desecrated: → Omen of Light + Annul (rule 5.5)
  elif unwanted mod is lowest-mlvl AND no Essence-mod risk: → Whittling + Chaos (rule 5.1, 5.7)
  elif unwanted mod is on side-with-fracture: → Sinistral/Dextral Annul + Omen (rule 5.4)
  elif need replacement immediately: → Sinistral/Dextral Erasure + Chaos
  else: → raw Annul (last resort)

PARTIAL_SUCCESS_GATE (after each step):
  current_value = market.priceCheck(item)
  remaining_cost = sum(deterministic_steps) + RNG_steps × variance_factor
  if remaining_steps_all_deterministic AND remaining_cost < expected_uplift: → CONTINUE
  elif current_value ≥ 0.5 × target_value AND next step has > 30% brick chance: → SELL (rule 6.2)
  elif cumulative_cost > 3× current_market_target: → SELL or ABANDON (rule 1.5)

FINISH:
  apply quality (rule 7.3)
  fill sockets (Artificer / runes / soul cores)
  decide Vaal/Sanctify:
    if irreplaceable: → DO NOT (rule 10.1)
    if have backup AND positive EV: → Vaal (rule 10.2)
    if all 6 mods at target AND have Omen of Sanctification AND can absorb 78% downside: → Sanctify
    if Vaal-themed unique AND temple progressed: → consider twice-corrupt (rule 10.8)

EXIT:
  list at expected_sale_price - 5% (undercut for fast sale)
  if item is mid-craft service-able: → list as craft-service slot
```

---

## 15. Sources & Further Reading

Primary sources scraped (all live as of league 0.4):

- **Mobalytics (Lolcohol, sirgog)** — Omen Crafting; Fracturing Orbs; Vaal Orbs; Isolation Crafting; 50% MS Boot Craft; Abyss & Desecration Crafting. Strongest deterministic-recipe source. `mobalytics.gg/poe-2/guides/*`
- **Maxroll (Tenkiei)** — Corruption Outcomes (definitive Vaal probability table). `maxroll.gg/poe2/resources/corruption-outcomes`
- **VULKK (RubyRose)** — Crafting Recommendations and Tips. Clear step-by-step decision flow. `vulkk.com/2025/01/10/path-of-exile-2-crafting-recommendations-and-tips`
- **AOEAH** — 0.3 Guaranteed Mods Crafting Guide (full multi-recipe walkthroughs); 0.4 Twice-Corrupt guide.
- **Reddit r/Poe2BudgetCraftGuide** — community decision norms for budget crafters; often cites Tarke Cat / Mosey / Subtractem patterns.
- **PoE Wiki** — Hinekora's Lock mechanics. `poewiki.net/wiki/Hinekora%27s_Lock`
- **PoE2DB** — current currency and essence effect tables, including Magic-to-Rare essence behavior. `poe2db.tw/us/Crafting`, `poe2db.tw/us/Essence`
- **Lidamnmy / Medium** — Desecration Crafting Guide (synergies, omens, abyss essence).

Sources referenced but not deep-scraped (Reddit JSON unsupported by scraper, YouTube transcripts unavailable in batch):
- YouTube transcripts from Tarke Cat, Mosey, Subtractem, Captain Lance, Big Ducks, Goratha — names appeared in search results; their guidance is encoded in budget/recovery rules but not directly quoted here.
- Krakenbul / Prohibited Library Discord FAQ — server discoverable via `pathofexile.com/forum/view-thread/3294478`; live FAQ is gated behind Discord and was not extractable through the web scraper.

Rules flagged as "community norm" or "Tarke folk rule" should be **validated against direct streamer transcripts** before being treated as canon for the Advisor — the scraped corpus did not contain Tarke's exact bankroll-fraction wording.

---

*This document is intended as input to a forward-chaining inference engine. Each numbered rule (e.g. `1.3`, `5.4`) can be referenced as a stable identifier in production-rule outputs.*
