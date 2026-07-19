import { describe, expect, test } from "bun:test";
import { parseNinjaOverview } from "../src/prices/poeninja";

describe("parseNinjaOverview", () => {
  test("prefers divineValue, else derives from chaosValue / chaosPerDivine", () => {
    const overview = {
      lines: [
        { currencyTypeName: "Divine Orb", divineValue: 1, chaosValue: 200 },
        { currencyTypeName: "Chaos Orb", chaosValue: 10 }, // → 10/200 = 0.05 div
        { name: "Some Rune", chaosEquivalent: 50 }, // → 50/200 = 0.25 div
      ],
    };
    const rows = parseNinjaOverview(overview, "Runes of Aldur", 200, "2026-06-30T00:00:00Z");
    expect(rows).toHaveLength(3);

    expect(rows[0].name).toBe("Divine Orb");
    expect(rows[0].priceDivine).toBe(1);
    expect(rows[0].category).toBe("ninja");
    expect(rows[0].apiId).toBe("ninja:divine orb");
    expect(rows[0].normalizedName).toBe("divine orb");

    expect(rows[1].priceDivine).toBeCloseTo(0.05);
    expect(rows[2].normalizedName).toBe("some rune");
    expect(rows[2].priceDivine).toBeCloseTo(0.25);
  });

  test("skips lines with no name and lines with no derivable price", () => {
    const rows = parseNinjaOverview(
      {
        lines: [
          { chaosValue: 5 }, // no name
          { currencyTypeName: "No Price", chaosValue: 5 }, // chaosPerDivine 0 → skip
        ],
      },
      "L",
      0,
      "t",
    );
    expect(rows).toHaveLength(0);
  });

  test("missing/empty lines yields no rows (beta-surface safe)", () => {
    expect(parseNinjaOverview({}, "L", 100, "t")).toEqual([]);
    expect(parseNinjaOverview({ lines: [] }, "L", 100, "t")).toEqual([]);
  });
});
