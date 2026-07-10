import { describe, expect, test } from "bun:test";
import {
  buildRecognizedVariant,
  resolutionScore,
  resolveAndPrice,
  resolveAndPriceBatch,
  spatialSlotKey,
  variantNeedsAccurateFallback,
} from "../ocr/scan";
import type { SlotRead } from "../ocr/rowLock";
import type { OcrLineGeometry } from "../ocr/extractRows";
import type { PreprocessTransform } from "../ocr/preprocess";
import type { StructuredRecognition } from "../ocr/tesseract";

function read(over: Partial<SlotRead>): SlotRead {
  return {
    slot: "0",
    key: null,
    name: "noise",
    quantity: 1,
    method: "none",
    score: 0,
    ...over,
  };
}

describe("OCR variant resolution score", () => {
  test("catalogue-resolved rows dominate unresolved header noise", () => {
    const unresolved = [
      read({ name: "Runesmap Combinations" }),
      read({ slot: "1", name: "blurred icons" }),
    ];
    const rewards = [
      read({ key: "Uhtred's Saga", name: "Uhtred’s Saga", method: "exact", score: 1 }),
      read({
        slot: "1",
        key: "Greater Chaos Orb",
        name: "Greater Chaos Orb",
        method: "exact",
        score: 1,
      }),
    ];
    expect(resolutionScore(rewards)).toBeGreaterThan(resolutionScore(unresolved));
  });

  test("accepts a confident fast pass and retries incomplete reads", () => {
    const transform: PreprocessTransform = {
      source: { width: 100, height: 100 },
      crop: { x: 50, y: 0, width: 50, height: 100 },
      processed: { width: 100, height: 200 },
    };
    const variant = buildRecognizedVariant(
      0.5,
      {
        text: "Divine Orb\nChaos Orb",
        lines: [
          {
            text: "Divine Orb",
            confidence: 92,
            bbox: { x0: 0, y0: 20, x1: 100, y1: 35 },
            baseline: { x0: 0, y0: 32, x1: 100, y1: 32 },
          },
          {
            text: "Chaos Orb",
            confidence: 88,
            bbox: { x0: 0, y0: 60, x1: 100, y1: 75 },
            baseline: { x0: 0, y0: 72, x1: 100, y1: 72 },
          },
        ],
      },
      transform,
    );
    const complete = [
      read({ key: "divine", method: "exact", ocrConfidence: 92 }),
      read({ key: "chaos", method: "exact", ocrConfidence: 88 }),
      read({ key: "mirror", method: "exact", ocrConfidence: 91 }),
      read({ key: "exalted", method: "exact", ocrConfidence: 87 }),
    ];
    expect(variantNeedsAccurateFallback(variant, complete)).toBe(false);
    expect(variantNeedsAccurateFallback(variant, complete.map((entry) => ({
      ...entry,
      ocrConfidence: 20,
    })))).toBe(false);
    expect(variantNeedsAccurateFallback(variant, complete.slice(0, 2))).toBe(true);
    expect(variantNeedsAccurateFallback(variant, [
      complete[0],
      read({ key: "chaos", method: "fuzzy", score: 0.95, ocrConfidence: 88 }),
      complete[2],
      complete[3],
    ])).toBe(false);
  });
});

describe("OCR spatial rows", () => {
  const geometry: OcrLineGeometry = {
    bbox: { x0: 0.35, y0: 0.18, x1: 0.9, y1: 0.22 },
    baseline: { x0: 0.35, y0: 0.21, x1: 0.9, y1: 0.21 },
    center: { x: 0.625, y: 0.2 },
  };

  test("nearest-bucket slot keys tolerate small center jitter", () => {
    expect(spatialSlotKey(0.2)).toBe(spatialSlotKey(0.204));
    expect(spatialSlotKey(0.2)).not.toBe(spatialSlotKey(0.23));
  });

  test("a recognized variant maps lines through its matching crop transform", () => {
    const recognition: StructuredRecognition = {
      text: "Divine Orb",
      lines: [
        {
          text: "Divine Orb",
          confidence: 90,
          bbox: { x0: 0, y0: 20, x1: 100, y1: 40 },
          baseline: { x0: 0, y0: 36, x1: 100, y1: 36 },
        },
      ],
    };
    const transform: PreprocessTransform = {
      source: { width: 200, height: 100 },
      crop: { x: 100, y: 0, width: 100, height: 100 },
      processed: { width: 100, height: 100 },
    };

    const selected = buildRecognizedVariant(0.5, recognition, transform);
    expect(selected.iconCrop).toBe(0.5);
    expect(selected.transform).toBe(transform);
    expect(selected.rows[0].geometry?.bbox).toEqual({
      x0: 0.5,
      y0: 0.2,
      x1: 1,
      y1: 0.4,
    });
  });

  test("a recognized variant falls back to legacy text rows without blocks", () => {
    const transform: PreprocessTransform = {
      source: { width: 100, height: 100 },
      crop: { x: 0, y: 0, width: 100, height: 100 },
      processed: { width: 100, height: 100 },
    };
    const variant = buildRecognizedVariant(
      0,
      { text: "Chaos Orb x3", lines: [] },
      transform,
    );
    expect(variant.rows).toEqual([{ name: "Chaos Orb", quantity: 3 }]);
  });

  test("resolveAndPrice retains selected-row geometry and OCR confidence", async () => {
    const result = await resolveAndPrice(
      [{ name: "Divine Orb", quantity: 2, confidence: 96, geometry }],
      async () => ({ key: "divine", score: 1, method: "exact" }),
    );

    expect(result.reads[0]).toMatchObject({
      slot: spatialSlotKey(geometry.center.y),
      key: "divine",
      ocrConfidence: 96,
      geometry,
    });
    expect(result.priced[0]).toMatchObject({
      key: "divine",
      quantity: 2,
      ocrConfidence: 96,
      geometry,
    });
  });

  test("resolveAndPrice keeps index slots for text-only fallback rows", async () => {
    const result = await resolveAndPrice(
      [
        { name: "Chaos Orb", quantity: 1 },
        { name: "Divine Orb", quantity: 1 },
      ],
      async (name) => ({ key: name, score: 1, method: "exact" }),
    );
    expect(result.reads.map((entry) => entry.slot)).toEqual(["0", "1"]);
    expect(result.reads.every((entry) => entry.geometry === undefined)).toBe(true);
  });

  test("resolveAndPriceBatch resolves every row in one worker round-trip", async () => {
    const calls: string[][] = [];
    const result = await resolveAndPriceBatch(
      [
        { name: "Chaos Orb", quantity: 1 },
        { name: "Divine Orb", quantity: 1 },
      ],
      async (raws) => {
        calls.push(raws);
        return raws.map((raw) => ({ key: raw, score: 1, method: "exact" }));
      },
    );

    expect(calls).toHaveLength(1);
    expect(result.reads.map((entry) => entry.key)).toEqual(["Chaos Orb", "Divine Orb"]);
  });

  test("retries trimmed OCR suffixes and keeps the strongest canonical match", async () => {
    const seen: string[] = [];
    const clean = new Map([
      ["Rune of Vital Flame", "rune-vital"],
      ["Artificer’s Orb", "artificer"],
      ["Mind Rune", "mind"],
    ]);
    const { reads } = await resolveAndPrice(
      [
        { name: "Rune of Vital Flame (F558", quantity: 1 },
        { name: "Artificer’s Orb Jay", quantity: 2 },
        { name: "Mind Rune) a", quantity: 1 },
      ],
      async (raw) => {
        seen.push(raw);
        const key = clean.get(raw) ?? null;
        return { key, score: key ? 1 : 0, method: key ? "exact" : "none" };
      },
    );
    expect(reads.map((read) => read.key)).toEqual(["rune-vital", "artificer", "mind"]);
    expect(seen).toContain("Rune of Vital Flame");
    expect(seen).toContain("Artificer’s Orb");
    expect(seen).toContain("Mind Rune");
  });
});
