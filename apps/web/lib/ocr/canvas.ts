"use client";

/// Browser {@link CanvasAdapter} for the OCR preprocessing pipeline.
///
/// This is the ONLY DOM/canvas-bound part of the OCR lib — all pixel math is
/// pure (see preprocess.ts) and unit-tested with a fake adapter. We prefer
/// `OffscreenCanvas` (works on the main thread and in a worker) and fall back
/// to a detached `<canvas>` element when it isn't available.

import type { CanvasAdapter, RgbaFrame } from "./preprocess";

function makeCanvas(w: number, h: number): {
  ctx: CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D;
  toDataUrl: () => Promise<string>;
} {
  if (typeof OffscreenCanvas !== "undefined") {
    const canvas = new OffscreenCanvas(Math.max(1, w), Math.max(1, h));
    const ctx = canvas.getContext("2d", {
      willReadFrequently: true,
    }) as OffscreenCanvasRenderingContext2D | null;
    if (!ctx) throw new Error("OffscreenCanvas 2d context unavailable");
    const toDataUrl = async () => {
      const blob = await canvas.convertToBlob({ type: "image/png" });
      return await blobToDataUrl(blob);
    };
    return { ctx, toDataUrl };
  }
  const canvas = document.createElement("canvas");
  canvas.width = Math.max(1, w);
  canvas.height = Math.max(1, h);
  const ctx = canvas.getContext("2d", { willReadFrequently: true });
  if (!ctx) throw new Error("canvas 2d context unavailable");
  return { ctx, toDataUrl: async () => canvas.toDataURL("image/png") };
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
