"use client";

/// Best-effort price lookup for the OCR scan overlay (ADR-0013).
///
/// The overlay annotates each resolved row with a price, but the price data
/// itself ships on a SIBLING branch (poe.ninja / poe2scout `applyNinjaPrices`).
/// To stay compile-independent we read whatever the app already exposes through
/// a tiny optional global seam and degrade to "no price" when it's absent:
///
///   window.poc2PriceSource?: (key: string) => PriceInfo | null | undefined
///
/// The price branch sets `window.poc2PriceSource` once its snapshot is loaded;
/// the overlay calls {@link lookupPrice} per resolved key. No price data here,
/// no import of the price module — purely a lookup adapter with a null floor.

import type { OcrLineGeometry } from "./extractRows";

/** A unit price for one resolved currency/item key. */
export interface PriceInfo {
  /** Price per unit, in the source's display unit (e.g. divine or exalt). */
  perUnit: number;
  /** Currency/unit label, when known (e.g. "div", "ex", "c"). */
  unit?: string | null;
  /** Canonical Divine value used to compare mixed display units. */
  perUnitDivine?: number | null;
  perUnitExalt?: number | null;
}

/** A resolved row decorated with its (best-effort) price. */
export interface PricedRow {
  key: string | null;
  name: string;
  quantity: number;
  /** Per-unit price, or null when no price is available. */
  perUnit: number | null;
  /** quantity × perUnit, or null. */
  total: number | null;
  totalDivine: number | null;
  unit: string | null;
  method: string;
  score: number;
  ocrConfidence?: number;
  geometry?: OcrLineGeometry;
}

type PriceSourceFn = (key: string) => PriceInfo | null | undefined;

declare global {
  interface Window {
    /** Optional price provider, set by the prices branch when loaded. */
    poc2PriceSource?: PriceSourceFn;
  }
}

/** The active price source, or null when the prices branch isn't present. */
export function getPriceSource(): PriceSourceFn | null {
  if (typeof window === "undefined") return null;
  return typeof window.poc2PriceSource === "function" ? window.poc2PriceSource : null;
}

/** Look up a single key's price, returning null on any miss (best-effort). */
export function lookupPrice(key: string | null): PriceInfo | null {
  if (!key) return null;
  const src = getPriceSource();
  if (!src) return null;
  try {
    return src(key) ?? null;
  } catch {
    return null;
  }
}

/** Decorate a resolved row with its price (null-priced when unavailable). */
export function priceRow(input: {
  key: string | null;
  name: string;
  quantity: number;
  method: string;
  score: number;
  ocrConfidence?: number;
  geometry?: OcrLineGeometry;
}): PricedRow {
  const info = lookupPrice(input.key);
  const totalDivine =
    typeof info?.perUnitDivine === "number"
      ? info.perUnitDivine * input.quantity
      : info?.unit === "div" && typeof info.perUnit === "number"
        ? info.perUnit * input.quantity
        : null;
  const useDivine = totalDivine !== null && totalDivine >= 1;
  const selectedPerUnit = useDivine
    ? info?.perUnitDivine
    : typeof info?.perUnitExalt === "number"
      ? info.perUnitExalt
      : info?.perUnit;
  const perUnit = typeof selectedPerUnit === "number" ? selectedPerUnit : null;
  const total = perUnit !== null ? perUnit * input.quantity : null;
  return {
    key: input.key,
    name: input.name,
    quantity: input.quantity,
    perUnit,
    total,
    totalDivine,
    unit: info
      ? useDivine
        ? "div"
        : typeof info.perUnitExalt === "number"
          ? "ex"
          : info.unit ?? null
      : null,
    method: input.method,
    score: input.score,
    ocrConfidence: input.ocrConfidence,
    geometry: input.geometry,
  };
}

/** Index of the highest-value row (by total), or -1 when nothing is priced. */
export function highestValueIndex(rows: PricedRow[]): number {
  let best = -1;
  let bestVal = -Infinity;
  rows.forEach((r, i) => {
    const comparable = r.totalDivine ?? r.total;
    if (comparable !== null && comparable > bestVal) {
      bestVal = comparable;
      best = i;
    }
  });
  return best;
}
