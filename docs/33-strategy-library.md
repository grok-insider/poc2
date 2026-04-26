# PoE 2 Patch 0.4 (Fate of the Vaal) — Advanced Crafting Strategy Library

Comprehensive catalog of named/codified expert strategies in patch 0.4 (December 2025 launch through ~April 2026). Sourced from primary expert content: Belton (`@BeltonPoE`), sirgog (Mobalytics), Lolcohol (Mobalytics), PaintMasterPoE, plus Mobalytics, MMOJUGG, POE2FUN, AOEAH, and the official PoE2 wiki.

---

## Patch 0.4 Context — What Changed

The single biggest 0.4 crafting shift, per the official patch notes (Maxroll PoE2 mirror + GGG forum thread 3883495):

- **Omen of Homogenising Exaltation: DISABLED FROM DROPPING.** Existing stockpiles still work.
- **Omen of Homogenising Coronation: DISABLED FROM DROPPING.** Existing stockpiles still work.
- Result: the dominant 0.3 "5T1 guaranteed" pipeline (Belton's term) is gone. Every strategy below has been re-evaluated for the post-homogenising era.
- New league mechanic *Fate of the Vaal* added the Corruption Altar / **Architect's Orb** / **Crystallised Corruption** / **Vaal Cultivation Orb** — enables true double corruption portably (50% destroy on uniques).
- Carry-over from 0.3 still relevant: Greater/Perfect tiers of Transmute/Aug/Regal/Chaos/Exalt; reworked Essences (Lesser/Normal/Greater/Perfect); Desecration system (Bones, Well of Souls, Abyss-exclusive mods); Hinekora's Lock; Sanctification; Exceptional Bases (up to 30% qual / extra socket).

**Glossary used throughout:**
- **Greater/Perfect Exalt**: guarantees min mod-level 35 / 50 respectively.
- **Greater/Perfect Chaos**: same min-mod-level guarantees on the Chaos remove-and-add.
- **Sinistral / Dextral**: prefix-side / suffix-side targeting.
- **Bones**: Jawbone (weapon/quiver), Rib (armour), Collarbone (jewelry), Cranium (jewel), Vertebrae (waystone). Quality tiers: Gnawed (≤ilvl 64), Preserved (any ilvl), Ancient (any ilvl, guarantees min mod-level 40).
- **Abyss Lord omens**: Liege = Amanamu, Sovereign = Ulaman, Blackblooded = Kurgal — each forces an Abyss-exclusive mod from that lord's pool on the next desecration, weapon/jewelry only.
- **Sanctified**: post-Sanctification tag that locks all further crafting on the item.

---

## Strategy Catalog

### 1. The Apprentice Blueprint (Additive-Only Crafting)

- **Source:** Tarke Cat / community shorthand. Codified across multiple beginner guides (Mobalytics crafting basics, MyleSwi, PaintMaster). The "additive only" framing — never use Annul/Chaos, only steps that add — is the canonical entry-level pipeline.
- **Item type:** Any (boots, gloves, helmets, jewelry early game).
- **Preconditions:** White (Normal) base, decent ilvl for desired mod tiers (ilvl 82 for tri-res T1; ilvl 75+ for general endgame).
- **Currencies:** 1× Orb of Transmutation, 1× Orb of Augmentation, 1× Regal Orb, 3× Exalted Orbs (Greater/Perfect preferred), optional 1× Preserved Bone.
- **Procedure:**
  1. Transmute white → magic (1 mod).
  2. Augment magic (2 mods). Inspect: are *both* mods build-relevant? If yes, keep going. If no — vendor and retry on a new white base.
  3. Regal magic → rare (3 mods). Inspect again.
  4. Exalt × 3 to fill remaining slots (4, 5, 6 mods). Use Greater/Perfect Exalts to avoid garbage low-tier mods.
  5. Optional finisher: Preserved Bone for a 7th-effective desecrated mod (random reveal of 3 options).
- **Decision points:**
  - After Augment: if both mods are good → continue; if one is junk → vendor (do NOT annul, that's not the Apprentice Blueprint).
  - After Regal: 3rd mod random; you can stop here and sell as a budget rare.
  - Use Sinistral/Dextral Exaltation omens to bias prefix vs suffix per Exalt slam.
- **Recovery:** If a slam adds a junk mod, you cannot remove without losing the "additive only" purity. Either (a) sell the bricked piece, or (b) graduate out of Apprentice Blueprint into the Whittling Cleanup (#7).
- **Expected cost:** ~1–3 div for typical endgame piece (mostly Exalts). Greater/Perfect Exalts inflate to 5–15 div.
- **Probability of success (build-usable item):** ~25–40% per attempt depending on how strict your mod requirements are.
- **When to abandon:** After Augment if both mods aren't useful. After 5th slam if 3 of 5 mods are junk — sell as scraps.

---

### 2. Fracture-Then-Chaos-Spam

- **Source:** Codified by sirgog (Mobalytics fracturing-orbs guide), expanded by POE2FUN's "When to Stop Spamming Chaos Orbs" guide, made into the de facto baseline by Belton's 0.4 video.
- **Item type:** Any 4+ mod rare; most efficient on weapons, body armour, jewelry with one rare anchor mod (+gem level, +projectiles, T1 phys).
- **Preconditions:** Rare item with **exactly 4 explicit mods** containing one excellent mod. ilvl 82 ideal.
- **Currencies:** 1× Fracturing Orb (≈15–25 div), bulk Chaos Orbs (Greater/Perfect preferred).
- **Procedure:**
  1. Get rare item to 4 mods with one chase mod (e.g., +2 projectile, T1 phys%, +5 spell levels).
  2. Slam Fracturing Orb. **75% chance to fracture the wrong mod — bricked**, ~25% (1 in 4) to lock the chase mod.
  3. If chase mod locked: spam Chaos Orbs until you hit the desired *opposite-side* T1 pair.
     - Fractured prefix → spam until 2 good suffixes appear.
     - Fractured suffix → spam until 2 good prefixes appear.
  4. **Stop the moment you have the fracture + 2 good opposite-side mods** (the "Golden Rule").
  5. Transition to Whittling Cleanup (#7) for the final 2 slots — never naked-Exalt at this stage.
- **Decision points:**
  - Fracture failure → see Recovery.
  - Fractured side determines which side you Chaos for (intact pool is "easier").
  - Multi-good-fracture bases (e.g., weapon with both T1 phys% AND T1 added cold) make this strategy 2× cheaper because either fracture is a win.
- **Recovery (bricked fracture):**
  - Sell the bricked rare (often recovers 30–50% of fracturing cost).
  - DO NOT continue investing chaos orbs into a fracture you didn't want — bin it.
- **Expected cost:**
  - Profitable case (ES helmet with 2 T1 ES prefixes): ~20 div fracture + ~28 div chaos = **~48 div total**, sells for 50+ div.
  - Unprofitable case (triple T1 flat damage ring): ~150 div chaos expected — DO NOT do this method here.
- **Expected probability:** Fracture hit ~25%; once locked, ~50–80% the chaos spam is profitable depending on weight of target mods.
- **When to abandon:** Stop chaos spam if you've spent 50+ div without hitting two good opposite-side mods. Sell the 2-good-mod-on-fracture state as a crafting base to another player.

---

### 3. Annul-Augment Spam (Magic Base Refinement)

- **Source:** Standard Path of Exile crafting move; in PoE2 0.4 specifically validated by Belton as the way to preserve **Homogenising Coronation** value (one of the few reasons magic items still matter post-0.4).
- **Item type:** Magic items destined to become rare — best for high-ilvl bases targeting low-weight mods (e.g., +3 Spell Skills on amulets, T1 crit on weapons).
- **Preconditions:** White base; access to Greater/Perfect Transmute (ilvl 55/70 minimum mod level), Augment, Annulment.
- **Currencies:** Perfect Transmute, Perfect Augment, Orbs of Annulment, optionally Homogenising Coronation + Regal at the end.
- **Procedure:**
  1. Perfect Transmute the white base (mod is min mod-level 70).
  2. If desired mod hit: keep it; if not, scour and retry.
  3. Perfect Augment to add second mod. If both are desired → you have a 2-mod magic item, ready to Regal.
  4. If one is junk: Annul. With only 2 mods, Annul is a 50/50.
  5. If you Annul the wrong one: Augment again to refill.
  6. Loop until you have exactly the 2 mods you want.
  7. Apply Homogenising Coronation (if you stocked them) + Regal Orb → forces the 3rd mod to share a tag with existing mods.
- **Decision points:**
  - Use this as a *Fracture seed* (rare-state with 1 great mod → Fracture) instead of going to rare directly when chasing very low-weight mods.
- **Recovery:** Bricked? Just scour or vendor; magic items are cheap to retry.
- **Expected cost:** 1–5 div per successful 2-mod magic seed.
- **Probability:** Highly mod-dependent; ~10–30% per attempt to land your specific 2-mod combo on a Perfect Transmute/Augment.
- **When to abandon:** If after 10+ Perfect Transmute attempts you haven't even hit one half of your combo, the mod weight is too low — switch base or strategy.

---

### 4. Greater Essence Regal Lock-In

- **Source:** PaintMasterPoE (Mobalytics essence guide, updated for 0.4), reinforced by Reddit r/Poe2BudgetCraftGuide sticky.
- **Item type:** Any rare; most efficient on jewelry, body armour, weapons.
- **Preconditions:** Magic base with 2 good mods you've already secured (e.g., via Annul-Augment Spam #3); Greater Essence of the desired guaranteed mod.
- **Currencies:** Greater Essence (cost varies: 1–10 div per type), then 3× Exalts.
- **Procedure:**
  1. Hold a 2-mod magic item with 2 good mods.
  2. Apply Greater Essence — converts to rare, adds the guaranteed essence mod (T3 typical for Greater) on its prefix/suffix side.
  3. Item is now a rare with 3 known mods. Slam Greater/Perfect Exalts × 3 to fill.
- **Decision points:**
  - Pick essence side carefully: e.g., Essence of Body is a prefix (life on armour) — if your magic item has 2 prefixes already, the essence will replace one. Use the magic-side awareness from PaintMaster.
  - Pair with Sinistral/Dextral Exaltation omens during slams to bias remaining mods.
- **Recovery:** If the essence overwrote a desired mod (because rare was full on that side) — sell as scraps.
- **Expected cost:** 3–15 div total.
- **Probability:** ~50–70% to a usable rare given 2 good mods entering.
- **When to abandon:** If essence's resulting rare has ≤ 1 desirable mod, sell it.

---

### 5. Perfect Essence + Crystallisation (Isolation Crafting Variant)

- **Source:** sirgog (Mobalytics isolation-crafting guide), refined for 0.4 by Belton.
- **Item type:** Near-finished rares where you want to swap exactly one mod.
- **Preconditions:** Rare with 5–6 mods, ≥3 good mods on one side (the "safe" side), 1+ junk mod on the other side.
- **Currencies:** Perfect Essence (specific to desired mod), Omen of Sinistral Crystallisation (removes prefix only) OR Omen of Dextral Crystallisation (removes suffix only).
- **Procedure:**
  1. Identify which side has the junk mod (e.g., 3 great suffixes locked, junk prefix to remove).
  2. Activate Omen of Sinistral Crystallisation (since junk is a prefix).
  3. Apply Perfect Essence (must add a prefix-type mod — match the side).
  4. Result: a junk prefix is removed and replaced by the essence's guaranteed mod on the prefix side. Suffixes are untouched.
- **Decision points:**
  - **Critical compatibility:** Perfect Essence cannot be used on an item already carrying that essence's specific mod (e.g., can't Perfect Essence of Sorcery onto a staff with +X Cold Spell levels existing).
  - If item has open prefix slot AND junk prefix, the Crystallisation omen still ensures it removes a prefix — but it could pick the open slot's "ghost" or the junk; the system targets the existing junk first. Confirmed working in 0.3 community testing.
- **Recovery:** If the wrong prefix was removed: refill via Sinistral Exaltation + Greater Exalt slam.
- **Expected cost:** 5–20 div per swap (Perfect Essences are expensive; Crystallisation omens 0.5–2 div).
- **Probability:** ~95% to land the intended swap; ~5% edge cases when other prefixes are present.
- **When to abandon:** If item has 3 prefixes already and you're not certain which one will be replaced, lock with Hinekora's first.

---

### 6. Greater Exaltation Stacking

- **Source:** Confirmed mechanic via PoE2DB + Mobalytics omen-crafting guide. Pairs explicitly with Perfect Exalts post-0.4 since Homogenising is gone.
- **Item type:** Rares with 3–4 mods, room for 2–3 more.
- **Preconditions:** Rare item with open slots; Omen of Greater Exaltation in inventory.
- **Currencies:** Omen of Greater Exaltation (0.5–3 div), Greater/Perfect Exalt, optional Sinistral/Dextral Exaltation.
- **Procedure:**
  1. Activate Omen of Greater Exaltation (next Exalt adds **2** mods instead of 1).
  2. Optionally also activate Sinistral OR Dextral Exaltation to force both new mods to one side.
  3. Slam Perfect Exalt — adds 2 mods at min-mod-level 50.
- **Decision points:**
  - Best on items at 3 or 4 mods (jumps to 5 or 6). Cannot use if you only have 1 open slot.
  - Stacks with Catalysing Exaltation on jewelry (see #15).
- **Recovery:** If both mods are junk: this is the riskiest single-click — use Hinekora's Lock to preview first if you have one.
- **Expected cost:** 1.5–5 div per slam.
- **Probability:** Highly variable; ~10–25% to land 2 desired mods without targeting tools.
- **When to abandon:** If both junk and removable side has 3 desired mods — Sinistral/Dextral Eraser the entire opposite side and rebuild.

---

### 7. Whittling Cleanup

- **Source:** Mobalytics omen-crafting guide; codified usage rules by Belton ("Whittle to surgically remove the lowest-ilvl mod").
- **Item type:** 5–6 mod rares with one specific lower-ilvl junk mod.
- **Preconditions:** Item where the unwanted mod has a strictly lower **mod-level** (not tier) than every other mod on the item. Critical: "lowest level" refers to the mod's spawn-level requirement on PoE2DB, not its visible tier (T1 fire res ilvl 82 outranks a T1 chaos res at ilvl 81).
- **Currencies:** Omen of Whittling (3–10 div), 1× Chaos Orb (Greater/Perfect ideal).
- **Procedure:**
  1. Look up each mod's mod-level on poe2db.tw.
  2. Confirm exactly one mod is the strict minimum.
  3. Activate Omen of Whittling. Slam Chaos Orb — removes that lowest-ilvl mod and adds a random new one.
- **Decision points:**
  - If 2 mods tie for lowest mod-level: **50/50** which gets removed. Risky.
  - Pair with Sinistral/Dextral Erasure to bias the *new* mod's side (per Belton).
  - Use a regular Chaos (not Greater/Perfect) when the goal is to bias the new mod toward *lower* ilvl than your fracture target — this avoids "bricking up" further.
- **Recovery:** If new added mod is also junk: re-Whittle (now a different mod is lowest). Cycle until success.
- **Expected cost:** 5–15 div per successful Whittle (omen + chaos + retries).
- **Probability:** ~90% to remove the intended mod (when there's a clear minimum); the *new* mod is RNG and frequently still bad.
- **When to abandon:** After 4–5 Whittling cycles without hitting a desired new mod.

---

### 8. Sinistral / Dextral Erasure (Side-Targeted Chaos)

- **Source:** Mobalytics omen-crafting; widely used by Belton, Mosey, Tarke Cat in profit crafts.
- **Item type:** Rares where one entire side (3 prefixes OR 3 suffixes) is good and the other has 2-3 garbage.
- **Preconditions:** Rare with locked-in good side, junk on opposite side; Omen of Sinistral/Dextral Erasure.
- **Currencies:** Omen of Sinistral/Dextral Erasure (0.5–2 div), Chaos Orbs.
- **Procedure:**
  1. Activate Sinistral Erasure (chaos removes only prefix) or Dextral Erasure (suffix only).
  2. Slam Chaos Orb — removes one mod from targeted side, adds one new mod (random side, but if all 3 of opposite side full, lands on the targeted side).
  3. Repeat until clean.
- **Decision points:**
  - Pair with Whittling (#7) to also force the lowest-ilvl side mod out, not just any.
  - **Crucial pairing:** With one fractured mod on the target side, Erasure cannot remove the fracture — guaranteed safe-side targeting.
- **Recovery:** Each Chaos refills the slot — risk is the new mod is also bad. Loop.
- **Expected cost:** 2–10 div per side-cleanup.
- **Probability:** ~33% per slam to land a desired mod on the cleaned slot.
- **When to abandon:** After ~10 cycles without hitting any good mod on the targeted side.

---

### 9. Omen of Light Desecration Cleanup

- **Source:** Mobalytics abyss-crafting guide; Belton's 0.4 video explicitly identifies this as the key "flex mod removal" mechanic.
- **Item type:** Any rare with an unwanted desecrated mod.
- **Preconditions:** Item has at least one Desecrated mod (revealed); Omen of Light + Orb of Annulment.
- **Currencies:** Omen of Light (1–3 div), Orb of Annulment.
- **Procedure:**
  1. Activate Omen of Light (next Annul removes only Desecrated mods).
  2. Slam Orb of Annulment — guaranteed to remove only desecrated mods (if multiple desecrated mods, picks one at random among them; if only one, deterministic).
- **Decision points:**
  - This is the keystone trick that turns desecrated mods into a **"flex multi-tool"** (Belton): use a desecrated mod as a temporary blocker, tag-feeder, ilvl-prop, or chaos target, then strip it later with Omen of Light + Annul without risk to organic mods.
  - Combine with isolation: if you have 5 organic + 1 desecrated, Light+Annul is 100% safe; pure Annul would 1/6 each.
- **Recovery:** N/A — operation is deterministic when only one desecrated mod exists.
- **Expected cost:** 1–4 div per cleanup.
- **Probability:** 100% (single desecrated mod scenario).
- **When to abandon:** Never — if you have an Omen of Light, this is always the right way to remove a desecrated mod.

---

### 10. Hinekora's Lock Save-State (Preview)

- **Source:** Brought into PoE2 in patch 0.3 (Mobalytics 0-3-crafting-changes); foundational PoE wiki entry.
- **Item type:** Any item; especially valuable for high-stakes Annul / Chaos / Vaal / Divine moments.
- **Preconditions:** Hinekora's Lock currency item; uncorrupted, unmirrored, unsanctified target item.
- **Currencies:** 1× Hinekora's Lock (rare drop / 5× Slumbering Beast div card).
- **Procedure:**
  1. Apply Lock to item — purple icon appears, item enters foreseeing state.
  2. Hover any currency over the item to preview the outcome (deterministic preview — same outcome for same currency type).
  3. **If acceptable:** click to apply — currency consumed, lock consumed, mod applied per preview.
  4. **If unacceptable:** apply a "harmless" non-modifying currency to clear the lock without losing the item state — most commonly a Tailoring Orb (re-rolls socket on body armour) or anointing/re-anointing a ring/amulet. Then re-Lock and try again.
- **Decision points:**
  - Lock CANNOT preview crafting bench, recombinator, beastcrafting, corruption altar.
  - Same-name currencies (any Chaos Orb) all show the same preview — but Awakener's Orb varies by donor item.
  - Recommended: stockpile multiple potential currencies, hover each, decide best path.
- **Recovery:** N/A — Lock is itself a recovery tool.
- **Expected cost:** 5–20 div per Lock (extremely valuable; treat as a single-use insurance policy).
- **Probability:** 100% deterministic (you literally see the result).
- **When to abandon:** If the preview is bad and you have no "harmless" clearing currency available — must commit or destroy item via re-mirror/corrupt path.

---

### 11. Sanctification Finish (Mirror-Tier Polish)

- **Source:** Mobalytics 0.3 crafting changes (Lolcohol); Belton's "Sanctifying a Mirror Amulet With Locks Until +4 Projectile Skills" video.
- **Item type:** Already-perfect 5–6 T1 rares destined for the mirror service.
- **Preconditions:** Item is fully crafted, all mods desired, no further crafting planned. Omen of Sanctification + Divine Orb.
- **Currencies:** Omen of Sanctification (extremely rare, 50+ div), 1× Divine Orb. Pair with Hinekora's Lock for re-rolls.
- **Procedure:**
  1. Activate Omen of Sanctification.
  2. Apply Divine Orb to the rare item — each modifier value rerolled to a value between 80%–120% of its normal range. Each rolls independently.
  3. Item gains "Sanctified" tag — **permanently locked from all further crafting**.
- **Decision points:**
  - Pair with Hinekora's Lock to preview the Sanctification result before committing — re-Lock with non-destructive currency until satisfied.
  - Best on items where one specific modifier matters far more than others (a +X to Skill Level pushed to +X+1 via 120% roll).
  - Cannot Sanctify fractured mods past their normal max (fracture's value is locked at original roll).
- **Recovery:** None — Sanctified is permanent. If outcome is suboptimal, the item is locked at that state forever.
- **Expected cost:** 60–200 div per attempt (omen dominates).
- **Probability:** ~30% to push at least one critical mod from max → above max; ~10% to hit god-tier on multiple mods.
- **When to abandon:** Only Sanctify items already worth ≥3× the omen cost.

---

### 12. Vaal Corruption Finish

- **Source:** sirgog Mobalytics vaal-corrupting guide; the standard PoE2 item-finishing technique.
- **Item type:** Finalized rares, all crafting complete; uniques where you want to push beyond normal stats; gems for +1 / extra socket.
- **Preconditions:** Item is crafting-complete; you accept the 25% no-change / 25% brick / 25% enchant / 25% extra-socket-or-mod-shuffle outcome distribution.
- **Currencies:** 1× Vaal Orb (very cheap), optional 1× Omen of Corruption (1+ div, removes the no-change outcome).
- **Procedure:**
  1. Apply quality, runes, and any Greater/Perfect crafts FIRST (Vaal locks the item).
  2. Optional: activate Omen of Corruption to eliminate the 25% no-change roll (3-outcome distribution remains).
  3. Slam Vaal Orb. Outcomes per Mobalytics vaal-corrupting guide:
     - 25% no change (or 0% with omen)
     - 25% chaos-orb-equivalent shuffle (often ruins the item) — with omen this becomes 1/3
     - 25% special enchant (helmet +1% max res, weapon +1 to skill levels, etc.)
     - 25% extra rune socket (armour/martial weapons) OR ±10% caster weapon quality
- **Decision points:**
  - Per sirgog: only use Omen of Corruption if the item is worth at least 3× the omen cost (otherwise it's mathematically equivalent to just re-rolling 4 items vs 3 items + 3 omens).
  - Waystones Vaal extremely well — Vaal early and often.
  - Gems: use Omen of Corruption only when the gem already has Perfect Jeweller's Orb sockets (very high value).
- **Recovery:** None — corrupted items can no longer be crafted on (excluding double-corrupt path #13).
- **Expected cost:** 0.1–1 div per Vaal; +1–5 div for Omen of Corruption.
- **Probability:** ~50% net positive outcome with Omen of Corruption (per Mobalytics outcome distribution).
- **When to abandon:** If item is a backup-tier piece worth less than 5 div, Vaal freely; if it's mirror-tier, Hinekora's Lock the corruption preview first (note: Lock cannot preview Altar of Corruption, so for raw Vaal Orb only).

---

### 13. Double Corruption (Twice-Corrupt) — NEW IN 0.4

- **Source:** Patch 0.4 league mechanic Fate of the Vaal; AOEAH double-corrupt guide.
- **Item type:** Already-corrupted rares, gems, uniques (especially Vaal-themed uniques).
- **Preconditions:** Item already corrupted; access to the Corruption Altar via Vaal Temple Tier 3 Sacrificial Chamber, OR the portable currencies: **Architect's Orb** (equipment/jewels), **Crystallised Corruption** (gems), or **Vaal Cultivation Orb** (uniques).
- **Currencies:** 1× Architect's Orb / Crystallised Corruption / Vaal Cultivation Orb (rare; 3–30 div depending on type).
- **Procedure:**
  1. Build Vaal Temple, upgrade Sacrificial Chamber to Tier 3 by placing adjacent green-highlighted rooms (e.g., Flesh Surgeon).
  2. Itemize the room into a portable orb at the temple.
  3. Apply orb to corrupted item.
  4. **50% chance the item is destroyed outright.**
  5. If survives: applies a second corruption-style outcome (extra socket, second enchant, second mod-shuffle, etc.).
- **Decision points:**
  - **Vaal Cultivation Orb on Vaal-themed uniques:** can replace up to 2 modifiers from a unique-specific pool. Sometimes removes downsides.
  - **Vaal Cultivation on non-Vaal uniques:** REPLACES the entire unique with a random corrupted unique of the same item class (very volatile).
  - Stack multiple temples for multi-attempt strategy on high-value targets.
- **Recovery:** The 50% destroy is total — consider buying a pre-twice-corrupted version of the item you want instead.
- **Expected cost:** 5–50 div per attempt; on mirror-tier items the EV often negative.
- **Probability:** 50% survive → of survivors, ~30–40% achieve a meaningful upgrade (community observation).
- **When to abandon:** Don't double-corrupt your only copy of an irreplaceable item. Always have a backup or accept the loss.

---

### 14. Bones with Abyssal Echoes (Reroll Reveal Options)

- **Source:** Mobalytics abyss-crafting (Lolcohol); central to Belton's 0.4 four-T1 rubric.
- **Item type:** Any rare matching the bone type (Jawbone-weapon, Rib-armour, Collarbone-jewelry, Cranium-jewel, Vertebrae-waystone).
- **Preconditions:** Rare item with at least one open mod slot (ideally fewer than 6 mods); Preserved or Ancient bone of correct type; Omen of Abyssal Echoes (rare-ish drop).
- **Currencies:** 1× Preserved/Ancient Bone (3–15 div for Ancient), 1× Omen of Abyssal Echoes (2–5 div).
- **Procedure:**
  1. Activate Omen of Abyssal Echoes.
  2. Optionally also activate Sinistral/Dextral Necromancy to bias prefix vs suffix.
  3. Apply Bone — adds an unrevealed desecrated mod.
  4. Travel to Well of Souls (Act 2 Mastodon Badlands → Lightless Passage).
  5. Reveal — get **3 options**.
  6. With Echoes active, click "reroll" once to get **3 more options** (you can still select from the original 3).
  7. Pick the best of up to 6 options. Confirm.
- **Decision points:**
  - **Bone tier matters:**
    - Gnawed: ≤ ilvl 64 only (campaign tier).
    - Preserved: any ilvl, random tier.
    - Ancient: any ilvl + guarantees min mod-level 40 (≈T3+).
  - Use Sinistral Necromancy (prefix-only) if you specifically need a prefix.
  - If item has 6 mods, bone removes 1 random mod first (matching prefix/suffix to what it adds) — risky, prefer using it on items with open slots.
- **Recovery:** If all 6 reveal options are bad — you must pick one. Then use Omen of Light + Annul (#9) to remove it cleanly.
- **Expected cost:** 5–20 div per reveal.
- **Probability:** With Echoes (6 options) and full mod pool, ~70–85% to get a usable mod.
- **When to abandon:** Out of Echoes omens AND target mod is rare — better to use an Abyss Lord omen (#15).

---

### 15. Liege / Blackblooded / Sovereign Omens (Abyss Lord-Specific)

- **Source:** Mobalytics abyss crafting; PoE2DB modifier list.
- **Item type:** **Weapons or jewelry only** (cannot be used on armor — confirmed bug-fixed in patch 0.3.x).
- **Preconditions:** Rare weapon or jewelry; the appropriate omen; bone of correct type.
- **Currencies:**
  - Omen of the Liege (Amanamu pool) — 5–15 div.
  - Omen of the Blackblooded (Kurgal pool) — 5–15 div.
  - Omen of the Sovereign (Ulaman pool) — 5–15 div.
  - Plus 1× Preserved or Ancient Jawbone/Collarbone.
- **Procedure:**
  1. Choose the Lord whose mod pool you want (Amanamu / Kurgal / Ulaman — see PoE2DB Desecrated Modifiers list for each).
  2. Activate that Lord's omen.
  3. Optionally also activate Sinistral or Dextral Necromancy + Echoes.
  4. Apply Bone — desecrated mod is **guaranteed** to be from that Lord's pool.
  5. Reveal at Well of Souls.
- **Decision points:**
  - The 3 reveal options are all from the chosen Lord's pool. Echoes still gives 6 options total.
  - Use Liege for Amanamu's powerful summoner mods (perma-minions on kill); Sovereign for Ulaman's defensive endgame mods; Blackblooded for Kurgal's offensive scaling mods.
- **Recovery:** Pick least-bad option from 3/6, then Light+Annul.
- **Expected cost:** 8–25 div per attempt (omen + bone).
- **Probability:** ~95% to land a Lord-pool mod; chance the specific desired Lord mod is in the 3/6 options is mod-pool-dependent (~30–60%).
- **When to abandon:** If you've burned 3+ Lord omens on the same item without success — rare-mod-targeting fatigue; sell the partial item.

---

### 16. Recombinator Strategy (Omen of Recombination)

- **Source:** sirgog (Mobalytics omen-of-recombination guide with the full math); IGN recombination guide; Belton's quiver crafts.
- **Item type:** Two same-class items (both rares or both magics) where you want to consolidate mods.
- **Preconditions:** Two items of identical class (e.g., two convoking wands), each carrying mods you want.
- **Currencies:** Optional Omen of Recombination (40+ exalt as of 0.3 publication).
- **Procedure:**
  1. Combine two source items at the Recombinator.
  2. Each desired mod has a transparent probability shown.
  3. Optional: activate Omen of Recombination — makes the next recombine **Lucky** (rolls twice, takes the better outcome).
  4. Confirm. Result is one consolidated item with new mods drawn from both sources.
- **Decision points (sirgog math):**
  - "Lucky" gives the biggest absolute boost when base odds are near 50% (~+25 percentage points at 50%).
  - **Decision rule:** use the Omen of Recombination iff `(failure chance × cost-to-replace-inputs) ≥ omen cost`.
  - At 5% base success, Omen "refunds" 95% of input cost — almost always worth on expensive crafts.
  - At 40% base success, Omen "refunds" 60% — borderline.
  - Skip the omen on cheap recombinations.
- **Recovery:** Failed recombine produces a magic or worse item — sell as base material.
- **Expected cost:** 10–500 div for inputs depending on rarity; +40–80 ex for the omen.
- **Probability:** Transparent per-mod % shown in-game; Lucky doubles it (with cap).
- **When to abandon:** If your inputs are below 5 div total, never use the omen.

---

### 17. Magic Base Exit Strategy (Sell at Magic)

- **Source:** Belton 0.4 video (explicit framing); MMOJUGG 0.4 crafting guide.
- **Item type:** High-ilvl bases (especially ilvl 82 weapons, body armour, jewelry) where the magic-stage 2-mod combo is itself valuable for further crafting.
- **Preconditions:** Identify rare 2-mod combos that downstream crafters want as fracture seeds — e.g., +X to Spell Levels + T1 Cast Speed on a staff; T1 Phys% + T1 Crit on a weapon.
- **Currencies:** Perfect Transmute, Perfect Augment, Annulment.
- **Procedure:**
  1. Run Annul-Augment Spam (#3) on white Perfect-tier bases.
  2. Once you hit a 2-mod magic with both mods being chase-tier — **STOP**.
  3. List on trade for 5–30 div.
- **Decision points:**
  - Don't Regal yourself unless you have the full 5-step pipeline planned. The 2-mod magic state is often worth more than a 3-mod rare with junk Regal.
  - In 0.4 specifically: the loss of Homogenising Coronation makes good 2-mod magics MORE valuable (downstream crafters need them as direct fracture seeds).
- **Recovery:** N/A — selling is the recovery.
- **Expected cost:** 0.5–2 div per attempt; recovers via sale.
- **Probability:** ~5–15% to hit a sellable 2-mod combo per Perfect Transmute/Augment cycle.
- **When to abandon:** When a buyer exists at a price that makes sense — don't gamble further if a profit is on the table.

---

### 18. Catalyst-Boosted Exaltation (Catalysing Exaltation)

- **Source:** Mobalytics omen-crafting guide (function partially uncertain); Reddit testing thread r/PathOfExile2 1o109h1 ("scales on Quality amount").
- **Item type:** Rings and amulets only (Catalysts are jewelry-exclusive).
- **Preconditions:** Jewelry with high-quality catalyst applied to the type matching your desired mod (e.g., Flesh Catalyst for life mods, Reaver Catalyst for attack mods).
- **Currencies:** Catalysts (1–3 ex each, to 20% or 40% via Adaptive Catalyst), Omen of Catalysing Exaltation (1–3 div), Greater/Perfect Exalt.
- **Procedure:**
  1. Apply matching catalysts to ring/amulet up to 20% quality (or 40% with Adaptive Catalyst from breach).
  2. Activate Omen of Catalysing Exaltation.
  3. Optionally pair with Omen of Greater Exaltation for a 2-mod slam.
  4. Slam Exalt — the omen consumes all catalyst quality and increases the chance of the matching-tag mod.
- **Decision points:**
  - **Critical caveat (Mobalytics):** the exact magnitude of the chance increase is undocumented. 20% Life Quality does NOT guarantee a Life mod — testing shows it's a probability boost, not a lock.
  - Reddit testing on 100 Adaptive 40% rings vs 100 single-catalyst 5% rings showed clear scaling — higher quality = higher hit rate, but not deterministic.
  - Best EV on suffix slams where the targeted mod weight is already medium-high.
- **Recovery:** If misses: re-catalyst (quality is consumed) and retry, or accept the hit and continue.
- **Expected cost:** 3–10 div per attempt (catalysts + omen + exalt).
- **Probability:** Estimated 50–75% (per testing) to hit the targeted-tag mod at 40% quality vs ~25–35% baseline weighting.
- **When to abandon:** If you've used 3+ omens without a hit on a low-weight target mod — switch to essence approach.

---

### 19. Belton's Four-T1 Rubric (The 0.4 Master Pipeline)

- **Source:** Belton (`@BeltonPoE`), "How to Craft Items in Patch 0.4 Without Homogenized Omens" (Dec 10, 2025, 44min). The canonical post-Homogenising-removal guaranteed pipeline.
- **Item type:** Any base — weapons, body armour, helmets, gloves, boots, jewelry. Best on ilvl 82 bases.
- **Preconditions:** Pick 3 desired prefixes OR 3 desired suffixes ahead of time, all sharing similar target mod-levels. ilvl 82 base. Have access to: Fracturing Orb, bulk Chaos Orbs, Preserved or Ancient Bone, Sinistral or Dextral Necromancy, Omen of Abyssal Echoes, Greater/Perfect Exalts, Omens of Sinistral/Dextral Erasure, Omen of Light, Omen of Whittling.
- **Currencies (full rubric):**
  - 1× Fracturing Orb
  - ~5–30 Chaos Orbs (Greater preferred)
  - 1× Preserved or Ancient Bone
  - 1× Omen of Sinistral OR Dextral Necromancy
  - 1× Omen of Abyssal Echoes
  - 1× Greater or Perfect Exalt
  - Optional: Omen of Light + Annul, Omen of Whittling, Sinistral/Dextral Erasure for 5th and 6th mods.
- **Procedure (Belton's canonical 4-step rubric for guaranteed 4 T1):**
  1. **Mod #1 — Fracture the Rarest:** Get rare to 4 mods. Slam Fracturing Orb. Target the rarest of your 3 desired same-side mods. ~25–33% success per attempt; bricks on failure → vendor.
  2. **Mod #2 — Chaos Tag-Match:** Spam Chaos Orbs until you hit the second desired same-side mod. The fractured mod cannot be removed. Stop the moment the second target is on the item.
  3. **Mod #3 — Desecrate Third:** Activate Sinistral OR Dextral Necromancy (matching your target side) + Omen of Abyssal Echoes. Apply Preserved or Ancient Bone. Reveal at Well of Souls. Pick from 3 + 3 reroll = 6 options, including Abyss Lord exclusives if you used Liege/Blackblooded/Sovereign instead of Necromancy.
  4. **Mod #4 — Exalt or Essence Fourth:** First open slot on the *other* (now-empty) side. Choose:
     - **Path A (Exalt):** Greater/Perfect Exalt. With 3 desired options on this side, decent odds. Pair with Sinistral/Dextral Exaltation to force the side.
     - **Path B (Essence):** Apply Greater/Perfect Essence with appropriate Crystallisation omen.
- **Decision points & 5T1 / 6T1 extension:**
  - **For 5T1:** make Mod #4 the highest mod-level prefix/suffix among your desired three. Then for Mod #5 use Whittling (lowest-mod-level removed) — the lowest will deterministically be the new junk slam, removable. Essence-swap into the 5th if needed.
  - **For 6T1:** Belton's recommendation: desecrate the *6th* mod (not the 3rd) to lock an Abyss-exclusive on the final slot. This pot-commits you because Mod #3 (originally desecrated) becomes a regular Whittle/Essence target, but the final reveal pool is unique.
- **Recovery options:**
  - **Bricked fracture:** sell rare for 0.5–2 div recovery; restart on new base.
  - **Bad Chaos spam:** stop after ~30–50 div; sell as 2-mod fracture seed.
  - **Bad Bone reveal:** Omen of Light + Annul to strip; redesecrate (this is why Bones are "flex multi-tools" per Belton).
  - **Bad 4th Exalt slam:** Sinistral/Dextral Annulment + retry (50/50 to remove the right mod with item at 4 mods on one side).
  - **5T1 5th-mod conflict at higher mod-level than #4:** Eraser + plain Chaos (NOT Greater/Perfect) to bias the new mod toward lower ilvl.
- **Expected cost per 4T1 attempt:**
  - Fracture: 15–25 div (1 success per ~3–4 tries).
  - Chaos spam mod #2: 5–20 div.
  - Bone + omens for mod #3: 5–15 div.
  - Exalt for mod #4: 1–5 div.
  - **Total: ~30–60 div per finished 4T1 item.**
- **Expected cost for 5T1:** add 10–30 div for Whittling chains.
- **Expected cost for 6T1:** add 30–100 div for second-side completion (depends heavily on omen prices).
- **Probability:**
  - 4T1: **near 100%** if you accept multiple fracture attempts.
  - 5T1: ~70% per finished 4T1 base.
  - 6T1: ~30–50% per finished 5T1 base.
- **When to abandon:** Belton's rule: **never restart prefixes or suffixes that already hit 3T1.** A bricked 5T1 attempt sells as a 4T1+meh for profit; cycle infinitely.

---

### 20. The Mark of the Abyss Swap (Essence of the Abyss Combo)

- **Source:** PaintMaster Mobalytics essence guide; Mobalytics abyss-crafting; Belton's example flow for tier-up swaps.
- **Item type:** Any rare with 6 mods where you want to replace one specific lower-tier organic mod with a higher-tier desecrated mod.
- **Preconditions:** Item has 6 mods; one mod is junk; you have an Essence of the Abyss + a Bone + reveal omens.
- **Currencies:** Essence of the Abyss (5–20 div), Preserved/Ancient Bone, Sinistral/Dextral Crystallisation (to target side), Sinistral/Dextral Necromancy.
- **Procedure:**
  1. Activate Sinistral or Dextral Crystallisation (to ensure the essence removes a specific side).
  2. Apply Essence of the Abyss — removes a random mod (forced to chosen side) and adds "Mark of the Abyssal Lord" placeholder mod on that side.
  3. Activate Sinistral/Dextral Necromancy + Echoes.
  4. Apply Bone — guaranteed to **remove the Mark of the Abyssal Lord** and add a desecrated mod *of higher mod-level than the mod the Essence originally removed*.
  5. Reveal at Well of Souls — pick best of 3/6.
- **Decision points:**
  - The "higher mod-level" guarantee is what makes this powerful: lets you upgrade a low-tier organic into a guaranteed high-tier desecrated.
  - The Mark itself is item-level 1, so if you Whittle-spam afterward, the Mark becomes the lowest-level mod and is removed first — useful trick for non-Bone removal.
- **Recovery:** Mark stuck without Bone access → Whittle removes it cheaply.
- **Expected cost:** 10–30 div per swap.
- **Probability:** ~80% to get a desirable higher-tier mod from the desecration step.
- **When to abandon:** If your "junk" mod is already mid-tier and the upgrade cost exceeds the value gain.

---

## Cross-Cutting Meta-Strategies

### 21. ilvl 82 + Tri-Resist Convergence

- **Source:** Belton's 0.4 video, deep insight section.
- **Item type:** Helmets, boots, body armour, gloves, rings, belts, amulets.
- **Insight:** All T1 elemental resists (fire, cold, lightning) are mod-level 82 with **identical 1000 weights**. T1 chaos res is mod-level 81 at 250 weight (rarer).
- **Strategy:** On any item where tri-res is the goal, prioritize prefixes first. Then on the suffix side, Whittling chains will *deterministically* converge on the three element resists because they share top mod-level. You can hit 3T1 tri-res "blindfolded" by Whittling.
- **Use case:** Cheap end-game capping pieces. Sell tri-res 4T1 helmets/boots for 5–20 div consistently.

### 22. Wraeclast / Itemized Crafting Workflow Order

- **Source:** Belton (asymmetric prefix/suffix safety principle).
- **Insight:** Prefixes and suffixes function as **separate items** for crafting determinism. Once you have 3 desired prefixes (or 3 desired suffixes), the opposite side can be freely manipulated using Sinistral/Dextral tools without ever risking the locked side.
- **Strategy:** Always finish ONE side completely (3T1) before touching the other. Then your fail states only ever cost half the item, not the whole thing.

### 23. Exceptional Bases Exploit (0.3+ Carry-Over)

- **Source:** Mobalytics 0.3-crafting-changes (Lolcohol).
- **Insight:** Exceptional bases drop with quality up to 30% (vs 20% normal) or with extra rune sockets. They are the literal best bases in the game and should ALWAYS be preferred for endgame crafting.
- **Strategy:** Hold all crafting attempts until you find an Exceptional base in the relevant slot. The 50% extra quality compounds with every other craft.

---

## Strategy Decision Matrix

| Goal | Budget Tier | Recommended Strategy |
|---|---|---|
| Cheap leveling rare | <1 div | Apprentice Blueprint (#1) |
| Sellable mid-tier rare | 5–20 div | Greater Essence Regal Lock-In (#4) + Vaal (#12) |
| 4T1 endgame piece | 30–60 div | Belton's Four-T1 Rubric (#19) |
| 5–6T1 chase item | 100–500 div | Rubric (#19) + Whittling (#7) + Erasure (#8) extensions |
| Mirror-tier item | 500+ div | Rubric → Lock (#10) → Sanctification (#11) |
| Repair junk mod on near-finished | 5–20 div | Light+Annul (#9) or Mark of Abyss (#20) |
| Profit flip via 2-mod magic | 1–5 div input | Magic Base Exit Strategy (#17) |
| Unique upgrade | 10–50 div | Vaal (#12) → Double Corrupt (#13) |
| Recombinator hybrid | 10–500 div | Recombination (#16) |
| Insurance for big slam | 5–20 div | Hinekora's Lock (#10) before any high-stakes click |

---

## Sources

| # | Source | URL | Author/Date |
|---|---|---|---|
| 1 | Belton 0.4 crafting guide (the definitive 0.4 expert source) | https://www.youtube.com/watch?v=TSVHK8w2Gu0 | Belton, 2025-12-10 |
| 2 | MMOJUGG 0.4 homogenising-removal companion article | https://www.mmojugg.com/news/poe-2-homogenising-remove-and-patch-040-crafting-guide.html | MMOJUGG team, 2025-12-13 |
| 3 | Mobalytics Abyss Crafting Guide | https://mobalytics.gg/poe-2/guides/abyss-crafting | Lolcohol, 2025-09-17 |
| 4 | Mobalytics Isolation Crafting | https://mobalytics.gg/poe-2/guides/isolation-crafting | sirgog, 2025-09-09 |
| 5 | Mobalytics Fracturing Orbs | https://mobalytics.gg/poe-2/guides/fracturing-orbs | sirgog, 2025-03-30 |
| 6 | Mobalytics Omen of Recombination | https://mobalytics.gg/poe-2/guides/omen-of-recombination | sirgog, 2025-09-22 |
| 7 | Mobalytics Vaal Corrupting | https://mobalytics.gg/poe-2/guides/vaal-corrupting | sirgog, 2025-01-16 |
| 8 | Mobalytics Omen Crafting (master list) | https://mobalytics.gg/poe-2/guides/omen-crafting | Lolcohol, 2025-09-18 |
| 9 | Mobalytics 0.3 Crafting Changes | https://mobalytics.gg/poe-2/guides/0-3-crafting-changes | Lolcohol, 2025-09-19 |
| 10 | Mobalytics Essence Guide (updated 0.4) | https://mobalytics.gg/poe-2/guides/paintmasters-essence-farm | PaintMasterPoE, 2026-02-22 |
| 11 | POE2FUN Chaos Spam Stop Rules | https://poe2fun.com/guides/poe2-chaos-orb-crafting-when-to-stop | poe2fun |
| 12 | AOEAH Double Corruption 0.4 | https://www.aoeah.com/news/4309--how-to-double-twice-corrupt-in-poe-2-04 | AOEAH, 2026-01-09 |
| 13 | PoE Wiki Hinekora's Lock | https://www.poewiki.net/wiki/Hinekora%27s_Lock | community |
| 14 | Reddit r/Poe2BudgetCraftGuide 0.4 sticky | https://www.reddit.com/r/Poe2BudgetCraftGuide/comments/1plxqfq/crafting_in_04_in_general/ | community |
| 15 | Reddit 0.4 crafting changes thoughts | https://www.reddit.com/r/PathOfExile2/comments/1pfvyk2/some_thoughts_on_the_upcoming_crafting_changes_in/ | community |

---

## Notes for Advisor Engine Implementation

1. **Strategy selection should branch on `(item_state, goal_mod_count, budget_div, available_omens)`.** Use the Decision Matrix as the primary lookup.
2. **Each strategy's preconditions are hard requirements** — the Advisor must validate them before recommending.
3. **Recovery branches are first-class.** Most strategies have 1–3 named fail states with prescribed responses; encode these as state transitions, not just flat metadata.
4. **Costs and probabilities are estimates** drawn from sources written between Sept 2025 and Feb 2026. Economy fluctuates dramatically; the Advisor should ingest live trade prices via an external API for accurate EV calculations.
5. **Belton's Four-T1 Rubric (#19) is the current "central" 0.4 strategy.** Most other strategies are tributaries / variants of it. Implement it as the default suggestion path with the others as branches.
6. **0.4-specific changes flagged:** Homogenising omens removed (existing stockpiles still work); Double Corruption introduced; Vaal Cultivation Orb / Architect's Orb / Crystallised Corruption are new portable corruption tools.
