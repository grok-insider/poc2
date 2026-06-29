// poe2scout fetcher — pulls the broad price catalogue for a league and converts
// each entry into a `PriceRow`. Runs in the Electron main process via
// `electron.net` (no CORS), mirroring trade/proxy.ts. Pure parsing is split out
// (`parseCategoryPage`) so it can be unit-tested without electron.

import { normalizeName } from "./normalize";
import { POE2SCOUT_PRICE_CATEGORIES, type PriceRow } from "./types";

const BASE = "https://poe2scout.com/api/poe2";
const UA = "poc2/1.1 (Path of Crafting 2; github.com/0xfell) Electron";

/** League metadata from `/Leagues` (PascalCase, straight off the API). */
export interface ScoutLeague {
  Value: string;
  DivinePrice: number;
  ChaosDivinePrice: number;
  IsCurrent?: boolean;
}

interface ScoutEntry {
  ApiId: string;
  Text?: string;
  CategoryApiId?: string;
  IconUrl?: string;
  currentPrice?: number | null;
  CurrentPrice?: number | null;
  ItemMetadata?: { name?: string; max_stack_size?: number };
}

interface ScoutPage {
  CurrentPage?: number;
  Pages?: number;
  Total?: number;
  Items?: ScoutEntry[];
}

async function getJson(url: string): Promise<unknown> {
  const { net } = await import("electron");
  const res = await net.fetch(url, {
    headers: { "User-Agent": UA, Accept: "application/json" },
  });
  if (!res.ok) throw new Error(`poe2scout ${res.status} for ${url}`);
  return res.json();
}

/** Resolve the league to fetch: explicit name, else poe2scout's `IsCurrent`. */
export async function resolveLeague(preferred?: string): Promise<ScoutLeague> {
  const leagues = (await getJson(`${BASE}/Leagues`)) as ScoutLeague[];
  if (!Array.isArray(leagues) || leagues.length === 0) {
    throw new Error("poe2scout returned no leagues");
  }
  if (preferred) {
    const hit = leagues.find((l) => l.Value === preferred);
    if (hit) return hit;
  }
  const current = leagues.find((l) => l.IsCurrent) ?? leagues[0];
  if (!current) throw new Error("poe2scout returned no leagues");
  return current;
}

/**
 * Pure transform of one `/Currencies/ByCategory` page into `PriceRow`s.
 * `divinePrice` = exalts-per-divine for the league (to derive divine prices).
 */
export function parseCategoryPage(
  page: ScoutPage,
  league: string,
  category: string,
  divinePrice: number,
  fetchedAt: string,
): PriceRow[] {
  const out: PriceRow[] = [];
  for (const it of page.Items ?? []) {
    const name = it.Text ?? it.ItemMetadata?.name ?? null;
    if (!name) continue;
    const priceExalt =
      typeof it.currentPrice === "number"
        ? it.currentPrice
        : typeof it.CurrentPrice === "number"
          ? it.CurrentPrice
          : null;
    const priceDivine =
      priceExalt !== null && divinePrice > 0 ? priceExalt / divinePrice : null;
    out.push({
      league,
      category,
      apiId: it.ApiId,
      name,
      normalizedName: normalizeName(name),
      priceExalt,
      priceDivine,
      stackMax: it.ItemMetadata?.max_stack_size ?? null,
      iconUrl: it.IconUrl ?? null,
      fetchedAt,
    });
  }
  return out;
}

/** Fetch + flatten every catalogue category for `league`. */
export async function fetchAllPrices(league: ScoutLeague): Promise<PriceRow[]> {
  const fetchedAt = new Date().toISOString();
  const rows: PriceRow[] = [];
  for (const category of POE2SCOUT_PRICE_CATEGORIES) {
    for (let pageNo = 1; ; pageNo += 1) {
      const url =
        `${BASE}/Leagues/${encodeURIComponent(league.Value)}` +
        `/Currencies/ByCategory?Category=${category}&Page=${pageNo}&PerPage=250`;
      let page: ScoutPage;
      try {
        page = (await getJson(url)) as ScoutPage;
      } catch {
        // A single category failing must not abort the whole refresh.
        break;
      }
      rows.push(...parseCategoryPage(page, league.Value, category, league.DivinePrice, fetchedAt));
      if (pageNo >= Math.max(page.Pages ?? 1, 1)) break;
    }
  }
  return rows;
}
