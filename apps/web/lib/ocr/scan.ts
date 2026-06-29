"use client";

/// One OCR price-scan pass (ADR-0013): a captured-region data-URL in, resolved
/// + de-flickered priced rows out. Ties the pure pieces (preprocess →
/// recognize → extractRows → resolveName → price) together for the overlay.
///
/// The capture itself (bridge.captureRegion) and the lock state live in the
/// overlay component; this module is the stateless compute spine so it can be
/// exercised without the desktop bridge.

import { browserCanvasAdapter } from "./canvas";
import { preprocessDataUrl, type PreprocessOptions } from "./preprocess";
import { extractRows, type OcrRow } from "./extractRows";
import { recognizeText, type RecognizeOptions } from "./tesseract";
import type { ResolveMethod, SlotRead } from "./rowLock";
import { priceRow, type PricedRow } from "./priceSource";

/** Engine `resolveName` shape (mirrors lib/types ResolveView), injected so the
 * scan stays testable without the worker. */
export interface ResolveResult {
  key: string | null;
  score: number;
  method: string;
}
export type ResolveNameFn = (raw: string) => Promise<ResolveResult>;

const KNOWN_METHODS: ReadonlySet<string> = new Set([
  "exact",
  "prefix",
  "fuzzy",
  "skeleton",
  "currency",
  "none",
]);

function coerceMethod(m: string): ResolveMethod {
  return (KNOWN_METHODS.has(m) ? m : "fuzzy") as ResolveMethod;
}

/**
 * Recognize + parse a captured-region PNG into raw rows. Pure pipeline modulo
 * the (mockable) canvas adapter + Tesseract worker.
 */
export async function recognizeRows(
  dataUrl: string,
  opts: { preprocess?: PreprocessOptions; recognize?: RecognizeOptions } = {},
): Promise<OcrRow[]> {
  const png = await preprocessDataUrl(dataUrl, browserCanvasAdapter, opts.preprocess);
  const text = await recognizeText(png, opts.recognize);
  return extractRows(text);
}

/**
 * Resolve + price a list of raw rows into {@link SlotRead}s keyed by screen
 * position (row index), ready to feed the rowLock state machine. `resolve` is
 * injected (the engine's `resolveName`) so this is unit-testable.
 */
export async function resolveAndPrice(
  rows: OcrRow[],
  resolve: ResolveNameFn,
): Promise<{ reads: SlotRead[]; priced: PricedRow[] }> {
  const reads: SlotRead[] = [];
  const priced: PricedRow[] = [];
  for (let i = 0; i < rows.length; i++) {
    const row = rows[i];
    let res: ResolveResult = { key: null, score: 0, method: "none" };
    try {
      res = await resolve(row.name);
    } catch {
      // best-effort: an unresolved row still shows by name.
    }
    reads.push({
      slot: String(i),
      key: res.key,
      name: row.name,
      quantity: row.quantity,
      method: coerceMethod(res.method),
      score: res.score,
    });
    priced.push(
      priceRow({
        key: res.key,
        name: row.name,
        quantity: row.quantity,
        method: res.method,
        score: res.score,
      }),
    );
  }
  return { reads, priced };
}
