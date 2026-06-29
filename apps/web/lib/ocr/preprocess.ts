/// Pure pixel math for the price-region OCR scan (ADR-0013).
///
/// The in-game price panel is light-on-dark text in a column to the RIGHT of a
/// stack of currency icons. Tesseract's `eng` model is trained on dark-on-light
/// print, and small glyphs OCR poorly, so the preprocessing pipeline is:
///
///   1. cropIconColumn — drop the left ~30% (the icon gutter) so the icon art
///      doesn't bleed glyph-shaped noise into the recognizer.
///   2. invert — light-on-dark → dark-on-light (Tesseract's training domain).
///   3. upscaleBicubic — 3× bicubic upscale so thin glyph strokes survive.
///
/// Every transform here is a pure `(RgbaFrame) -> RgbaFrame`: no canvas, no DOM,
/// no `Image`. The canvas-bound edges (decode a captured data-URL into pixels,
/// re-encode the processed pixels) live behind {@link CanvasAdapter} so the math
/// is unit-tested with a hand-built frame and a fake adapter.

/** A raw RGBA bitmap. `data.length === width * height * 4`, row-major. */
export interface RgbaFrame {
  width: number;
  height: number;
  /** RGBA, 4 bytes per pixel, row-major, top-left origin. */
  data: Uint8ClampedArray;
}

/** Default fraction of the frame width to crop off the left (icon gutter). */
export const DEFAULT_ICON_CROP = 0.3;

/** Default upscale factor. */
export const DEFAULT_SCALE = 3;

function assertFrame(f: RgbaFrame): void {
  if (f.width <= 0 || f.height <= 0) {
    throw new Error(`RgbaFrame has non-positive dimensions ${f.width}×${f.height}`);
  }
  if (f.data.length !== f.width * f.height * 4) {
    throw new Error(
      `RgbaFrame data length ${f.data.length} != ${f.width}×${f.height}×4`,
    );
  }
}

/**
 * Crop the left `fraction` of the frame off (the icon column), keeping the
 * right text column. Pure: returns a new frame. `fraction` is clamped to
 * `[0, 0.95]` so we never crop the whole frame away.
 */
export function cropIconColumn(frame: RgbaFrame, fraction = DEFAULT_ICON_CROP): RgbaFrame {
  assertFrame(frame);
  const f = Math.max(0, Math.min(0.95, fraction));
  const cutX = Math.min(frame.width - 1, Math.floor(frame.width * f));
  const newW = frame.width - cutX;
  const newH = frame.height;
  const out = new Uint8ClampedArray(newW * newH * 4);
  for (let y = 0; y < newH; y++) {
    const srcRow = (y * frame.width + cutX) * 4;
    const dstRow = y * newW * 4;
    out.set(frame.data.subarray(srcRow, srcRow + newW * 4), dstRow);
  }
  return { width: newW, height: newH, data: out };
}

/**
 * Invert RGB channels (255 - v) in place of color, leaving alpha untouched.
 * Turns the game's light-on-dark text into dark-on-light. Pure: new frame.
 */
export function invert(frame: RgbaFrame): RgbaFrame {
  assertFrame(frame);
  const d = frame.data;
  const out = new Uint8ClampedArray(d.length);
  for (let i = 0; i < d.length; i += 4) {
    out[i] = 255 - d[i];
    out[i + 1] = 255 - d[i + 1];
    out[i + 2] = 255 - d[i + 2];
    out[i + 3] = d[i + 3];
  }
  return { width: frame.width, height: frame.height, data: out };
}

/** Catmull-Rom (bicubic) kernel weight for a 1-D offset `t` and tap distance. */
function cubicWeight(t: number): [number, number, number, number] {
  // Catmull-Rom spline (a = -0.5) — the standard "bicubic" image kernel.
  const t2 = t * t;
  const t3 = t2 * t;
  return [
    -0.5 * t3 + t2 - 0.5 * t,
    1.5 * t3 - 2.5 * t2 + 1,
    -1.5 * t3 + 2 * t2 + 0.5 * t,
    0.5 * t3 - 0.5 * t2,
  ];
}

function clampIdx(i: number, n: number): number {
  return i < 0 ? 0 : i >= n ? n - 1 : i;
}

/**
 * Bicubic (Catmull-Rom) upscale by an integer-or-fractional `scale` (> 1).
 * Pure: returns a new, larger frame. Edge taps clamp to the border.
 */
export function upscaleBicubic(frame: RgbaFrame, scale = DEFAULT_SCALE): RgbaFrame {
  assertFrame(frame);
  if (scale <= 1) return { ...frame, data: new Uint8ClampedArray(frame.data) };
  const { width: sw, height: sh, data: src } = frame;
  const dw = Math.max(1, Math.round(sw * scale));
  const dh = Math.max(1, Math.round(sh * scale));
  const out = new Uint8ClampedArray(dw * dh * 4);

  const sample = (x: number, y: number, c: number): number => {
    const xi = clampIdx(x, sw);
    const yi = clampIdx(y, sh);
    return src[(yi * sw + xi) * 4 + c];
  };

  for (let dy = 0; dy < dh; dy++) {
    // Map dst-center to src space (the +0.5 keeps the sampling grid centered).
    const sy = (dy + 0.5) / scale - 0.5;
    const y0 = Math.floor(sy);
    const wy = cubicWeight(sy - y0);
    for (let dx = 0; dx < dw; dx++) {
      const sx = (dx + 0.5) / scale - 0.5;
      const x0 = Math.floor(sx);
      const wx = cubicWeight(sx - x0);
      const di = (dy * dw + dx) * 4;
      for (let c = 0; c < 4; c++) {
        let acc = 0;
        for (let m = -1; m <= 2; m++) {
          let rowAcc = 0;
          for (let n = -1; n <= 2; n++) {
            rowAcc += wx[n + 1] * sample(x0 + n, y0 + m, c);
          }
          acc += wy[m + 1] * rowAcc;
        }
        out[di + c] = Math.round(acc);
      }
    }
  }
  return { width: dw, height: dh, data: out };
}

export interface PreprocessOptions {
  /** Fraction of width cropped off the left (icon gutter). */
  iconCrop?: number;
  /** Upscale factor. */
  scale?: number;
}

/**
 * The pure preprocessing pipeline: crop the icon column, invert, then upscale.
 * Operates only on pixels — see {@link preprocessDataUrl} for the canvas-bound
 * entry point used by the overlay.
 */
export function preprocessFrame(frame: RgbaFrame, opts: PreprocessOptions = {}): RgbaFrame {
  const cropped = cropIconColumn(frame, opts.iconCrop ?? DEFAULT_ICON_CROP);
  const inverted = invert(cropped);
  return upscaleBicubic(inverted, opts.scale ?? DEFAULT_SCALE);
}

/**
 * Decode/encode boundary so the pure pipeline can run in the browser (real
 * canvas) and in tests (a fake). `toFrame` turns a captured data-URL into
 * pixels; `fromFrame` re-encodes processed pixels into a PNG data-URL that
 * Tesseract can recognize.
 */
export interface CanvasAdapter {
  toFrame(dataUrl: string): Promise<RgbaFrame>;
  fromFrame(frame: RgbaFrame): Promise<string>;
}

/**
 * Preprocess a captured-region data-URL end to end, returning a PNG data-URL
 * ready for `tesseract.recognize`. The canvas work is delegated to `adapter`.
 */
export async function preprocessDataUrl(
  dataUrl: string,
  adapter: CanvasAdapter,
  opts: PreprocessOptions = {},
): Promise<string> {
  const frame = await adapter.toFrame(dataUrl);
  const processed = preprocessFrame(frame, opts);
  return adapter.fromFrame(processed);
}
