import { describe, expect, test } from "bun:test";
import {
  applyResolvedUncutLevel,
  normalizeLocaleLevelSuffix,
  resolveLocaleArg,
  splitLevelParen,
} from "../ocr/localePrep";
import { resolveAndPriceBatch } from "../ocr/scan";

describe("localePrep", () => {
  test("splitLevelParen handles Level and Nivel", () => {
    expect(splitLevelParen("Gema de apoyo sin tallar (Nivel 5)")).toEqual({
      base: "Gema de apoyo sin tallar",
      level: 5,
    });
    expect(splitLevelParen("Uncut Skill Gem (Level 12)")).toEqual({
      base: "Uncut Skill Gem",
      level: 12,
    });
    expect(splitLevelParen("Orbe del caos").level).toBeNull();
  });

  test("normalizeLocaleLevelSuffix rewrites Nivel → Level", () => {
    expect(normalizeLocaleLevelSuffix("Gema (Nivel 2)")).toBe("Gema (Level 2)");
  });

  test("applyResolvedUncutLevel rekeys bare uncut with OCR level", () => {
    expect(
      applyResolvedUncutLevel("Uncut Support Gem", "Gema de apoyo sin tallar (Nivel 3)"),
    ).toBe("Uncut Support Gem (Level 3)");
    expect(applyResolvedUncutLevel("Chaos Orb", "Orbe del caos (Nivel 2)")).toBe(
      "Chaos Orb",
    );
  });

  test("resolveLocaleArg omits English", () => {
    expect(resolveLocaleArg("en")).toBeUndefined();
    expect(resolveLocaleArg("sp")).toBe("sp");
    expect(resolveLocaleArg("xx")).toBeUndefined();
  });
});

describe("Spanish OCR path via resolve batch", () => {
  test("passes prepped names; locale is caller-supplied", async () => {
    const seen: { raws: string[]; locale?: string }[] = [];
    // Simulate engine: translate Spanish chaos orb when locale is sp
    const { reads } = await resolveAndPriceBatch(
      [
        { name: "Orbe del caos", quantity: 1 },
        { name: "Gema de apoyo sin tallar", quantity: 1 },
        { name: "Gema de habilidad sin tallar (Nivel 4)", quantity: 1 },
      ],
      async (raws) => {
        seen.push({ raws: [...raws] });
        return raws.map((raw) => {
          if (/orbe del caos/i.test(raw) || /chaos orb/i.test(raw)) {
            return { key: "Chaos Orb", score: 1, method: "exact" };
          }
          if (/gema de apoyo|uncut support/i.test(raw)) {
            return { key: "Uncut Support Gem", score: 1, method: "exact" };
          }
          if (/gema de habilidad|uncut skill/i.test(raw)) {
            return { key: "Uncut Skill Gem", score: 1, method: "exact" };
          }
          return { key: null, score: 0, method: "none" };
        });
      },
    );
    expect(reads[0]?.key).toBe("Chaos Orb");
    expect(reads[1]?.key).toBe("Uncut Support Gem");
    // Explicit Nivel 4 re-applied onto bare uncut key
    expect(reads[2]?.key).toBe("Uncut Skill Gem (Level 4)");
    // Candidate expansion includes stripped Spanish base for locale translate
    const flat = seen.flatMap((s) => s.raws);
    expect(flat.some((r) => /gema de habilidad sin tallar/i.test(r))).toBe(true);
  });
});
