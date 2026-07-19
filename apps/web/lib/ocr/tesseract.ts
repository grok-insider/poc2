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
import type { PixelBaseline, PixelBbox } from "./preprocess";

/** Origin-relative base for the vendored OCR runtime. */
export const OCR_ASSET_BASE = "/ocr";

export interface RecognizeOptions {
  /** Page-segmentation mode. "6" = a uniform block of text (the price panel). */
  psm?: string;
  /** Restrict recognized glyphs — names + the `Nx` multiplier + separators. */
  charWhitelist?: string;
  /** Stock English model. Reward rows use fast; full tooltip import uses best. */
  model?: "best" | "fast";
}

export interface StructuredOcrLine {
  text: string;
  confidence: number;
  bbox: PixelBbox;
  baseline: PixelBaseline;
}

export interface StructuredRecognition {
  text: string;
  lines: StructuredOcrLine[];
}

export interface OcrSession {
  prewarm(): Promise<void>;
  recognize(
    image: Tesseract.ImageLike,
    opts?: Pick<RecognizeOptions, "psm" | "charWhitelist">,
  ): Promise<StructuredRecognition>;
  recognizeMany(
    images: Tesseract.ImageLike[],
    opts?: Pick<RecognizeOptions, "psm" | "charWhitelist">,
  ): Promise<StructuredRecognition[]>;
  terminate(): Promise<void>;
}

export type OcrWorkerFactory = (
  opts?: RecognizeOptions,
) => Promise<Tesseract.Worker>;

export interface RecognitionPageLike {
  text?: string | null;
  blocks?: Array<{
    paragraphs?: Array<{
      lines?: Array<{
        text: string;
        confidence: number;
        bbox: PixelBbox;
        baseline: PixelBaseline;
      }>;
    }>;
  }> | null;
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
    langPath: `${OCR_ASSET_BASE}/${opts.model ?? "best"}`,
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

/** Flatten Tesseract's block/paragraph tree into source-ordered line records. */
export function structuredRecognitionFromPage(
  page: RecognitionPageLike,
): StructuredRecognition {
  const lines: StructuredOcrLine[] = [];
  for (const block of page.blocks ?? []) {
    for (const paragraph of block.paragraphs ?? []) {
      for (const line of paragraph.lines ?? []) {
        lines.push({
          text: line.text,
          confidence: line.confidence,
          bbox: { ...line.bbox },
          baseline: { ...line.baseline },
        });
      }
    }
  }
  return { text: page.text ?? "", lines };
}

async function recognizeWithWorker(
  worker: Tesseract.Worker,
  image: Tesseract.ImageLike,
): Promise<StructuredRecognition> {
  const { data } = await worker.recognize(
    image,
    {},
    { text: true, blocks: true },
  );
  return structuredRecognitionFromPage(data);
}

/**
 * A single-worker, serialized OCR session. Tesseract worker/model startup is
 * expensive, so watcher scans keep one session warm instead of rebuilding the
 * 17 MB runtime for every frame.
 */
export function createOcrSession(
  opts: RecognizeOptions = {},
  workerFactory: OcrWorkerFactory = createOcrWorker,
): OcrSession {
  let workerPromise: Promise<Tesseract.Worker> | null = null;
  let queue: Promise<void> = Promise.resolve();
  let closed = false;
  let activeParameters = `${opts.psm ?? "6"}\0${opts.charWhitelist ?? DEFAULT_WHITELIST}`;

  const worker = async () => {
    if (closed) throw new Error("OCR session is terminated");
    workerPromise ??= workerFactory(opts);
    try {
      return await workerPromise;
    } catch (error) {
      workerPromise = null;
      throw error;
    }
  };

  const recognizeMany = (
    images: Tesseract.ImageLike[],
    callOpts: Pick<RecognizeOptions, "psm" | "charWhitelist"> = {},
  ) => {
    const task = queue.then(async () => {
      if (closed) throw new Error("OCR session is terminated");
      const activeWorker = await worker();
      const nextPsm = callOpts.psm ?? opts.psm ?? "6";
      const nextWhitelist = callOpts.charWhitelist ?? opts.charWhitelist ?? DEFAULT_WHITELIST;
      const parameterKey = `${nextPsm}\0${nextWhitelist}`;
      if (parameterKey !== activeParameters) {
        await activeWorker.setParameters({
          tessedit_pageseg_mode: nextPsm as Tesseract.PSM,
          tessedit_char_whitelist: nextWhitelist,
        });
        activeParameters = parameterKey;
      }
      const results: StructuredRecognition[] = [];
      try {
        for (const image of images) {
          results.push(await recognizeWithWorker(activeWorker, image));
        }
      } catch (error) {
        workerPromise = null;
        await activeWorker.terminate().catch(() => {});
        throw error;
      }
      return results;
    });
    queue = task.then(() => undefined, () => undefined);
    return task;
  };

  return {
    async prewarm() {
      await worker();
    },
    async recognize(image, callOpts) {
      return (await recognizeMany([image], callOpts))[0] ?? { text: "", lines: [] };
    },
    recognizeMany,
    async terminate() {
      if (closed) return;
      closed = true;
      await queue;
      const activeWorker = await workerPromise?.catch(() => null);
      workerPromise = null;
      await activeWorker?.terminate().catch(() => {});
    },
  };
}

/** Recognize one image with text and line-level geometry. */
export async function recognizeStructuredText(
  image: Tesseract.ImageLike,
  opts: RecognizeOptions = {},
): Promise<StructuredRecognition> {
  const session = createOcrSession(opts);
  try {
    return await session.recognize(image);
  } finally {
    await session.terminate();
  }
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
  return (await recognizeStructuredText(pngDataUrl, opts)).text;
}

/** Recognize multiple preprocessing variants with one worker/model load. */
export async function recognizeStructuredTexts(
  images: Tesseract.ImageLike[],
  opts: RecognizeOptions = {},
): Promise<StructuredRecognition[]> {
  if (images.length === 0) return [];
  const session = createOcrSession(opts);
  try {
    return await session.recognizeMany(images);
  } finally {
    await session.terminate();
  }
}

/** Existing text-only multi-image API. */
export async function recognizeTexts(
  images: Tesseract.ImageLike[],
  opts: RecognizeOptions = {},
): Promise<string[]> {
  return (await recognizeStructuredTexts(images, opts)).map((result) => result.text);
}
