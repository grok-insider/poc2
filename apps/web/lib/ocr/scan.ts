"use client";

/// One OCR price-scan pass (ADR-0013): a captured-region data-URL in, resolved
/// + de-flickered priced rows out. Ties the pure pieces (preprocess →
/// recognize → extractRows → resolveName → price) together for the overlay.
///
/// The capture itself (bridge.captureRegion) and the lock state live in the
/// overlay component; this module is the stateless compute spine so it can be
/// exercised without the desktop bridge.

import { browserCanvasAdapter, preprocessFrameNative } from "./canvas";
import {
  preprocessDataUrlWithTransform,
  preprocessFrameWithTransform,
  type PreprocessOptions,
  type PreprocessTransform,
  type RgbaFrame,
} from "./preprocess";
import { extractRows, extractStructuredRows, type OcrRow } from "./extractRows";
import {
  recognizeStructuredText,
  recognizeStructuredTexts,
  type OcrSession,
  type RecognizeOptions,
  type StructuredRecognition,
} from "./tesseract";
import type { ResolveMethod, SlotRead } from "./rowLock";
import { priceRow, type PricedRow } from "./priceSource";
import { expandUncutGemQuery } from "@/lib/prices/uncutGems";
import {
  applyResolvedUncutLevel,
  normalizeLocaleLevelSuffix,
  splitLevelParen,
} from "./localePrep";

/** Engine `resolveName` shape (mirrors lib/types ResolveView), injected so the
 * scan stays testable without the worker. */
export interface ResolveResult {
  key: string | null;
  score: number;
  method: string;
}
export type ResolveNameFn = (raw: string) => Promise<ResolveResult>;
export type ResolveNamesFn = (raws: string[]) => Promise<ResolveResult[]>;

const KNOWN_METHODS: ReadonlySet<string> = new Set([
  "exact",
  "prefix",
  "fuzzy",
  "skeleton",
  "currency",
  "none",
]);

/** 2% source-height buckets tolerate ordinary OCR bbox jitter between frames. */
export const SPATIAL_SLOT_QUANTUM = 0.02;

/** Stable, nearest-bucket screen slot for a normalized source Y coordinate. */
export function spatialSlotKey(normalizedY: number): string {
  const y = Math.max(0, Math.min(1, normalizedY));
  const bucket = Math.round(y / SPATIAL_SLOT_QUANTUM);
  return `y:${String(bucket).padStart(3, "0")}`;
}

function coerceMethod(m: string): ResolveMethod {
  return (KNOWN_METHODS.has(m) ? m : "fuzzy") as ResolveMethod;
}

const METHOD_RANK: Record<ResolveMethod, number> = {
  exact: 6,
  currency: 5,
  prefix: 4,
  skeleton: 3,
  fuzzy: 2,
  none: 0,
};

function resolutionCandidates(name: string): string[] {
  const candidates: string[] = [];
  const push = (q: string) => {
    if (q && !candidates.includes(q)) candidates.push(q);
  };
  const leveled = normalizeLocaleLevelSuffix(name);
  const { base, level } = splitLevelParen(leveled);
  // Stripped base first — Spanish/localized clients translate bare names.
  push(base);
  push(leveled);
  push(name);
  // English uncut expand (no-op for pure Spanish bases until translated).
  const forExpand =
    level !== null ? `${base} (Level ${level})` : base || leveled || name;
  for (const q of expandUncutGemQuery(forExpand)) push(q);
  for (const q of expandUncutGemQuery(name)) push(q);
  let current = base || name;
  for (let removed = 0; removed < 2; removed++) {
    const parts = current.trim().split(/\s+/);
    if (parts.length <= 1) break;
    parts.pop();
    current = parts.join(" ").replace(/[\p{P}\p{S}\d]+$/gu, "").trim();
    if (current.length >= 4) push(current);
  }
  return candidates;
}

function betterResolution(candidate: ResolveResult, current: ResolveResult): boolean {
  if (candidate.key !== null && current.key === null) return true;
  if (candidate.key === null) return false;
  const candidateRank = METHOD_RANK[coerceMethod(candidate.method)];
  const currentRank = METHOD_RANK[coerceMethod(current.method)];
  return candidateRank > currentRank ||
    (candidateRank === currentRank && candidate.score > current.score);
}

async function resolveOcrNames(
  names: string[],
  resolve: ResolveNamesFn,
): Promise<ResolveResult[]> {
  const candidates = names.map(resolutionCandidates);
  const flatCandidates = candidates.flat();
  let resolved: ResolveResult[];
  try {
    resolved = await resolve(flatCandidates);
  } catch {
    return names.map(() => ({ key: null, score: 0, method: "none" }));
  }

  let offset = 0;
  return candidates.map((rowCandidates) => {
    let best: ResolveResult = { key: null, score: 0, method: "none" };
    for (let index = 0; index < rowCandidates.length; index++) {
      const result = resolved[offset + index];
      if (result && betterResolution(result, best)) best = result;
    }
    offset += rowCandidates.length;
    return best;
  });
}

/**
 * Recognize + parse a captured-region PNG into raw rows. Pure pipeline modulo
 * the (mockable) canvas adapter + Tesseract worker.
 */
export async function recognizeRows(
  dataUrl: string,
  opts: { preprocess?: PreprocessOptions; recognize?: RecognizeOptions } = {},
): Promise<OcrRow[]> {
  const processed = await preprocessDataUrlWithTransform(
    dataUrl,
    browserCanvasAdapter,
    opts.preprocess,
  );
  const recognition = await recognizeStructuredText(processed.dataUrl, opts.recognize);
  return recognition.lines.length > 0
    ? extractStructuredRows(recognition, processed.transform)
    : extractRows(recognition.text);
}

export interface RecognizedVariant {
  iconCrop: number;
  /** Text-only compatibility surface used by diagnostics. */
  text: string;
  recognition: StructuredRecognition;
  transform: PreprocessTransform;
  rows: OcrRow[];
}

/** Pair one recognition result with the exact crop transform that produced it. */
export function buildRecognizedVariant(
  iconCrop: number,
  recognition: StructuredRecognition,
  transform: PreprocessTransform,
): RecognizedVariant {
  return {
    iconCrop,
    text: recognition.text,
    recognition,
    transform,
    rows:
      recognition.lines.length > 0
        ? extractStructuredRows(recognition, transform)
        : extractRows(recognition.text),
  };
}

/**
 * Run a tight-region crop and a whole-panel crop through one OCR worker. The
 * caller chooses the result with the strongest catalogue resolution score.
 */
export async function recognizeRowVariants(
  dataUrl: string,
  iconCrops: number[],
  opts: { preprocess?: PreprocessOptions; recognize?: RecognizeOptions } = {},
): Promise<RecognizedVariant[]> {
  const frame = await browserCanvasAdapter.toFrame(dataUrl);
  const uniqueCrops = [...new Set(iconCrops)];
  const processedVariants = uniqueCrops.map((iconCrop) => ({
    iconCrop,
    processed: preprocessFrameWithTransform(frame, {
      ...opts.preprocess,
      iconCrop,
    }),
  }));
  const pngs: string[] = [];
  for (const variant of processedVariants) {
    pngs.push(await browserCanvasAdapter.fromFrame(variant.processed.frame));
  }
  const recognitions = await recognizeStructuredTexts(pngs, opts.recognize);
  return processedVariants.map(({ iconCrop, processed }, index) => {
    const recognition = recognitions[index] ?? { text: "", lines: [] };
    return buildRecognizedVariant(iconCrop, recognition, processed.transform);
  });
}

/** Native-canvas single-variant path used by the latency-sensitive overlay. */
export async function recognizeFrameVariant(
  frame: RgbaFrame,
  iconCrop: number,
  session: OcrSession,
  opts: { preprocess?: PreprocessOptions; recognize?: RecognizeOptions } = {},
): Promise<RecognizedVariant> {
  const processed = await preprocessFrameNative(frame, {
    ...opts.preprocess,
    iconCrop,
  });
  const recognition = await session.recognize(processed.image, opts.recognize);
  return buildRecognizedVariant(iconCrop, recognition, processed.transform);
}

export function resolutionScore(reads: SlotRead[]): number {
  return reads.reduce(
    (score, row) => score + (row.key ? 100 + Math.max(0, row.score) : 0),
    0,
  );
}

/** Reward panels expose at least four rows; fewer matches require another pass. */
export const MIN_REWARD_ROWS = 4;

/** Whether a quick pass is incomplete enough to warrant the accurate fallback. */
export function variantNeedsAccurateFallback(
  variant: RecognizedVariant,
  reads: SlotRead[],
): boolean {
  const resolved = reads.filter((read) => read.key !== null);
  if (resolved.length === 0) return true;
  if (resolved.length < MIN_REWARD_ROWS) return true;
  return false;
}

/**
 * Resolve + price raw rows into {@link SlotRead}s keyed by normalized screen Y
 * when geometry is available, with row-index fallback for clipboard text.
 * `resolve` is injected (the engine's `resolveName`) so this is unit-testable.
 */
export async function resolveAndPrice(
  rows: OcrRow[],
  resolve: ResolveNameFn,
): Promise<{ reads: SlotRead[]; priced: PricedRow[] }> {
  return resolveAndPriceBatch(rows, (raws) => Promise.all(raws.map(async (raw) => {
    try {
      return await resolve(raw);
    } catch {
      return { key: null, score: 0, method: "none" };
    }
  })));
}

/** Resolve all OCR rows in one worker round-trip, then attach live prices. */
export async function resolveAndPriceBatch(
  rows: OcrRow[],
  resolve: ResolveNamesFn,
): Promise<{ reads: SlotRead[]; priced: PricedRow[] }> {
  const reads: SlotRead[] = [];
  const priced: PricedRow[] = [];
  const resolutions = await resolveOcrNames(rows.map((row) => row.name), resolve);
  for (let i = 0; i < rows.length; i++) {
    const row = rows[i];
    const res = resolutions[i] ?? { key: null, score: 0, method: "none" };
    const method = coerceMethod(res.method);
    // Re-apply OCR (Nivel/Level N) onto bare Uncut * Gem keys for pricing.
    const key = applyResolvedUncutLevel(res.key, row.name);
    const displayName = key !== null && method !== "currency" ? key : row.name;
    const slot = row.geometry
      ? spatialSlotKey(row.geometry.center.y)
      : String(i);
    reads.push({
      slot,
      key,
      name: displayName,
      quantity: row.quantity,
      method,
      score: res.score,
      ocrConfidence: row.confidence,
      geometry: row.geometry,
    });
    priced.push(
      priceRow({
        key,
        name: displayName,
        quantity: row.quantity,
        method: res.method,
        score: res.score,
        ocrConfidence: row.confidence,
        geometry: row.geometry,
      }),
    );
  }
  return { reads, priced };
}
