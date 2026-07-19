import { describe, expect, test } from "bun:test";
import {
  itemPopupFromClipboard,
  itemPopupFromItem,
  splitModValues,
} from "../itemPopup";
import type { Item } from "../types";

const FACEBREAKER = `
Item Class: Gloves
Rarity: Unique
Facebreaker
Stocky Mitts
--------
Ezomyte Gloves
Armour: 15
--------
Requires: Level 1
--------
Has 8 to 12 Physical damage, +3 to +4 per Boss's Face Broken
(30-50)% increased Stun Buildup
1% more Unarmed Damage per 5 Strength
+0.3 metres to Melee Strike Range while Unarmed
+1 to Armour per Strength
--------
"You think us savages?" mused the Red Wolf, as
he pulled teeth from the Eternal's skull. "I will
show your kind the way of tooth and claw."
`.trim();

const CRUDE_BOW = `
Item Class: Bows
Rarity: Normal
Crude Bow
--------
Physical Damage: 6-9
Critical Hit Chance: 5%
Attacks per Second: 1.2
`.trim();

const DIVINE = `
Item Class: Stackable Currency
Rarity: Currency
Divine Orb
--------
Stack Size: 1 / 10
--------
Randomises the numeric values of modifiers on an item
`.trim();

const RARE = `
Item Class: Body Armours
Rarity: Rare
Corruption Carapace
Sacrificial Mantle
--------
Quality: +20% (augmented)
Energy Shield: 410 (augmented)
--------
Requirements:
Level: 65
--------
Item Level: 81
--------
+85 to maximum Life
+30% to Cold Resistance (fractured)
--------
Corrupted
`.trim();

const GEM = `
Item Class: Support Gems
Rarity: Currency
Vilenta's Propulsion
Support
--------
Lineage, Spell, Projectile
Category: Projectile Speed
Requires: Level 65
Support Requirements: +5 Int
--------
Supports Spell Skills that fire Projectiles, causing increases and reductions to cast speed to also apply to projectile speed.
--------
75% of increases and reductions to Cast speed also apply to Projectile Speed for Supported Skills
--------
"Day and night, she hammered away."
--------
Place into a Skill's Support Gem socket in the Skills Panel to apply its effects to that Skill.
`.trim();

describe("itemPopupFromClipboard", () => {
  test("unique double-line Facebreaker shape", () => {
    const m = itemPopupFromClipboard(FACEBREAKER);
    expect(m.kind).toBe("unique");
    expect(m.doubleLine).toBe(true);
    expect(m.name).toBe("Facebreaker");
    expect(m.typeLine).toBe("Stocky Mitts");
    expect(m.properties.some((p) => p.label === "Armour" || p.text?.includes("Armour"))).toBe(true);
    expect(m.sections.some((s) => s.type === "mods")).toBe(true);
    expect(m.sections.some((s) => s.type === "flavour")).toBe(true);
  });

  test("normal Crude Bow properties only", () => {
    const m = itemPopupFromClipboard(CRUDE_BOW);
    expect(m.kind).toBe("normal");
    expect(m.doubleLine).toBe(false);
    expect(m.name).toBe("Crude Bow");
    expect(m.properties.length).toBeGreaterThan(0);
  });

  test("currency Divine Orb", () => {
    const m = itemPopupFromClipboard(DIVINE);
    expect(m.kind).toBe("currency");
    expect(m.name).toBe("Divine Orb");
    expect(m.sections.some((s) => s.type === "mods")).toBe(true);
  });

  test("rare double-line + corrupted flag", () => {
    const m = itemPopupFromClipboard(RARE);
    expect(m.kind).toBe("rare");
    expect(m.doubleLine).toBe(true);
    expect(m.name).toBe("Corruption Carapace");
    expect(m.typeLine).toBe("Sacrificial Mantle");
    expect(m.flags.corrupted).toBe(true);
  });

  test("gem Vilenta support shape", () => {
    const m = itemPopupFromClipboard(GEM);
    expect(m.kind).toBe("gem");
    expect(m.name).toBe("Vilenta's Propulsion");
    expect(m.sections.some((s) => s.type === "secDescr" || s.type === "mods")).toBe(true);
  });
});

describe("splitModValues", () => {
  test("extracts roll numbers for white spans", () => {
    const line = splitModValues("Has 8 to 12 Physical damage, +3 to +4 per Boss");
    expect(line.values).toEqual(expect.arrayContaining(["8", "12", "+3", "+4"]));
  });
});

describe("itemPopupFromItem", () => {
  test("maps craft item rarity and mods", () => {
    const item: Item = {
      base: "BodyArmour",
      ilvl: 82,
      rarity: "rare",
      corrupted: false,
      sanctified: false,
      mirrored: false,
      quality: 0,
      quality_kind: "Untagged",
      implicits: [],
      prefixes: [
        {
          mod_id: "IncreasedLife7",
          affix_type: "prefix",
          kind: "explicit",
          values: [85],
          is_fractured: false,
        },
      ],
      suffixes: [],
      enchantments: [],
      hidden_desecrated: null,
      sockets: [],
      hinekora_lock: null,
      base_display_name: "Hexer's Robe",
    };
    const m = itemPopupFromItem(item, { itemName: "Vile Shroud" });
    expect(m.kind).toBe("rare");
    expect(m.doubleLine).toBe(true);
    expect(m.name).toBe("Vile Shroud");
    expect(m.typeLine).toBe("Hexer's Robe");
  });
});
