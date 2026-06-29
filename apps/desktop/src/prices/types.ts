// Shared types for the poe2scout price cache (ADR-0013 follow-up).
//
// The cache lives in the desktop shell (Electron main / Node), backed by the
// built-in node:sqlite when available and a JSON file otherwise. The renderer
// reads a flattened snapshot over the bridge to price OCR'd reward-panel rows
// (the OCR overlay) without any per-lookup IPC round-trips.

/** One cached price row, keyed by `(league, category, apiId)`. */
export interface PriceRow {
  league: string;
  category: string;
  apiId: string;
  /** Display name (poe2scout `Text` / `ItemMetadata.name`). */
  name: string;
  /** `normalizeName(name)` — the lookup key shared with the matcher. */
  normalizedName: string;
  /** Price in exalts (poe2scout `current_price` is in the base currency). */
  priceExalt: number | null;
  /** Price in divines, derived via the league's DivinePrice. */
  priceDivine: number | null;
  /** Max stack size, when known (drives "N (x each)" overlay totals). */
  stackMax: number | null;
  iconUrl: string | null;
  /** ISO-8601 fetch time. */
  fetchedAt: string;
}

/** A single resolved unit price the overlay's `window.poc2PriceSource` returns. */
export interface PriceInfo {
  /** Per-unit price in the chosen display unit. */
  perUnit: number;
  /** Unit label (e.g. "div" | "ex"). */
  unit: string;
}

/**
 * Flattened snapshot the renderer consumes. `names` feeds the fuzzy matcher as
 * ad-hoc `candidates`; `byName` maps `normalizeName(name)` → price.
 */
export interface PriceSnapshot {
  league: string;
  /** Display names (for fuzzy `candidates`). */
  names: string[];
  /** `normalizedName` → price info. */
  byName: Record<string, PriceInfo>;
  /** ISO-8601 time of the snapshot's underlying fetch, or null if never. */
  fetchedAt: string | null;
}

/** Status surface for the Settings panel / diagnostics. */
export interface PriceStatus {
  league: string;
  count: number;
  fetchedAt: string | null;
  lastError: string | null;
  refreshing: boolean;
  /** "sqlite" | "json" | "memory" — which backend the store fell back to. */
  backend: string;
}

/** poe2scout categories the cache fetches (broad set; overlay prefers exact). */
export const POE2SCOUT_PRICE_CATEGORIES = [
  "currency",
  "runes",
  "idols",
  "omens",
  "essences",
  "ritual",
  "catalysts",
  "expedition",
  "fragments",
  "soulcores",
  "talismans",
] as const;
