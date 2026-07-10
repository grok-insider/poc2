"use client";

/// Browser {@link CanvasAdapter} for the OCR preprocessing pipeline.
///
/// This is the ONLY DOM/canvas-bound part of the OCR lib — all pixel math is
/// pure (see preprocess.ts) and unit-tested with a fake adapter. We prefer
/// `OffscreenCanvas` (works on the main thread and in a worker) and fall back
/// to a detached `<canvas>` element when it isn't available.

import {
  DEFAULT_ICON_CROP,
  DEFAULT_SCALE,
  type CanvasAdapter,
  type PreprocessOptions,
  type PreprocessTransform,
  type RgbaFrame,
} from "./preprocess";

interface CanvasSurface {
  canvas: HTMLCanvasElement | OffscreenCanvas;
  ctx: CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D;
  toBlob: () => Promise<Blob>;
  toDataUrl: () => Promise<string>;
}

function makeCanvas(w: number, h: number): {
  canvas: HTMLCanvasElement | OffscreenCanvas;
  ctx: CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D;
  toBlob: () => Promise<Blob>;
  toDataUrl: () => Promise<string>;
} {
  if (typeof OffscreenCanvas !== "undefined") {
    const canvas = new OffscreenCanvas(Math.max(1, w), Math.max(1, h));
    const ctx = canvas.getContext("2d", {
      willReadFrequently: true,
    }) as OffscreenCanvasRenderingContext2D | null;
    if (!ctx) throw new Error("OffscreenCanvas 2d context unavailable");
    const toBlob = () => canvas.convertToBlob({ type: "image/png" });
    return { canvas, ctx, toBlob, toDataUrl: async () => blobToDataUrl(await toBlob()) };
  }
  const canvas = document.createElement("canvas");
  canvas.width = Math.max(1, w);
  canvas.height = Math.max(1, h);
  const ctx = canvas.getContext("2d", { willReadFrequently: true });
  if (!ctx) throw new Error("canvas 2d context unavailable");
  const toBlob = () => new Promise<Blob>((resolve, reject) => {
    canvas.toBlob((blob) => blob ? resolve(blob) : reject(new Error("Canvas PNG encoding failed")), "image/png");
  });
  return { canvas, ctx, toBlob, toDataUrl: async () => canvas.toDataURL("image/png") };
}

function blobToDataUrl(blob: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const fr = new FileReader();
    fr.onload = () => resolve(String(fr.result));
    fr.onerror = () => reject(fr.error ?? new Error("FileReader failed"));
    fr.readAsDataURL(blob);
  });
}

async function decodeToBitmap(dataUrl: string): Promise<ImageBitmap> {
  // `fetch(dataUrl)` → blob → createImageBitmap works on the main thread and in
  // a worker without an `<img>` element.
  const res = await fetch(dataUrl);
  const blob = await res.blob();
  return createImageBitmap(blob);
}

export const browserCanvasAdapter: CanvasAdapter = {
  async toFrame(dataUrl: string): Promise<RgbaFrame> {
    const bmp = await decodeToBitmap(dataUrl);
    try {
      const { ctx } = makeCanvas(bmp.width, bmp.height);
      ctx.drawImage(bmp, 0, 0);
      const img = ctx.getImageData(0, 0, bmp.width, bmp.height);
      return { width: img.width, height: img.height, data: img.data };
    } finally {
      bmp.close?.();
    }
  },

  async fromFrame(frame: RgbaFrame): Promise<string> {
    const { ctx, toDataUrl } = makeCanvas(frame.width, frame.height);
    // Copy into a fresh array over a plain ArrayBuffer (not SharedArrayBuffer)
    // so the ImageData constructor's `ImageDataArray` overload is satisfied.
    const buf = new Uint8ClampedArray(frame.data.length);
    buf.set(frame.data);
    const img = new ImageData(buf, frame.width, frame.height);
    ctx.putImageData(img, 0, 0);
    return toDataUrl();
  },
};

export interface NativePreprocessedImage {
  image: Blob;
  transform: PreprocessTransform;
}

function croppedMeanLuminance(
  frame: RgbaFrame,
  cropX: number,
  cropY = 0,
  cropHeight = frame.height,
): number {
  let total = 0;
  let count = 0;
  // A sparse sample is sufficient for polarity and avoids another full-frame pass.
  for (let y = cropY; y < cropY + cropHeight; y += 4) {
    for (let x = cropX; x < frame.width; x += 4) {
      const i = (y * frame.width + x) * 4;
      total += frame.data[i] * 0.2126 + frame.data[i + 1] * 0.7152 + frame.data[i + 2] * 0.0722;
      count += 1;
    }
  }
  return count > 0 ? total / count : 0;
}

/** Locate the parchment panel and exclude dark chat/background below it. */
export function detectBrightVerticalRegion(
  frame: RgbaFrame,
  cropX: number,
): { y: number; height: number } | null {
  const startX = Math.max(cropX, Math.floor(frame.width * 0.45));
  const endX = Math.max(startX + 1, Math.floor(frame.width * 0.92));
  let first = -1;
  let last = -1;
  for (let y = 0; y < frame.height; y += 2) {
    let bright = 0;
    let count = 0;
    for (let x = startX; x < endX; x += 8) {
      const i = (y * frame.width + x) * 4;
      const value =
        frame.data[i] * 0.2126 +
        frame.data[i + 1] * 0.7152 +
        frame.data[i + 2] * 0.0722;
      if (value > 110) bright += 1;
      count += 1;
    }
    if (count > 0 && bright / count >= 0.45) {
      if (first < 0) first = y;
      last = y;
    }
  }
  if (first < 0 || last - first < 80) return null;
  const y = Math.max(0, first - 24);
  const end = Math.min(frame.height, last + 26);
  return { y, height: end - y };
}

function putFrame(surface: CanvasSurface, frame: RgbaFrame): void {
  const pixels = new Uint8ClampedArray(frame.data.length);
  pixels.set(frame.data);
  surface.ctx.putImageData(new ImageData(pixels, frame.width, frame.height), 0, 0);
}

/**
 * Crop and scale through Chromium's native canvas implementation. This replaces
 * the old per-pixel TypeScript bicubic hot path while retaining exact inverse
 * geometry for row-aligned compositor rendering.
 */
export async function preprocessFrameNative(
  frame: RgbaFrame,
  opts: PreprocessOptions = {},
): Promise<NativePreprocessedImage> {
  const fraction = Math.max(0, Math.min(0.95, opts.iconCrop ?? DEFAULT_ICON_CROP));
  const cropX = Math.min(frame.width - 1, Math.floor(frame.width * fraction));
  const cropWidth = frame.width - cropX;
  const vertical = opts.trimVertical
    ? detectBrightVerticalRegion(frame, cropX)
    : null;
  const cropY = vertical?.y ?? 0;
  const cropHeight = vertical?.height ?? frame.height;
  const scale = Math.max(1, opts.scale ?? DEFAULT_SCALE);
  const width = Math.max(1, Math.round(cropWidth * scale));
  const height = Math.max(1, Math.round(cropHeight * scale));
  const source = makeCanvas(frame.width, frame.height);
  const output = makeCanvas(width, height);
  putFrame(source, frame);
  output.ctx.imageSmoothingEnabled = true;
  output.ctx.imageSmoothingQuality = "high";
  output.ctx.drawImage(
    source.canvas,
    cropX,
    cropY,
    cropWidth,
    cropHeight,
    0,
    0,
    width,
    height,
  );

  const polarity = opts.polarity ?? "auto";
  const shouldInvert = polarity === "light-on-dark" ||
    (polarity === "auto" && croppedMeanLuminance(frame, cropX, cropY, cropHeight) < 128);
  if (shouldInvert) {
    const pixels = output.ctx.getImageData(0, 0, width, height);
    for (let i = 0; i < pixels.data.length; i += 4) {
      pixels.data[i] = 255 - pixels.data[i];
      pixels.data[i + 1] = 255 - pixels.data[i + 1];
      pixels.data[i + 2] = 255 - pixels.data[i + 2];
    }
    output.ctx.putImageData(pixels, 0, 0);
  }

  return {
    image: await output.toBlob(),
    transform: {
      source: { width: frame.width, height: frame.height },
      crop: { x: cropX, y: cropY, width: cropWidth, height: cropHeight },
      processed: { width, height },
    },
  };
}
