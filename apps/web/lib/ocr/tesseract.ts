"use client";

/// Origin-relative Tesseract.js setup for the price-region OCR scan (ADR-0013).
///
/// All asset paths are ROOT-ABSOLUTE (`/ocr/...`) so they resolve identically
/// whether the app is served from a web origin or the desktop shell's `app://`
/// scheme, and survive `output: 'export'`. The vendored runtime lives under
/// apps/web/public/ocr/ — `bun run ocr:assets` populates it (see
/// scripts/fetch-ocr-assets.mjs). Nothing here is fetched from a CDN.
///
///   - workerPath → /ocr/worker.min.js
///   - corePath   → /ocr   (a directory; tesseract.js appends the WASM variant
///                          it picks from feature detection, e.g.
///                          /ocr/tesseract-core-simd.wasm.js — every variant is
///                          vendored so none 404s)
///   - langPath   → /ocr   (serves /ocr/eng.traineddata.gz; gzip:true)
///   - cacheMethod: "none" — don't write the model into IndexedDB; serving it
///                  origin-relative each run is simpler and avoids privileged-
///                  scheme cache quirks.

import type Tesseract from "tesseract.js";

/** Origin-relative base for the vendored OCR runtime. */
export const OCR_ASSET_BASE = "/ocr";

export interface RecognizeOptions {
  /** Page-segmentation mode. "6" = a uniform block of text (the price panel). */
  psm?: string;
  /** Restrict recognized glyphs — names + the `Nx` multiplier + separators. */
  charWhitelist?: string;
}

const DEFAULT_WHITELIST =
  "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789 x×'’-.,()/";

/**
 * Create a Tesseract worker wired to the vendored, origin-relative runtime.
 * The caller owns the worker lifetime and must `terminate()` it.
 */
export async function createOcrWorker(
  opts: RecognizeOptions = {},
): Promise<Tesseract.Worker> {
  const Tess = (await import("tesseract.js")).default;
  const worker = await Tess.createWorker("eng", Tess.OEM.LSTM_ONLY, {
    workerPath: `${OCR_ASSET_BASE}/worker.min.js`,
    // Pin the explicit NON-SIMD LSTM core file rather than the directory form.
    // Letting tesseract.js feature-detect can pick a SIMD core whose intrinsics
    // (e.g. DotProductSSE) abort under some Chromium/Electron wasm runtimes —
    // observed on the Electron 41 desktop shell. LSTM_ONLY needs the -lstm core.
    corePath: `${OCR_ASSET_BASE}/tesseract-core-lstm.wasm.js`,
    langPath: OCR_ASSET_BASE,
    // The model is served origin-relative every run; no IndexedDB caching.
    cacheMethod: "none",
    gzip: true,
  });
  await worker.setParameters({
    tessedit_pageseg_mode: (opts.psm ?? "6") as Tesseract.PSM,
    tessedit_char_whitelist: opts.charWhitelist ?? DEFAULT_WHITELIST,
  });
  return worker;
}

/**
 * Recognize a preprocessed PNG data-URL, returning the raw text blob. Spawns a
 * one-shot worker and tears it down (a scan is a single, hotkey-triggered pass,
 * not a hot loop — no worker pooling needed).
 */
export async function recognizeText(
  pngDataUrl: string,
  opts: RecognizeOptions = {},
): Promise<string> {
  const worker = await createOcrWorker(opts);
  try {
    const { data } = await worker.recognize(pngDataUrl);
    return data.text ?? "";
  } finally {
    await worker.terminate();
  }
}
