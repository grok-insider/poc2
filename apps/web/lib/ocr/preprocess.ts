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
/// Pixel transforms and inverse geometry here are pure: no canvas, no DOM, no
/// `Image`. The canvas-bound edges (decode a captured data-URL into pixels,
/// re-encode the processed pixels) live behind {@link CanvasAdapter} so the math
/// is unit-tested with a hand-built frame and a fake adapter.

/** A raw RGBA bitmap. `data.length === width * height * 4`, row-major. */
export interface RgbaFrame {
  width: number;
  height: number;
  /** RGBA, 4 bytes per pixel, row-major, top-left origin. */
  data: Uint8ClampedArray;
}

export interface PixelDimensions {
  width: number;
  height: number;
}

export interface PixelRegion extends PixelDimensions {
  x: number;
  y: number;
}

/** Tesseract pixel coordinates in the processed image. */
export interface PixelBbox {
  x0: number;
  y0: number;
  x1: number;
  y1: number;
}

/** Tesseract baseline pixel coordinates in the processed image. */
export interface PixelBaseline {
  x0: number;
  y0: number;
  x1: number;
  y1: number;
}

export interface NormalizedPoint {
  x: number;
  y: number;
}

export interface NormalizedBbox {
  x0: number;
  y0: number;
  x1: number;
  y1: number;
}

export interface NormalizedBaseline {
  x0: number;
  y0: number;
  x1: number;
  y1: number;
}

/** Geometry needed to invert processed OCR coordinates into the source frame. */
export interface PreprocessTransform {
  source: PixelDimensions;
  crop: PixelRegion;
  processed: PixelDimensions;
}

export interface PreprocessResult {
  frame: RgbaFrame;
  transform: PreprocessTransform;
}

export interface PreprocessedDataUrl {
  dataUrl: string;
  transform: PreprocessTransform;
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

/** Mean Rec. 709 luminance, used to infer the panel background polarity. */
export function meanLuminance(frame: RgbaFrame): number {
  assertFrame(frame);
  let total = 0;
  for (let i = 0; i < frame.data.length; i += 4) {
    total +=
      frame.data[i] * 0.2126 +
      frame.data[i + 1] * 0.7152 +
      frame.data[i + 2] * 0.0722;
  }
  return total / (frame.width * frame.height);
}

function copyFrame(frame: RgbaFrame): RgbaFrame {
  return { ...frame, data: new Uint8ClampedArray(frame.data) };
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
  /** Text/background polarity. Auto supports parchment and dark HUD panels. */
  polarity?: "auto" | "dark-on-light" | "light-on-dark";
  /** Trim bright parchment captures to their active vertical panel band. */
  trimVertical?: boolean;
}

function clampNormalized(value: number): number {
  return Math.max(0, Math.min(1, value));
}

function mapProcessedPoint(
  x: number,
  y: number,
  transform: PreprocessTransform,
): NormalizedPoint {
  const sourceX =
    transform.crop.x + (x / transform.processed.width) * transform.crop.width;
  const sourceY =
    transform.crop.y + (y / transform.processed.height) * transform.crop.height;
  return {
    x: clampNormalized(sourceX / transform.source.width),
    y: clampNormalized(sourceY / transform.source.height),
  };
}

/** Map a processed Tesseract bbox back to normalized full-source coordinates. */
export function mapProcessedBboxToNormalizedSource(
  bbox: PixelBbox,
  transform: PreprocessTransform,
): NormalizedBbox {
  const start = mapProcessedPoint(bbox.x0, bbox.y0, transform);
  const end = mapProcessedPoint(bbox.x1, bbox.y1, transform);
  return { x0: start.x, y0: start.y, x1: end.x, y1: end.y };
}

/** Map a processed Tesseract baseline to normalized full-source coordinates. */
export function mapProcessedBaselineToNormalizedSource(
  baseline: PixelBaseline,
  transform: PreprocessTransform,
): NormalizedBaseline {
  const start = mapProcessedPoint(baseline.x0, baseline.y0, transform);
  const end = mapProcessedPoint(baseline.x1, baseline.y1, transform);
  return { x0: start.x, y0: start.y, x1: end.x, y1: end.y };
}

/**
 * The pure preprocessing pipeline: crop the icon column, invert, then upscale.
 * Operates only on pixels — see {@link preprocessDataUrl} for the canvas-bound
 * entry point used by the overlay.
 */
export function preprocessFrameWithTransform(
  frame: RgbaFrame,
  opts: PreprocessOptions = {},
): PreprocessResult {
  const cropped = cropIconColumn(frame, opts.iconCrop ?? DEFAULT_ICON_CROP);
  const polarity = opts.polarity ?? "auto";
  const normalized =
    polarity === "light-on-dark" ||
    (polarity === "auto" && meanLuminance(cropped) < 128)
      ? invert(cropped)
      : copyFrame(cropped);
  const processed = upscaleBicubic(normalized, opts.scale ?? DEFAULT_SCALE);
  return {
    frame: processed,
    transform: {
      source: { width: frame.width, height: frame.height },
      crop: {
        x: frame.width - cropped.width,
        y: 0,
        width: cropped.width,
        height: cropped.height,
      },
      processed: { width: processed.width, height: processed.height },
    },
  };
}

/** Existing frame-only preprocessing API. */
export function preprocessFrame(frame: RgbaFrame, opts: PreprocessOptions = {}): RgbaFrame {
  return preprocessFrameWithTransform(frame, opts).frame;
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
  return (await preprocessDataUrlWithTransform(dataUrl, adapter, opts)).dataUrl;
}

/** Data-URL preprocessing with the inverse geometry transform retained. */
export async function preprocessDataUrlWithTransform(
  dataUrl: string,
  adapter: CanvasAdapter,
  opts: PreprocessOptions = {},
): Promise<PreprocessedDataUrl> {
  const frame = await adapter.toFrame(dataUrl);
  const processed = preprocessFrameWithTransform(frame, opts);
  return {
    dataUrl: await adapter.fromFrame(processed.frame),
    transform: processed.transform,
  };
}
