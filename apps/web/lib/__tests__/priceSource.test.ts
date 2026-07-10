import { afterEach, describe, expect, test } from "bun:test";
import { normalizeName } from "../prices/normalize";
import { priceRow } from "../ocr/priceSource";

const globalWithWindow = globalThis as unknown as { window?: unknown };

afterEach(() => {
  delete globalWithWindow.window;
});

describe("price normalizeName (web)", () => {
  test("matches the desktop cache + Rust matcher normalization", () => {
    expect(normalizeName("  Greater   Vision-Rune!! ")).toBe("greater vision rune");
    expect(normalizeName("Mirror of Kalandra")).toBe("mirror of kalandra");
    expect(normalizeName("Orb of Transmutation")).toBe("orb of transmutation");
    expect(normalizeName("---")).toBe("");
  });

  test("an OCR'd display-name key folds to the cache key", () => {
    // The overlay looks up window.poc2PriceSource(matchedName); normalizing the
    // matched display name must equal the cache's normalizedName key.
    const cacheKey = normalizeName("Divine Orb");
    expect(normalizeName("DIVINE  ORB")).toBe(cacheKey);
    expect(normalizeName("divine orb")).toBe(cacheKey);
  });
});

describe("reward display units", () => {
  test("uses Divine when the stack total reaches one Divine", () => {
    globalWithWindow.window = {
      poc2PriceSource: () => ({
        perUnit: 150,
        unit: "ex",
        perUnitDivine: 0.75,
        perUnitExalt: 150,
      }),
    };
    const priced = priceRow({
      key: "rune",
      name: "Rune",
      quantity: 2,
      method: "exact",
      score: 1,
    });
    expect(priced).toMatchObject({ perUnit: 0.75, total: 1.5, totalDivine: 1.5, unit: "div" });
  });
});
