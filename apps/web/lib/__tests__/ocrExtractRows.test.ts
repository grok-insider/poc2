import { describe, expect, test } from "bun:test";
import {
  extractRows,
  extractStructuredRows,
  parseRow,
} from "../ocr/extractRows";
import type { PreprocessTransform } from "../ocr/preprocess";
import type { StructuredRecognition } from "../ocr/tesseract";

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

  test("parses the live Runesmap reward rows including a curly apostrophe", () => {
    expect(extractRows("1x Uhtred’s Saga\n3x Greater Chaos Orb")).toEqual([
      { name: "Uhtred’s Saga", quantity: 1 },
      { name: "Greater Chaos Orb", quantity: 3 },
    ]);
  });
});

describe("extractStructuredRows", () => {
  test("applies parseRow and retains confidence plus normalized source geometry", () => {
    const transform: PreprocessTransform = {
      source: { width: 100, height: 50 },
      crop: { x: 30, y: 0, width: 70, height: 50 },
      processed: { width: 210, height: 150 },
    };
    const recognition: StructuredRecognition = {
      text: "noise\nDivine Orb x2",
      lines: [
        {
          text: "---",
          confidence: 10,
          bbox: { x0: 0, y0: 0, x1: 30, y1: 15 },
          baseline: { x0: 0, y0: 12, x1: 30, y1: 12 },
        },
        {
          text: "Divine Orb x2",
          confidence: 93.5,
          bbox: { x0: 21, y0: 30, x1: 189, y1: 60 },
          baseline: { x0: 21, y0: 54, x1: 189, y1: 54 },
        },
      ],
    };

    const rows = extractStructuredRows(recognition, transform);
    expect(rows).toHaveLength(1);
    expect(rows[0]).toMatchObject({
      name: "Divine Orb",
      quantity: 2,
      confidence: 93.5,
    });
    expect(rows[0].geometry?.bbox).toEqual({
      x0: 0.37,
      y0: 0.2,
      x1: 0.93,
      y1: 0.4,
    });
    expect(rows[0].geometry?.center.x).toBeCloseTo(0.65);
    expect(rows[0].geometry?.center.y).toBeCloseTo(0.3);
    expect(rows[0].geometry?.baseline.y0).toBeCloseTo(0.36);
  });
});
