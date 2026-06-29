import { describe, expect, test } from "bun:test";
import { normalizeName } from "../src/prices/normalize";
import { parseCategoryPage } from "../src/prices/poe2scout";

describe("normalizeName", () => {
  test("lowercases, flattens punctuation, collapses + trims", () => {
    expect(normalizeName("  Greater   Vision-Rune!! ")).toBe("greater vision rune");
    expect(normalizeName("Mirror of Kalandra")).toBe("mirror of kalandra");
    expect(normalizeName("Aldur's Legacy")).toBe("aldur s legacy");
    expect(normalizeName("---")).toBe("");
  });

  test("matches what OCR'd names fold to (case/spacing drift)", () => {
    expect(normalizeName("DIVINE  ORB")).toBe(normalizeName("Divine Orb"));
  });
});

describe("parseCategoryPage", () => {
  const page = {
    CurrentPage: 1,
    Pages: 1,
    Total: 2,
    Items: [
      {
        ApiId: "divine-orb",
        Text: "Divine Orb",
        CurrentPrice: 200, // exalts
        IconUrl: "http://x/divine.png",
        ItemMetadata: { name: "Divine Orb", max_stack_size: 20 },
      },
      {
        ApiId: "aldurs-legacy",
        Text: "Aldur's Legacy",
        currentPrice: null, // no market data
        ItemMetadata: { name: "Aldur's Legacy", max_stack_size: 10 },
      },
    ],
  };

  test("derives divine price via DivinePrice and normalizes names", () => {
    const rows = parseCategoryPage(page, "Runes of Aldur", "runes", 200, "2026-06-29T00:00:00Z");
    expect(rows).toHaveLength(2);

    const divine = rows[0];
    expect(divine.apiId).toBe("divine-orb");
    expect(divine.normalizedName).toBe("divine orb");
    expect(divine.priceExalt).toBe(200);
    expect(divine.priceDivine).toBe(1); // 200 exalt / 200 exalt-per-div
    expect(divine.stackMax).toBe(20);
    expect(divine.league).toBe("Runes of Aldur");

    const rune = rows[1];
    expect(rune.priceExalt).toBeNull();
    expect(rune.priceDivine).toBeNull(); // null price → null divine, no crash
    expect(rune.normalizedName).toBe("aldur s legacy");
  });

  test("skips entries with no name", () => {
    const rows = parseCategoryPage(
      { Items: [{ ApiId: "x", currentPrice: 5 }] },
      "L",
      "currency",
      100,
      "t",
    );
    expect(rows).toHaveLength(0);
  });

  test("accepts both currentPrice (camel) and CurrentPrice (pascal)", () => {
    const rows = parseCategoryPage(
      { Items: [{ ApiId: "a", Text: "A", currentPrice: 50 }, { ApiId: "b", Text: "B", CurrentPrice: 50 }] },
      "L",
      "currency",
      50,
      "t",
    );
    expect(rows[0].priceDivine).toBe(1);
    expect(rows[1].priceDivine).toBe(1);
  });
});
