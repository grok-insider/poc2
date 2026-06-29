// Price refresh orchestration: open the store, refresh on startup, every hour,
// and on league change. Exposes the current snapshot + status to the IPC layer.

import { openPriceStore, priceBackend, replaceLeaguePrices, priceSnapshot, priceCount } from "./store";
import { fetchAllPrices, resolveLeague } from "./poe2scout";
import type { PriceSnapshot, PriceStatus } from "./types";

const REFRESH_MS = 60 * 60 * 1000; // 1 hour

let timer: ReturnType<typeof setInterval> | null = null;
let currentLeague = "";
let fetchedAt: string | null = null;
let lastError: string | null = null;
let refreshing = false;

/** Initialise the store under `dir` and start the hourly schedule for `league`. */
export function startPriceScheduler(dir: string, league: string): void {
  openPriceStore(dir);
  currentLeague = league;
  // Refresh immediately (non-blocking), then every hour.
  void refreshNow();
  if (timer) clearInterval(timer);
  timer = setInterval(() => void refreshNow(), REFRESH_MS);
  // Don't keep the event loop alive solely for the price timer.
  timer.unref?.();
}

/** Switch leagues: re-point and refresh now (keeps the hourly cadence). */
export function setPriceLeague(league: string): void {
  if (league === currentLeague) return;
  currentLeague = league;
  void refreshNow();
}

/** Fetch from poe2scout and replace the cache for the active league. */
export async function refreshNow(): Promise<boolean> {
  if (refreshing) return false;
  refreshing = true;
  lastError = null;
  try {
    const resolved = await resolveLeague(currentLeague || undefined);
    // poe2scout may report a different current league than our setting; adopt it.
    currentLeague = resolved.Value;
    const rows = await fetchAllPrices(resolved);
    if (rows.length > 0) {
      replaceLeaguePrices(resolved.Value, rows);
      fetchedAt = new Date().toISOString();
    }
    return rows.length > 0;
  } catch (e) {
    lastError = e instanceof Error ? e.message : String(e);
    return false;
  } finally {
    refreshing = false;
  }
}

export function getPriceSnapshot(): PriceSnapshot {
  return priceSnapshot(currentLeague);
}

export function getPriceStatus(): PriceStatus {
  return {
    league: currentLeague,
    count: priceCount(currentLeague),
    fetchedAt,
    lastError,
    refreshing,
    backend: priceBackend(),
  };
}

export function stopPriceScheduler(): void {
  if (timer) clearInterval(timer);
  timer = null;
}
