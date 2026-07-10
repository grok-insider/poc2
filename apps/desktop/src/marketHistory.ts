import { app } from "electron";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";

const MAX_HISTORY = 200;

export type OverlayMarketHistoryKind = "item-price" | "reward-scan";

export interface OverlayMarketHistoryEntry {
  id: string;
  kind: OverlayMarketHistoryKind;
  createdAt: string;
  title: string;
  league?: string;
  summary: string;
  rows: Array<{ label: string; value?: string; detail?: string }>;
}

const FILE = () => path.join(app.getPath("userData"), "market-history.json");

function loadRaw(): OverlayMarketHistoryEntry[] {
  try {
    const parsed = JSON.parse(readFileSync(FILE(), "utf8")) as unknown;
    if (!Array.isArray(parsed)) return [];
    return parsed.filter((x): x is OverlayMarketHistoryEntry => {
      const entry = x as Partial<OverlayMarketHistoryEntry>;
      return typeof entry.id === "string" && typeof entry.kind === "string" && typeof entry.title === "string";
    });
  } catch {
    return [];
  }
}

function saveRaw(entries: OverlayMarketHistoryEntry[]): void {
  try {
    mkdirSync(path.dirname(FILE()), { recursive: true });
    writeFileSync(FILE(), JSON.stringify(entries.slice(0, MAX_HISTORY), null, 2));
  } catch {
    // best-effort
  }
}

export function addMarketHistory(
  entry: Omit<OverlayMarketHistoryEntry, "id" | "createdAt"> & {
    id?: string;
    createdAt?: string;
  },
): OverlayMarketHistoryEntry {
  const full: OverlayMarketHistoryEntry = {
    id: entry.id ?? `${Date.now()}-${Math.random().toString(16).slice(2)}`,
    createdAt: entry.createdAt ?? new Date().toISOString(),
    kind: entry.kind,
    title: entry.title,
    league: entry.league,
    summary: entry.summary,
    rows: entry.rows,
  };
  saveRaw([full, ...loadRaw()].slice(0, MAX_HISTORY));
  return full;
}

export function listMarketHistory(): OverlayMarketHistoryEntry[] {
  return loadRaw();
}
