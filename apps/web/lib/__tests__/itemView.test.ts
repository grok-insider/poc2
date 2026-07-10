import { describe, expect, test } from "bun:test";
import { buildItemView, parseCapture, type UniqueCatalog } from "../itemView";

const CATALOG: UniqueCatalog = {
  version: 1,
  entries: {
    facebreaker: {
      name: "Facebreaker",
      baseType: "Stocky Mitts",
      artRel: "Facebreaker.webp",
      requirements: ["Requires: Level 1"],
      implicits: [],
      explicits: [
        "Has 8 to 12 Physical damage, +3 to +4 per Boss's Face Broken",
        "(30—50)% increased Stun Buildup",
        "1% more Unarmed Damage per 5 Strength",
      ],
      flavour:
        '"You think us savages?" mused the Red Wolf, as he pulled teeth from the Eternal\'s skull.',
    },
    "forgotten warden": {
      name: "Forgotten Warden",
      baseType: "Primal Markings",
      artRel: "TheAncientOrder.webp",
      requirements: ["Requires: Level 70, 67 Dex, 67 Int"],
      grantsSkill: "Grants Skill: Level 16 Spirit Vessel",
      implicits: [],
      explicits: [
        "+(70—100) to Deflection Rating per 50 missing Energy Shield",
        "(200—300)% increased Evasion and Energy Shield",
      ],
    },
  },
};

const FACEBREAKER_SIMPLE = `
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
`.trim();

const FATE_BEAM = `
Item Class: Quarterstaves
Rarity: Rare
Fate Beam
Sinister Quarterstaff
--------
Quality: +20% (augmented)
Physical Damage: 156-258 (augmented)
--------
Requires: Level 67, 104 Dex, 41 Int
--------
Sockets: S S 
--------
Item Level: 81
--------
Gain 13% of Damage as Extra Chaos Damage (rune)
50% increased Attack Damage against Rare or Unique Enemies (rune)
--------
{ Desecrated Prefix Modifier "Cruel" (Tier: 3) — Damage, Physical, Attack }
136(135-154)% increased Physical Damage
{ Prefix Modifier "Electrocuting" (Tier: 2) — Damage, Elemental, Lightning, Attack }
Adds 8(1-16) to 300(239-300) Lightning Damage
{ Suffix Modifier "of the Panther" (Tier: 4) — Attribute }
+22(21-24) to Dexterity
--------
Corrupted
`.trim();

const WARDEN = `
Item Class: Body Armours
Rarity: Unique
Forgotten Warden
Primal Markings
--------
Quality: +20% (augmented)
Armour: 43 (augmented)
--------
Requires: Level 70, 67 Dex, 67 Int
--------
Sockets: S S 
--------
Item Level: 84
--------
{ Corruption Enhancement — Life }
+39(30-40) to maximum Life
--------
+30 to Armour (rune)
--------
Grants Skill: Level 18 Spirit Vessel
--------
{ Unique Modifier — Evasion }
+78(70-100) to Deflection Rating per 50 missing Energy Shield
--------
Corrupted
`.trim();

describe("parseCapture advanced rare", () => {
  test("reads prefix/suffix tiers and runes", () => {
    const cap = parseCapture(FATE_BEAM);
    expect(cap.kind).toBe("rare");
    expect(cap.name).toBe("Fate Beam");
    expect(cap.typeLine).toBe("Sinister Quarterstaff");
    expect(cap.advanced).toBe(true);
    expect(cap.flags.corrupted).toBe(true);
    expect(cap.sockets).toContain("S");
    const runes = cap.mods.filter((m) => m.side === "rune" || m.text.includes("(rune)"));
    expect(runes.length).toBeGreaterThanOrEqual(1);
    const tiered = cap.mods.filter((m) => m.tierLabel?.startsWith("T"));
    expect(tiered.length).toBeGreaterThanOrEqual(2);
  });
});

describe("buildItemView unique catalog match", () => {
  test("simple unique paste merges catalog mods + art", () => {
    const view = buildItemView(FACEBREAKER_SIMPLE, { uniqueCatalog: CATALOG });
    expect(view.uniqueMatched).toBe(true);
    expect(view.model.name).toBe("Facebreaker");
    expect(view.model.typeLine).toBe("Stocky Mitts");
    expect(view.artUrl).toBe("/unique-icons/Facebreaker.webp");
    const mods = view.model.sections.find((s) => s.type === "mods");
    expect(mods && mods.type === "mods" && mods.lines.length).toBeGreaterThanOrEqual(3);
  });

  test("unique advanced paste keeps runes + corruption + rolled unique", () => {
    const view = buildItemView(WARDEN, { uniqueCatalog: CATALOG });
    expect(view.uniqueMatched).toBe(true);
    const mods = view.model.sections.find((s) => s.type === "mods");
    expect(mods && mods.type === "mods").toBe(true);
    if (mods && mods.type === "mods") {
      const texts = mods.lines.map((l) => l.text);
      expect(texts.some((t) => t.includes("maximum Life"))).toBe(true);
      expect(texts.some((t) => t.includes("(rune)") || t.includes("Armour"))).toBe(true);
      expect(texts.some((t) => t.includes("Deflection") || t.includes("Spirit Vessel"))).toBe(
        true,
      );
    }
    expect(view.model.flags.corrupted).toBe(true);
  });

  test("rare uses capture path (no catalog)", () => {
    const view = buildItemView(FATE_BEAM, { uniqueCatalog: CATALOG });
    expect(view.uniqueMatched).toBe(false);
    expect(view.model.kind).toBe("rare");
    expect(view.model.name).toBe("Fate Beam");
    expect(view.capture.advanced).toBe(true);
    const tiered = view.capture.mods.filter((m) => m.tierLabel?.startsWith("T"));
    expect(tiered.length).toBeGreaterThanOrEqual(2);
  });

  test("missing unique catalog soft-fails: still parses unique from clipboard", () => {
    const view = buildItemView(FACEBREAKER_SIMPLE, { uniqueCatalog: null });
    expect(view.uniqueMatched).toBe(false);
    expect(view.model.kind).toBe("unique");
    expect(view.model.name).toBe("Facebreaker");
    // No throw; art may be null without manifests
    expect(view.artUrl).toBeNull();
  });

  test("unknown unique name without catalog entry still renders capture", () => {
    const text = `
Item Class: Gloves
Rarity: Unique
Not In Catalog
Stocky Mitts
--------
Armour: 10
--------
+5 to Strength
`.trim();
    const view = buildItemView(text, { uniqueCatalog: CATALOG });
    expect(view.uniqueMatched).toBe(false);
    expect(view.model.name).toBe("Not In Catalog");
    expect(view.model.kind).toBe("unique");
  });
});
