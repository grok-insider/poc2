import { describe, expect, test } from "bun:test";
import { div, humanizeId, humanizeModId, modLines, pct } from "../format";

describe("humanizeId", () => {
  test("splits PascalCase ids into words", () => {
    expect(humanizeId("ChaosOrb")).toBe("Chaos Orb");
    expect(humanizeId("OmenOfWhittling")).toBe("Omen of Whittling");
  });

  test("lowercases the 'Of' in orb names", () => {
    expect(humanizeId("OrbOfAlchemy")).toBe("Orb of Alchemy");
    expect(humanizeId("GreaterEssenceOfRuin")).toBe("Greater Essence of Ruin");
  });

  test("keeps digits attached to their word", () => {
    expect(humanizeId("Tier2Rune")).toBe("Tier2 Rune");
  });
});

describe("modLines", () => {
  test("collapses [Id|Display] tags and roll ranges", () => {
    const tpl =
      "(80-91)% increased [EnergyShield|Energy Shield]\n+(7-10) to maximum [Life|Life]";
    expect(modLines(tpl)).toEqual([
      "#% increased Energy Shield",
      "+# to maximum Life",
    ]);
  });

  test("uses the id when a tag has no display text", () => {
    expect(modLines("Adds (1-3) [Lightning] Damage")).toEqual(["Adds # Lightning Damage"]);
  });

  test("handles null/undefined/empty templates", () => {
    expect(modLines(null)).toEqual([]);
    expect(modLines(undefined)).toEqual([]);
    expect(modLines("")).toEqual([]);
  });
});

describe("pct", () => {
  test("formats a probability as whole-number percent", () => {
    expect(pct(0.5)).toBe("50%");
    expect(pct(0.123)).toBe("12%");
    expect(pct(1)).toBe("100%");
    expect(pct(0)).toBe("0%");
  });
});

describe("div", () => {
  test("handles missing and zero costs", () => {
    expect(div(null)).toBe("—");
    expect(div(undefined)).toBe("—");
    expect(div(0)).toBe("free");
  });

  test("scales precision with magnitude", () => {
    expect(div(0.5)).toBe("0.50d");
    expect(div(5)).toBe("5.0d");
    expect(div(12.4)).toBe("12d");
  });

  test("reads the expected value from a DivEquiv", () => {
    expect(div({ min: 1, expected: 2.5, max: 4 })).toBe("2.5d");
  });
});

describe("humanizeModId", () => {
  test("strips the trailing group ordinal (not a display tier)", () => {
    expect(humanizeModId("IncreasedLife7")).toBe("Increased Life");
    expect(humanizeModId("LocalIncreasedEnergyShield1")).toBe("Local Increased Energy Shield");
    expect(humanizeModId("FireResist12")).toBe("Fire Resist");
  });

  test("leaves ids without ordinals unchanged", () => {
    expect(humanizeModId("OmenOfTheBlackblooded")).toBe("Omen of The Blackblooded");
  });
});
