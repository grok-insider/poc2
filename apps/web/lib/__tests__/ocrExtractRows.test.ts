import { describe, expect, test } from "bun:test";
import { extractRows, parseRow } from "../ocr/extractRows";

describe("parseRow — quantity multiplier", () => {
  test("trailing x-quantity: 'Chaos Orb x3'", () => {
    expect(parseRow("Chaos Orb x3")).toEqual({ name: "Chaos Orb", quantity: 3 });
  });

  test("trailing unicode ×: 'Divine Orb ×12'", () => {
    expect(parseRow("Divine Orb ×12")).toEqual({ name: "Divine Orb", quantity: 12 });
  });

  test("trailing 'Nx' form: 'Exalted Orb 5x'", () => {
    expect(parseRow("Exalted Orb 5x")).toEqual({ name: "Exalted Orb", quantity: 5 });
  });

  test("leading quantity: '3x Chaos Orb'", () => {
    expect(parseRow("3x Chaos Orb")).toEqual({ name: "Chaos Orb", quantity: 3 });
  });

  test("leading 'xN' form: 'x7 Vaal Orb'", () => {
    expect(parseRow("x7 Vaal Orb")).toEqual({ name: "Vaal Orb", quantity: 7 });
  });

  test("no multiplier defaults quantity to 1", () => {
    expect(parseRow("Orb of Annulment")).toEqual({
      name: "Orb of Annulment",
      quantity: 1,
    });
  });

  test("a bare trailing integer (ambiguous roll) is NOT treated as quantity", () => {
    // "Regal Orb 80" — 80 could be a roll value; keep it in the name.
    const row = parseRow("Regal Orb 80");
    expect(row?.quantity).toBe(1);
    expect(row?.name).toContain("Regal Orb");
  });
});

describe("parseRow — leading noise + rejection rules", () => {
  test("strips leading non-letter icon bleed", () => {
    expect(parseRow("•· 12 Chaos Orb")?.name).toBe("Chaos Orb");
  });

  test("rejects rows shorter than 4 chars", () => {
    expect(parseRow("Orb")).toBeNull();
    expect(parseRow("ab")).toBeNull();
  });

  test("rejects rows with no 4+-letter word", () => {
    expect(parseRow("x x x")).toBeNull();
    expect(parseRow("a1 b2 c3")).toBeNull();
    expect(parseRow("-- // --")).toBeNull();
  });

  test("keeps a row whose only long token qualifies", () => {
    expect(parseRow("of Chaos")).toEqual({ name: "of Chaos", quantity: 1 });
  });

  test("collapses internal whitespace", () => {
    expect(parseRow("Gemcutter's   Prism")?.name).toBe("Gemcutter's Prism");
  });

  test("empty / whitespace lines are null", () => {
    expect(parseRow("")).toBeNull();
    expect(parseRow("   ")).toBeNull();
  });
});

describe("extractRows — full blob", () => {
  test("parses a multi-line panel, dropping noise rows", () => {
    const blob = [
      "•••••••",
      "Divine Orb x2",
      "3x Chaos Orb",
      "++",
      "Exalted Orb",
      "  ",
      "x5 Vaal Orb",
    ].join("\n");
    expect(extractRows(blob)).toEqual([
      { name: "Divine Orb", quantity: 2 },
      { name: "Chaos Orb", quantity: 3 },
      { name: "Exalted Orb", quantity: 1 },
      { name: "Vaal Orb", quantity: 5 },
    ]);
  });

  test("handles CRLF line endings", () => {
    expect(extractRows("Mirror of Kalandra x1\r\nChaos Orb x9")).toEqual([
      { name: "Mirror of Kalandra", quantity: 1 },
      { name: "Chaos Orb", quantity: 9 },
    ]);
  });

  test("empty input → empty array", () => {
    expect(extractRows("")).toEqual([]);
  });

  test("preserves top-to-bottom order (screen position)", () => {
    const rows = extractRows("Alpha Orb\nBeta Orb\nGamma Orb");
    expect(rows.map((r) => r.name)).toEqual(["Alpha Orb", "Beta Orb", "Gamma Orb"]);
  });
});
