import { describe, expect, test } from "bun:test";
import { bigramSimilarity, cleanLines } from "../ocr";

describe("bigramSimilarity", () => {
  test("identical strings score 1", () => {
    expect(bigramSimilarity("Vaal Regalia", "Vaal Regalia")).toBe(1);
  });

  test("normalizes case and punctuation before comparing", () => {
    expect(bigramSimilarity("VAAL-REGALIA", "vaal regalia")).toBe(1);
  });

  test("OCR-garbled names still beat unrelated bases", () => {
    const garbled = bigramSimilarity("Vaal Regalla", "Vaal Regalia");
    const unrelated = bigramSimilarity("Sun Leather", "Vaal Regalia");
    expect(garbled).toBeGreaterThan(0.7);
    expect(unrelated).toBeLessThan(0.3);
    expect(garbled).toBeGreaterThan(unrelated);
  });

  test("sub-bigram strings fall back to exact equality", () => {
    expect(bigramSimilarity("a", "a")).toBe(1);
    expect(bigramSimilarity("a", "b")).toBe(0);
  });
});

describe("cleanLines", () => {
  test("collapses whitespace and fixes pipe-for-I confusions", () => {
    expect(cleanLines("Item   Level:  81\n|tem Class")).toEqual([
      "Item Level: 81",
      "Item Class",
    ]);
  });

  test("drops lines with fewer than 3 alphanumerics", () => {
    expect(cleanLines("--------\n+5\nVaal Regalia")).toEqual(["Vaal Regalia"]);
  });

  test("drops tooltip UI noise", () => {
    const raw = [
      "Alt to compare",
      "Shift click to unstack",
      "Ctrl+C price check",
      "+72 to maximum Life",
    ].join("\n");
    expect(cleanLines(raw)).toEqual(["+72 to maximum Life"]);
  });
});
