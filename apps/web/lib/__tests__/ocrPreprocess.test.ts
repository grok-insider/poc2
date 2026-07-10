import { describe, expect, test } from "bun:test";
import {
  cropIconColumn,
  invert,
  meanLuminance,
  upscaleBicubic,
  preprocessFrame,
  preprocessFrameWithTransform,
  preprocessDataUrl,
  mapProcessedBaselineToNormalizedSource,
  mapProcessedBboxToNormalizedSource,
  type CanvasAdapter,
  type RgbaFrame,
} from "../ocr/preprocess";
import { detectBrightVerticalRegion } from "../ocr/canvas";

/** Build a solid-color frame for assertions. */
function solid(w: number, h: number, r: number, g: number, b: number, a = 255): RgbaFrame {
  const data = new Uint8ClampedArray(w * h * 4);
  for (let i = 0; i < data.length; i += 4) {
    data[i] = r;
    data[i + 1] = g;
    data[i + 2] = b;
    data[i + 3] = a;
  }
  return { width: w, height: h, data };
}

/** Two vertical halves: left color L, right color R. */
function twoTone(w: number, h: number, L: number, R: number): RgbaFrame {
  const data = new Uint8ClampedArray(w * h * 4);
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      const v = x < w / 2 ? L : R;
      const i = (y * w + x) * 4;
      data[i] = v;
      data[i + 1] = v;
      data[i + 2] = v;
      data[i + 3] = 255;
    }
  }
  return { width: w, height: h, data };
}

describe("cropIconColumn", () => {
  test("drops ~30% of the width off the left by default", () => {
    const f = solid(10, 4, 1, 2, 3);
    const out = cropIconColumn(f);
    expect(out.width).toBe(7); // floor(10 * 0.3) = 3 cut
    expect(out.height).toBe(4);
    expect(out.data.length).toBe(7 * 4 * 4);
  });

  test("keeps the RIGHT (text) column, discards the LEFT (icon) column", () => {
    // 8 wide: left half = 10 (icon), right half = 200 (text). Crop 50%.
    const f = twoTone(8, 1, 10, 200);
    const out = cropIconColumn(f, 0.5);
    expect(out.width).toBe(4);
    // every surviving pixel should be the right-column value.
    for (let x = 0; x < out.width; x++) {
      expect(out.data[x * 4]).toBe(200);
    }
  });

  test("clamps the fraction so it never crops the whole frame", () => {
    const f = solid(10, 2, 5, 5, 5);
    const out = cropIconColumn(f, 5); // absurd fraction
    expect(out.width).toBeGreaterThanOrEqual(1);
  });

  test("custom fraction of 0 is a no-op width", () => {
    const f = solid(10, 2, 5, 5, 5);
    expect(cropIconColumn(f, 0).width).toBe(10);
  });
});

describe("invert", () => {
  test("maps light-on-dark to dark-on-light (255 - v), preserving alpha", () => {
    const f = solid(2, 2, 230, 200, 10, 128);
    const out = invert(f);
    expect(out.data[0]).toBe(25); // 255 - 230
    expect(out.data[1]).toBe(55); // 255 - 200
    expect(out.data[2]).toBe(245); // 255 - 10
    expect(out.data[3]).toBe(128); // alpha untouched
  });

  test("double invert is identity", () => {
    const f = solid(3, 3, 17, 99, 240);
    const back = invert(invert(f));
    expect(Array.from(back.data)).toEqual(Array.from(f.data));
  });
});

describe("meanLuminance", () => {
  test("distinguishes parchment from a dark HUD panel", () => {
    expect(meanLuminance(solid(2, 2, 220, 205, 170))).toBeGreaterThan(128);
    expect(meanLuminance(solid(2, 2, 20, 18, 15))).toBeLessThan(128);
  });
});

describe("upscaleBicubic", () => {
  test("scales dimensions by the factor", () => {
    const f = solid(4, 5, 100, 100, 100);
    const out = upscaleBicubic(f, 3);
    expect(out.width).toBe(12);
    expect(out.height).toBe(15);
    expect(out.data.length).toBe(12 * 15 * 4);
  });

  test("a uniform image stays uniform after upscale (no ringing on flats)", () => {
    const f = solid(4, 4, 120, 120, 120);
    const out = upscaleBicubic(f, 3);
    for (let i = 0; i < out.data.length; i += 4) {
      expect(out.data[i]).toBe(120);
    }
  });

  test("scale <= 1 returns a copy of the same size (not the same buffer)", () => {
    const f = solid(2, 2, 9, 9, 9);
    const out = upscaleBicubic(f, 1);
    expect(out.width).toBe(2);
    expect(out.data).not.toBe(f.data);
    expect(Array.from(out.data)).toEqual(Array.from(f.data));
  });

  test("output stays within byte range on a sharp edge (clamped, no overflow)", () => {
    const f = twoTone(6, 2, 0, 255);
    const out = upscaleBicubic(f, 3);
    for (let i = 0; i < out.data.length; i++) {
      expect(out.data[i]).toBeGreaterThanOrEqual(0);
      expect(out.data[i]).toBeLessThanOrEqual(255);
    }
  });
});

describe("preprocessFrame", () => {
  test("composes crop → invert → upscale with the expected output size", () => {
    const f = solid(10, 4, 220, 220, 220);
    const out = preprocessFrame(f, {
      iconCrop: 0.3,
      scale: 3,
      polarity: "light-on-dark",
    });
    // width: (10 - floor(10*0.3)=3) = 7, ×3 = 21; height 4×3 = 12.
    expect(out.width).toBe(21);
    expect(out.height).toBe(12);
    // light (220) → inverted to 35, and a flat region stays flat through upscale.
    expect(out.data[0]).toBe(35);
  });

  test("auto keeps dark text on a light parchment background uninverted", () => {
    const f = solid(4, 2, 220, 205, 170);
    const out = preprocessFrame(f, { iconCrop: 0, scale: 1, polarity: "auto" });
    expect(out.data[0]).toBe(220);
    expect(out.data[1]).toBe(205);
  });

  test("auto inverts a dark HUD background", () => {
    const f = solid(4, 2, 20, 20, 20);
    const out = preprocessFrame(f, { iconCrop: 0, scale: 1, polarity: "auto" });
    expect(out.data[0]).toBe(235);
  });

  test("records crop/source/processed dimensions and inverts OCR geometry", () => {
    const result = preprocessFrameWithTransform(solid(10, 4, 20, 20, 20), {
      iconCrop: 0.3,
      scale: 3,
    });
    expect(result.transform).toEqual({
      source: { width: 10, height: 4 },
      crop: { x: 3, y: 0, width: 7, height: 4 },
      processed: { width: 21, height: 12 },
    });

    const bbox = mapProcessedBboxToNormalizedSource(
      { x0: 0, y0: 3, x1: 21, y1: 9 },
      result.transform,
    );
    expect(bbox.x0).toBeCloseTo(0.3);
    expect(bbox.y0).toBeCloseTo(0.25);
    expect(bbox.x1).toBeCloseTo(1);
    expect(bbox.y1).toBeCloseTo(0.75);

    const baseline = mapProcessedBaselineToNormalizedSource(
      { x0: 0, y0: 6, x1: 21, y1: 6 },
      result.transform,
    );
    expect(baseline).toEqual({ x0: 0.3, y0: 0.5, x1: 1, y1: 0.5 });
  });

  test("uses actual rounded output dimensions for fractional-scale inversion", () => {
    const result = preprocessFrameWithTransform(solid(9, 5, 20, 20, 20), {
      iconCrop: 0,
      scale: 1.5,
    });
    expect(result.transform.processed).toEqual({ width: 14, height: 8 });
    expect(
      mapProcessedBboxToNormalizedSource(
        { x0: 0, y0: 0, x1: 14, y1: 8 },
        result.transform,
      ),
    ).toEqual({ x0: 0, y0: 0, x1: 1, y1: 1 });
  });
});

describe("native vertical trim", () => {
  test("finds a bright parchment band and ignores dark surrounding rows", () => {
    const frame = solid(100, 200, 20, 20, 20);
    for (let y = 50; y < 140; y++) {
      for (let x = 45; x < 92; x++) {
        const i = (y * frame.width + x) * 4;
        frame.data[i] = 190;
        frame.data[i + 1] = 180;
        frame.data[i + 2] = 150;
      }
    }
    expect(detectBrightVerticalRegion(frame, 40)).toEqual({ y: 26, height: 138 });
  });

  test("falls back when no substantial bright panel exists", () => {
    expect(detectBrightVerticalRegion(solid(100, 200, 20, 20, 20), 40)).toBeNull();
  });
});

describe("preprocessDataUrl (with a fake CanvasAdapter)", () => {
  test("runs the pure pipeline between decode/encode without any DOM", async () => {
    const source = solid(10, 2, 200, 200, 200);
    let encoded: RgbaFrame | null = null;
    const fake: CanvasAdapter = {
      async toFrame() {
        return source;
      },
      async fromFrame(frame) {
        encoded = frame;
        return "data:image/png;base64,FAKE";
      },
    };
    const out = await preprocessDataUrl("data:image/png;base64,IN", fake, {
      iconCrop: 0.3,
      scale: 3,
      polarity: "light-on-dark",
    });
    expect(out).toBe("data:image/png;base64,FAKE");
    expect(encoded).not.toBeNull();
    // crop 3 off 10 → 7, ×3 = 21 wide.
    expect(encoded!.width).toBe(21);
    expect(encoded!.data[0]).toBe(55); // 255 - 200
  });
});
