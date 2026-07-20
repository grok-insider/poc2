import { describe, expect, test } from "bun:test";
import { expandUncutGemQuery, parseUncutGem } from "../prices/uncutGems";
import { resolveAndPriceBatch } from "../ocr/scan";

describe("uncut gem OCR query expand", () => {
  test("bare support gem → Level 1 first", () => {
    const q = expandUncutGemQuery("Uncut Support Gem");
    expect(q[0]).toBe("Uncut Support Gem (Level 1)");
    expect(q).toContain("Uncut Support Gem");
  });

  test("levelled skill gem keeps level", () => {
    expect(parseUncutGem("Uncut Skill Gem (Level 14)")).toEqual({
      base: "Uncut Skill Gem",
      level: 14,
      canonical: "Uncut Skill Gem (Level 14)",
    });
  });

  test("OCR gel typo still parses", () => {
    expect(parseUncutGem("Uncut Support Gel")?.canonical).toBe(
      "Uncut Support Gem (Level 1)",
    );
  });
});

describe("resolveAndPriceBatch + uncut expand", () => {
  test("bare Uncut Support Gem resolves via Level 1 catalogue key", async () => {
    const catalogue = new Map([
      ["Uncut Support Gem (Level 1)", "uncut-support-1"],
      ["Uncut Skill Gem (Level 12)", "uncut-skill-12"],
    ]);
    const seen: string[] = [];
    const { reads } = await resolveAndPriceBatch(
      [
        { name: "Uncut Support Gem", quantity: 1 },
        { name: "Uncut Skill Gem (Level 12)", quantity: 1 },
      ],
      async (raws) => {
        seen.push(...raws);
        return raws.map((raw) => {
          const key = catalogue.get(raw) ?? null;
          return { key, score: key ? 1 : 0, method: key ? "exact" : "none" };
        });
      },
    );
    expect(reads.map((r) => r.key)).toEqual(["uncut-support-1", "uncut-skill-12"]);
    expect(seen).toContain("Uncut Support Gem (Level 1)");
    expect(seen).toContain("Uncut Skill Gem (Level 12)");
  });
});
