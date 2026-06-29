import { describe, expect, test } from "bun:test";
import { normalizeName } from "../prices/normalize";

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
