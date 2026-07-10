import { describe, expect, test } from "bun:test";
import {
  cleanModText,
  estimateValue,
  evaluateMods,
  parseItemInfo,
  rollPercent,
  shortCurrency,
  tierBadge,
} from "../overlay/evaluate";

describe("parseItemInfo", () => {
  test("reads rarity, item level, and required level", () => {
    const text = [
      "Item Class: Body Armours",
      "Rarity: Rare",
      "Vile Robe",
      "Widowsilk Robe",
      "--------",
      "Item Level: 80",
      "Requires Level 65",
    ].join("\n");
    expect(parseItemInfo(text)).toEqual({ rarity: "rare", ilvl: 80, requiresLevel: 65 });
  });

  test("handles 'Requires: Level 68, 111 Int' form", () => {
    const info = parseItemInfo("Rarity: Rare\nRequires: Level 68, 111 Int");
    expect(info.requiresLevel).toBe(68);
  });

  test("defaults to normal with nulls when absent", () => {
    expect(parseItemInfo("some junk")).toEqual({ rarity: "normal", ilvl: null, requiresLevel: null });
  });
});

describe("rollPercent", () => {
  test("mid-range roll → 0.5", () => {
    expect(rollPercent([{ value: 85, min: 80, max: 90 }])).toBeCloseTo(0.5, 5);
  });
  test("max roll → 1, min roll → 0", () => {
    expect(rollPercent([{ value: 90, min: 80, max: 90 }])).toBe(1);
    expect(rollPercent([{ value: 80, min: 80, max: 90 }])).toBe(0);
  });
  test("averages multiple stats", () => {
    const r = rollPercent([
      { value: 90, min: 80, max: 90 }, // 1
      { value: 80, min: 80, max: 90 }, // 0
    ]);
    expect(r).toBeCloseTo(0.5, 5);
  });
  test("degenerate single-value tier counts as full", () => {
    expect(rollPercent([{ value: 3, min: 3, max: 3 }])).toBe(1);
  });
  test("clamps out-of-range rolls to [0,1]", () => {
    expect(rollPercent([{ value: 100, min: 80, max: 90 }])).toBe(1);
    expect(rollPercent([{ value: 70, min: 80, max: 90 }])).toBe(0);
  });
  test("empty → null", () => {
    expect(rollPercent([])).toBeNull();
  });
});

describe("tierBadge", () => {
  test("prefix/suffix/implicit letters with ordinal", () => {
    expect(tierBadge("prefix", 2)).toBe("P2");
    expect(tierBadge("suffix", 3)).toBe("S3");
    expect(tierBadge("implicit", 1)).toBe("I1");
  });
  test("null tier drops the ordinal", () => {
    expect(tierBadge("prefix", null)).toBe("P");
  });
});

describe("cleanModText", () => {
  test("strips (min-max) range annotations", () => {
    expect(cleanModText("+96(80-91) to maximum Life")).toBe("+96 to maximum Life");
    expect(cleanModText("Adds 5(1-6) to 12(7-15) Fire Damage")).toBe("Adds 5 to 12 Fire Damage");
  });
  test("leaves plain lines untouched", () => {
    expect(cleanModText("38% total Elemental Resistance")).toBe("38% total Elemental Resistance");
  });
});

describe("evaluateMods", () => {
  test("advanced format yields tier badges + roll scores", () => {
    const text = [
      "Item Class: Body Armours",
      "Rarity: Rare",
      "Vile Robe",
      "--------",
      "Item Level: 80",
      "--------",
      '{ Prefix Modifier "Rounded" (Tier: 2) }',
      "+90(80-91) to maximum Life",
      '{ Suffix Modifier "of the Walrus" (Tier: 3) — Cold }',
      "+20(11-20)% to Cold Resistance",
    ].join("\n");
    const mods = evaluateMods(text, []);
    expect(mods).toHaveLength(2);
    expect(mods[0]).toMatchObject({ badge: "P2", text: "+90 to maximum Life" });
    expect(mods[0].roll).toBeCloseTo(0.909, 2);
    expect(mods[1]).toMatchObject({ badge: "S3", text: "+20% to Cold Resistance", roll: 1 });
  });

  test("basic format falls back to plain lines with empty badges", () => {
    const text = ["Rarity: Rare", "Vile Robe", "--------", "+85 to maximum Life"].join("\n");
    const mods = evaluateMods(text, ["+85 to maximum Life", "38% total Elemental Resistance"]);
    expect(mods).toEqual([
      { badge: "", text: "+85 to maximum Life", roll: null },
      { badge: "", text: "38% total Elemental Resistance", roll: null },
    ]);
  });
});

describe("estimateValue", () => {
  test("null when no priced listings", () => {
    expect(estimateValue([])).toBeNull();
    expect(estimateValue([{ amount: 0, currency: "divine" }])).toBeNull();
  });

  test("aggregates dominant currency into approx/range/reliability", () => {
    const listings = [
      { amount: 10, currency: "divine" },
      { amount: 12, currency: "divine" },
      { amount: 13, currency: "divine" },
      { amount: 14, currency: "divine" },
      { amount: 15, currency: "divine" },
      { amount: 16, currency: "divine" },
      { amount: 18, currency: "divine" },
      { amount: 20, currency: "divine" },
      { amount: 999, currency: "exalted" }, // minority currency, dropped
    ];
    const est = estimateValue(listings);
    expect(est).not.toBeNull();
    expect(est!.unit).toBe("divine");
    expect(est!.count).toBe(8);
    expect(est!.low).toBeLessThanOrEqual(est!.approx);
    expect(est!.approx).toBeLessThanOrEqual(est!.high);
    expect(est!.low).toBeGreaterThanOrEqual(10);
    expect(est!.high).toBeLessThanOrEqual(20);
    expect(est!.reliability).toBe("High");
  });

  test("few, wide listings → Low reliability", () => {
    const est = estimateValue([
      { amount: 1, currency: "divine" },
      { amount: 100, currency: "divine" },
    ]);
    expect(est!.reliability).toBe("Low");
  });
});

describe("shortCurrency", () => {
  test("maps known currencies to short forms", () => {
    expect(shortCurrency("divine")).toBe("div");
    expect(shortCurrency("exalted")).toBe("ex");
    expect(shortCurrency("chaos")).toBe("chaos");
    expect(shortCurrency("mirror")).toBe("mirror");
    expect(shortCurrency(null)).toBe("");
  });
});
