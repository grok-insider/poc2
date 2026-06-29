import { describe, expect, test } from "bun:test";
import {
  cropIconColumn,
  invert,
  upscaleBicubic,
  preprocessFrame,
  preprocessDataUrl,
  type CanvasAdapter,
  type RgbaFrame,
} from "../ocr/preprocess";

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
    const out = preprocessFrame(f, { iconCrop: 0.3, scale: 3 });
    // width: (10 - floor(10*0.3)=3) = 7, ×3 = 21; height 4×3 = 12.
    expect(out.width).toBe(21);
    expect(out.height).toBe(12);
    // light (220) → inverted to 35, and a flat region stays flat through upscale.
    expect(out.data[0]).toBe(35);
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
    });
    expect(out).toBe("data:image/png;base64,FAKE");
    expect(encoded).not.toBeNull();
    // crop 3 off 10 → 7, ×3 = 21 wide.
    expect(encoded!.width).toBe(21);
    expect(encoded!.data[0]).toBe(55); // 255 - 200
  });
});
