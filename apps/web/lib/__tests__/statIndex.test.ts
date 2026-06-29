import { describe, expect, test } from "bun:test";
import {
  buildIndex,
  lineValues,
  matchLine,
  normalizeLine,
  type TradeStat,
} from "../trade/statIndex";

describe("normalizeLine", () => {
  test("a leading plus folds into the placeholder", () => {
    expect(normalizeLine("+85 to maximum Life")).toBe("# to maximum life");
    // …so a "+#"-style matcher template lands on the same key.
    expect(normalizeLine("+# to maximum Life")).toBe("# to maximum life");
  });

  test("decimals collapse to a single placeholder", () => {
    expect(normalizeLine("12.5% increased Spell Damage")).toBe(
      "#% increased spell damage",
    );
  });

  test("negatives keep the minus literal (negate-template form)", () => {
    expect(normalizeLine("-13% to Chaos Resistance")).toBe("-#% to chaos resistance");
  });

  test("multiple numbers each become #", () => {
    expect(normalizeLine("Adds 12 to 24 Fire Damage")).toBe("adds # to # fire damage");
  });

  test("whitespace collapses", () => {
    expect(normalizeLine("  +10   to\tStrength ")).toBe("# to strength");
  });

  test("case folds (scraped matcher casing drifts from clipboard text)", () => {
    expect(normalizeLine("+40 to Maximum Life")).toBe(normalizeLine("# to maximum Life"));
  });

  test("trailing tag suffixes are stripped", () => {
    expect(normalizeLine("+85 to maximum Life (fractured)")).toBe("# to maximum life");
    expect(normalizeLine("+12% to Fire Resistance (rune)")).toBe("#% to fire resistance");
    expect(normalizeLine("30% increased Rarity of Items found (enchant)")).toBe(
      "#% increased rarity of items found",
    );
    expect(normalizeLine("+1 to Level of all Minion Skills (desecrated)")).toBe(
      "# to level of all minion skills",
    );
    expect(normalizeLine("+20 to Spirit (crafted)")).toBe("# to spirit");
    expect(normalizeLine("+5% to all Elemental Resistances (implicit)")).toBe(
      "#% to all elemental resistances",
    );
  });

  test("stacked tags strip in sequence", () => {
    expect(normalizeLine("+85 to maximum Life (fractured) (implicit)")).toBe(
      "# to maximum life",
    );
  });

  test("lines without numbers pass through (case-folded)", () => {
    expect(normalizeLine("Bow Attacks fire an additional Arrow")).toBe(
      "bow attacks fire an additional arrow",
    );
  });
});

describe("lineValues", () => {
  test("magnitudes in order, decimals intact", () => {
    expect(lineValues("Adds 12 to 24.5 Fire Damage")).toEqual([12, 24.5]);
    expect(lineValues("+85 to maximum Life (fractured)")).toEqual([85]);
    expect(lineValues("no numbers here")).toEqual([]);
  });
});

// Inline fixture mirroring the trade-stats.json contract.
const STATS: TradeStat[] = [
  {
    ref: "# to maximum Life",
    better: 1,
    matchers: [{ string: "+# to maximum Life" }],
    ids: {
      explicit: ["explicit.stat_3299347043"],
      fractured: ["fractured.stat_3299347043"],
    },
  },
  {
    ref: "# Charm Slots",
    better: 1,
    matchers: [
      { string: "# Charm Slots" },
      { string: "# Charm Slot", value: 1, negate: true },
    ],
    ids: { explicit: ["explicit.stat_charm"] },
  },
  {
    ref: "#% to Chaos Resistance",
    better: 1,
    matchers: [
      { string: "+#% to Chaos Resistance" },
      { string: "-#% to Chaos Resistance", negate: true },
    ],
    ids: { explicit: ["explicit.stat_chaos"] },
  },
];

describe("buildIndex / matchLine", () => {
  const index = buildIndex(STATS);

  test("indexes every matcher under its normalized template", () => {
    expect(index.size).toBe(5);
    expect(index.get("# to maximum life")).toHaveLength(1);
  });

  test("template match extracts values in order", () => {
    const m = matchLine(index, "+85 to maximum Life");
    expect(m).not.toBeNull();
    expect(m!.stat.ref).toBe("# to maximum Life");
    expect(m!.values).toEqual([85]);
    expect(m!.matchedBy).toBe("template");
  });

  test("tag suffixes don't block matching", () => {
    expect(matchLine(index, "+85 to maximum Life (fractured)")!.values).toEqual([85]);
  });

  test("value-matcher matches only its pinned number", () => {
    const m = matchLine(index, "1 Charm Slot");
    expect(m).not.toBeNull();
    expect(m!.values).toEqual([1]);
    expect(m!.matchedBy).toBe("exact-value");
    // "2 Charm Slot" hits the same key but fails the pinned value.
    expect(matchLine(index, "2 Charm Slot")).toBeNull();
    // The plural template stays a normal match.
    expect(matchLine(index, "2 Charm Slots")!.values).toEqual([2]);
  });

  test("negate templates flip the extracted magnitude", () => {
    const m = matchLine(index, "-13% to Chaos Resistance");
    expect(m).not.toBeNull();
    expect(m!.stat.ref).toBe("#% to Chaos Resistance");
    expect(m!.values).toEqual([-13]);
    expect(matchLine(index, "+13% to Chaos Resistance")!.values).toEqual([13]);
  });

  test("unknown lines return null", () => {
    expect(matchLine(index, "Sound effects play 10% louder")).toBeNull();
  });
});
