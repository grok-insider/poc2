/// "MergeReads" de-flicker state machine for the OCR price scan (ADR-0013).
///
/// OCR over a re-scanned region flickers: a name resolves cleanly one frame,
/// garbles the next, the quantity drops for a frame, etc. This is a small
/// state machine — one slot per screen position (rows are top-to-bottom in a
/// fixed panel) — that stabilizes what the overlay shows across re-scans:
///
///   - EXACT match  → locks in a single read (high confidence, show now).
///   - FUZZY/PREFIX → needs 2 consecutive confirming reads of the SAME key
///     before it locks (avoids showing a one-frame mis-resolve).
///   - quantity memory survives ONE dropped frame: if a locked slot is absent
///     from a scan, it's held (with its last quantity) for that frame and only
///     expires if it's still missing on the next.
///
/// Pure + unit-tested: `applyScan(state, reads) -> { state, rows }`. No DOM.

import type { OcrLineGeometry } from "./extractRows";

/** How a read's name resolved (mirrors the engine `ResolveView.method`). */
export type ResolveMethod =
  | "exact"
  | "prefix"
  | "fuzzy"
  | "skeleton"
  | "currency"
  | "none";

/** Methods that lock on the first read (high confidence). */
const EXACT_METHODS: ReadonlySet<ResolveMethod> = new Set(["exact", "currency"]);

/** One resolved read from a single scan, for one screen-position slot. */
export interface SlotRead {
  /** Stable screen-position key (normalized Y bucket, or text-row index). */
  slot: string;
  /** Resolved canonical key, or null when unresolved. */
  key: string | null;
  /** Display name to show for the row. */
  name: string;
  /** Stack quantity parsed for the row. */
  quantity: number;
  /** How `key` resolved. */
  method: ResolveMethod;
  /** Resolver confidence in [0,1]. */
  score: number;
  /** Tesseract line confidence, when available. */
  ocrConfidence?: number;
  /** Source-normalized line geometry, when available. */
  geometry?: OcrLineGeometry;
}

/** Per-slot lock state retained between scans. */
export interface SlotState {
  key: string | null;
  name: string;
  quantity: number;
  method: ResolveMethod;
  score: number;
  ocrConfidence?: number;
  /** Latest bbox/baseline with a center smoothed across observed frames. */
  geometry?: OcrLineGeometry;
  /** True once the slot has met its confirmation threshold. */
  locked: boolean;
  /** Consecutive confirming reads of the current `key` (for fuzzy/prefix). */
  confirms: number;
  /** Scans this slot has been absent for (quantity-memory grace). */
  missing: number;
}

/** The de-flicker state: slot key → lock state. */
export type RowLockState = Record<string, SlotState>;

/** A stabilized, lock-confirmed row the overlay renders. */
export interface LockedRow {
  slot: string;
  key: string | null;
  name: string;
  quantity: number;
  method: ResolveMethod;
  score: number;
  ocrConfidence?: number;
  geometry?: OcrLineGeometry;
}

/** Scans a slot may be missing before it is dropped (1 dropped-frame grace). */
const MAX_MISSING = 1;

/** Equal previous/current weighting damps OCR bbox jitter without lagging much. */
const CENTER_SMOOTHING = 0.5;
const SPATIAL_MATCH_TOLERANCE = 0.035;

export function emptyRowLock(): RowLockState {
  return {};
}

function lockedRowOf(slot: string, s: SlotState): LockedRow {
  return {
    slot,
    key: s.key,
    name: s.name,
    quantity: s.quantity,
    method: s.method,
    score: s.score,
    ocrConfidence: s.ocrConfidence,
    geometry: s.geometry,
  };
}

function nextGeometry(
  before: OcrLineGeometry | undefined,
  current: OcrLineGeometry | undefined,
): OcrLineGeometry | undefined {
  if (!current) {
    return before
      ? {
          bbox: { ...before.bbox },
          baseline: { ...before.baseline },
          center: { ...before.center },
        }
      : undefined;
  }
  return {
    bbox: { ...current.bbox },
    baseline: { ...current.baseline },
    center: before
      ? {
          x:
            before.center.x * CENTER_SMOOTHING +
            current.center.x * (1 - CENTER_SMOOTHING),
          y:
            before.center.y * CENTER_SMOOTHING +
            current.center.y * (1 - CENTER_SMOOTHING),
        }
      : { ...current.center },
  };
}

/**
 * Fold one scan's reads into the lock state. Returns the next state plus the
 * currently-locked rows (in stable slot order). Pure — never mutates `prev`.
 */
export function applyScan(
  prev: RowLockState,
  reads: SlotRead[],
): { state: RowLockState; rows: LockedRow[] } {
  const next: RowLockState = {};
  const seen = new Set<string>();
  const associatedReads = associateSpatialReads(prev, reads);

  // 1) Advance every slot that appeared in this scan.
  for (const r of associatedReads) {
    seen.add(r.slot);
    const before = prev[r.slot];
    const isExact = EXACT_METHODS.has(r.method) && r.key !== null;
    const sameKey = before && before.key === r.key && r.key !== null;

    // Confirmation count: continues only while the resolved key is unchanged.
    const confirms = sameKey ? before.confirms + 1 : 1;
    // Exact locks immediately; fuzzy/prefix needs a 2nd confirming read.
    const locked = isExact || confirms >= 2 || (before?.locked === true && sameKey);

    next[r.slot] = {
      key: r.key,
      name: r.name,
      quantity: r.quantity,
      method: r.method,
      score: r.score,
      ocrConfidence: r.ocrConfidence,
      geometry: nextGeometry(before?.geometry, r.geometry),
      locked,
      confirms,
      missing: 0,
    };
  }

  // 2) Hold absent slots for one frame (quantity memory), then expire.
  for (const slot of Object.keys(prev)) {
    if (seen.has(slot)) continue;
    const before = prev[slot];
    if (before.missing < MAX_MISSING) {
      next[slot] = { ...before, missing: before.missing + 1 };
    }
    // else: dropped — omit from `next`.
  }

  // 3) Emit locked rows by geometry, with legacy slot ordering as fallback.
  const rows = Object.keys(next)
    .filter((slot) => next[slot].locked)
    .sort((a, b) => compareSlotStates(a, next[a], b, next[b]))
    .map((slot) => lockedRowOf(slot, next[slot]));

  return { state: next, rows };
}

function associateSpatialReads(
  previous: RowLockState,
  reads: SlotRead[],
): SlotRead[] {
  const used = new Set<string>();
  return reads.map((read) => {
    const y = read.geometry?.center.y;
    if (y === undefined) return read;
    let bestSlot: string | null = null;
    let bestDistance = Infinity;
    for (const [slot, state] of Object.entries(previous)) {
      if (used.has(slot) || state.geometry?.center.y === undefined) continue;
      const distance = Math.abs(state.geometry.center.y - y);
      if (distance > SPATIAL_MATCH_TOLERANCE) continue;
      const keyPenalty =
        read.key !== null && state.key !== null && read.key !== state.key ? 0.02 : 0;
      if (distance + keyPenalty < bestDistance) {
        bestDistance = distance + keyPenalty;
        bestSlot = slot;
      }
    }
    if (!bestSlot) return read;
    used.add(bestSlot);
    return { ...read, slot: bestSlot };
  });
}

function compareSlotStates(
  a: string,
  aState: SlotState,
  b: string,
  bState: SlotState,
): number {
  const ay = aState.geometry?.center.y;
  const by = bState.geometry?.center.y;
  if (ay !== undefined && by !== undefined && ay !== by) return ay - by;
  return compareSlots(a, b);
}

function compareSlots(a: string, b: string): number {
  const na = Number(a);
  const nb = Number(b);
  if (Number.isFinite(na) && Number.isFinite(nb)) return na - nb;
  return a < b ? -1 : a > b ? 1 : 0;
}
