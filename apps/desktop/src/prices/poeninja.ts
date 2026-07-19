// poe.ninja PoE2 fallback fetcher. poe2scout is the authoritative price source;
// this fills gaps for names poe2scout did not price (e.g. categories poe2scout
// doesn't cover yet). poe.ninja's PoE2 economy surface is still beta (see
// crates/market/src/meta.rs), so EVERY step here soft-fails: an unknown shape,
// a missing endpoint, or a non-200 yields zero rows and leaves the poe2scout
// cache untouched. Runs in Electron main via `electron.net` (no CORS).
//
// Rows are tagged `category: "ninja"` and a divine price is derived directly
// from the line's `divineValue` (preferred) or `chaosValue / chaosPerDivine`.
// They are appended AFTER poe2scout rows so the store's "first write wins"
// dedupe keeps poe2scout authoritative and ninja only fills blanks.

import { normalizeName } from "./normalize";
import type { PriceRow } from "./types";

const BASE = "https://poe.ninja/api/data/poe2";
const UA = "poc2/1.1 (Path of Crafting 2; github.com/0xfell) Electron";

// poe.ninja splits its economy into currency-style overviews (Currency,
// Fragment) and item-style overviews. We pull the broad currency/fragment set;
// items fall to poe2scout. These names are best-effort — a 404 is swallowed.
const CURRENCY_OVERVIEWS = ["Currency", "Fragment", "Rune", "Omen"] as const;

/** A single poe.ninja economy line; fields are all optional (beta surface). */
interface NinjaLine {
  currencyTypeName?: string;
  name?: string;
  chaosValue?: number;
  chaosEquivalent?: number;
  divineValue?: number;
}

interface NinjaOverview {
  lines?: NinjaLine[];
}

async function getJson(url: string): Promise<unknown> {
  const { net } = await import("electron");
  const res = await net.fetch(url, {
    headers: { "User-Agent": UA, Accept: "application/json" },
  });
  if (!res.ok) throw new Error(`poe.ninja ${res.status} for ${url}`);
  return res.json();
}

/**
 * Pure transform of one poe.ninja overview into `PriceRow`s. `chaosPerDivine`
 * converts chaos-denominated lines to divine; pass `0` to skip that derivation.
 * Exported for unit testing without electron.
 */
export function parseNinjaOverview(
  overview: NinjaOverview,
  league: string,
  chaosPerDivine: number,
  fetchedAt: string,
): PriceRow[] {
  const out: PriceRow[] = [];
  for (const line of overview.lines ?? []) {
    const name = line.currencyTypeName ?? line.name;
    if (!name) continue;
    let priceDivine: number | null = null;
    if (typeof line.divineValue === "number" && Number.isFinite(line.divineValue)) {
      priceDivine = line.divineValue;
    } else {
      const chaos =
        typeof line.chaosValue === "number"
          ? line.chaosValue
          : typeof line.chaosEquivalent === "number"
            ? line.chaosEquivalent
            : null;
      if (chaos !== null && chaosPerDivine > 0) priceDivine = chaos / chaosPerDivine;
    }
    if (priceDivine === null) continue;
    out.push({
      league,
      category: "ninja",
      apiId: `ninja:${normalizeName(name)}`,
      name,
      normalizedName: normalizeName(name),
      priceExalt: null,
      priceDivine,
      stackMax: null,
      iconUrl: null,
      fetchedAt,
    });
  }
  return out;
}

/**
 * Best-effort poe.ninja fallback fetch. Returns `[]` on any failure — callers
 * append the result to the poe2scout rows, so an empty result is a clean no-op.
 * `chaosPerDivine` comes from poe2scout's league metadata (`ChaosDivinePrice`).
 */
export async function fetchNinjaFallback(
  league: string,
  chaosPerDivine: number,
): Promise<PriceRow[]> {
  const fetchedAt = new Date().toISOString();
  const rows: PriceRow[] = [];
  for (const overview of CURRENCY_OVERVIEWS) {
    const url = `${BASE}/${overview.toLowerCase()}overview?league=${encodeURIComponent(league)}`;
    let data: NinjaOverview;
    try {
      data = (await getJson(url)) as NinjaOverview;
    } catch {
      // Missing/beta endpoint for this overview — skip, never abort.
      continue;
    }
    try {
      rows.push(...parseNinjaOverview(data, league, chaosPerDivine, fetchedAt));
    } catch {
      continue;
    }
  }
  return rows;
}
