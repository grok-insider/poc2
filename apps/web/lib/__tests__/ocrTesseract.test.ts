import { describe, expect, test } from "bun:test";
import type Tesseract from "tesseract.js";
import { createOcrSession, structuredRecognitionFromPage } from "../ocr/tesseract";

function line(text: string, y: number, confidence: number) {
  return {
    text,
    confidence,
    bbox: { x0: 10, y0: y, x1: 90, y1: y + 10 },
    baseline: { x0: 10, y0: y + 8, x1: 90, y1: y + 8 },
  };
}

describe("structured Tesseract recognition", () => {
  test("flattens blocks, paragraphs, and lines in source order", () => {
    const result = structuredRecognitionFromPage({
      text: "Alpha Orb\nBeta Orb\nGamma Orb\n",
      blocks: [
        {
          paragraphs: [
            { lines: [line("Alpha Orb", 5, 91), line("Beta Orb", 20, 87)] },
          ],
        },
        {
          paragraphs: [{ lines: [line("Gamma Orb", 40, 95)] }],
        },
      ],
    });

    expect(result.text).toBe("Alpha Orb\nBeta Orb\nGamma Orb\n");
    expect(result.lines.map((entry) => entry.text)).toEqual([
      "Alpha Orb",
      "Beta Orb",
      "Gamma Orb",
    ]);
    expect(result.lines[1]).toEqual(line("Beta Orb", 20, 87));
  });

  test("keeps text compatibility when block output is absent", () => {
    expect(structuredRecognitionFromPage({ text: "Chaos Orb", blocks: null })).toEqual({
      text: "Chaos Orb",
      lines: [],
    });
  });
});

describe("reusable Tesseract session", () => {
  test("loads one worker and serializes repeated recognition", async () => {
    let factoryCalls = 0;
    let active = 0;
    let maxActive = 0;
    let terminateCalls = 0;
    const worker = {
      async recognize(image: Tesseract.ImageLike) {
        active += 1;
        maxActive = Math.max(maxActive, active);
        await Promise.resolve();
        active -= 1;
        return {
          data: {
            text: String(image),
            blocks: null,
          },
        };
      },
      async terminate() {
        terminateCalls += 1;
      },
    } as unknown as Tesseract.Worker;
    const session = createOcrSession({}, async () => {
      factoryCalls += 1;
      return worker;
    });

    await session.prewarm();
    const results = await Promise.all([
      session.recognize("first"),
      session.recognize("second"),
    ]);
    await session.terminate();
    await session.terminate();

    expect(results.map((result) => result.text)).toEqual(["first", "second"]);
    expect(factoryCalls).toBe(1);
    expect(maxActive).toBe(1);
    expect(terminateCalls).toBe(1);
  });

  test("retries worker creation after initialization failure", async () => {
    let attempts = 0;
    const worker = {
      async recognize() {
        return { data: { text: "recovered", blocks: null } };
      },
      async terminate() {},
    } as unknown as Tesseract.Worker;
    const session = createOcrSession({}, async () => {
      attempts += 1;
      if (attempts === 1) throw new Error("model failed");
      return worker;
    });

    await expect(session.prewarm()).rejects.toThrow("model failed");
    expect((await session.recognize("frame")).text).toBe("recovered");
    expect(attempts).toBe(2);
    await session.terminate();
  });
});
