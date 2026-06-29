"use client";

/// Wires the desktop poe2scout price cache into the OCR overlay's price seam.
///
/// The overlay prices each resolved row via `window.poc2PriceSource(key)`
/// (apps/web/lib/ocr/priceSource.ts). The `key` is whatever `engine.resolveName`
/// returned — when the overlay passes the cached `names` as fuzzy `candidates`,
/// that key is the matched display name. So we install a source keyed by
/// `normalizeName(name)` and normalize the lookup key the same way.
///
/// Also returns the snapshot's `names` so the overlay can feed them to
/// `resolveName({ raw, candidates })` — this is what lets noisy OCR'd rune /
/// idol / alloy names resolve against the FULL poe2scout catalogue instead of
/// the engine valuator's tiny built-in currency list.

import { getDesktopBridge, type PriceSnapshot } from "@/lib/desktop";
import { normalizeName } from "./normalize";

let cached: PriceSnapshot | null = null;

/** Install/refresh `window.poc2PriceSource` from the desktop cache. */
export async function loadPriceSource(): Promise<PriceSnapshot | null> {
  const bridge = getDesktopBridge();
  if (!bridge?.pricesSnapshot) return null;
  let snap: PriceSnapshot;
  try {
    snap = await bridge.pricesSnapshot();
  } catch {
    return null;
  }
  cached = snap;
  if (typeof window !== "undefined") {
    window.poc2PriceSource = (key: string) => {
      const info = snap.byName[normalizeName(key)];
      return info ?? null;
    };
  }
  return snap;
}

/** Candidate display names for the fuzzy matcher (empty when no cache). */
export function priceCandidates(): string[] {
  return cached?.names ?? [];
}

/** The last loaded snapshot, if any. */
export function currentPriceSnapshot(): PriceSnapshot | null {
  return cached;
}
