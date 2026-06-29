// Persistent price store. Prefers Node's built-in `node:sqlite` (Electron 41's
// Node has it; stable in Node 24) so there is NO native dependency to rebuild
// for packaging — the `better-sqlite3` ABI/prebuild fragility is avoided. If
// `node:sqlite` is unavailable (older Node, import throws), it degrades to a
// JSON file with the same API, and finally to an in-memory map.
//
// Path: <userData>/prices.db (or prices.json), matching the desktop's existing
// convention (windowState/captureRegion also live under app.getPath userData).

import { existsSync, readFileSync, writeFileSync, mkdirSync } from "node:fs";
import path from "node:path";
import type { PriceRow, PriceSnapshot, PriceInfo } from "./types";

type Backend = "sqlite" | "json" | "memory";

interface Db {
  backend: Backend;
  replaceLeague(league: string, rows: PriceRow[]): void;
  snapshot(league: string): PriceSnapshot;
  count(league: string): number;
}

// ─────────────────────────── sqlite backend ───────────────────────────

function trySqlite(file: string): Db | null {
  let DatabaseSync: (new (p: string) => SqliteDb) | undefined;
  try {
    // Built-in; lazy require so a runtime without it can't crash module load.
    // eslint-disable-next-line @typescript-eslint/no-require-imports
    ({ DatabaseSync } = require("node:sqlite"));
  } catch {
    return null;
  }
  if (!DatabaseSync) return null;
  try {
    const db = new DatabaseSync(file);
    db.exec(`
      CREATE TABLE IF NOT EXISTS prices (
        league TEXT NOT NULL,
        category TEXT NOT NULL,
        api_id TEXT NOT NULL,
        name TEXT NOT NULL,
        normalized_name TEXT NOT NULL,
        price_exalt REAL,
        price_divine REAL,
        stack_max INTEGER,
        icon_url TEXT,
        fetched_at TEXT NOT NULL,
        PRIMARY KEY (league, category, api_id)
      );
      CREATE INDEX IF NOT EXISTS idx_prices_lookup ON prices (league, normalized_name);
    `);
    return {
      backend: "sqlite",
      replaceLeague(league, rows) {
        const del = db.prepare("DELETE FROM prices WHERE league = ?");
        const ins = db.prepare(
          `INSERT OR REPLACE INTO prices
           (league, category, api_id, name, normalized_name, price_exalt, price_divine, stack_max, icon_url, fetched_at)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
        );
        db.exec("BEGIN");
        try {
          del.run(league);
          for (const r of rows) {
            ins.run(
              r.league, r.category, r.apiId, r.name, r.normalizedName,
              r.priceExalt, r.priceDivine, r.stackMax, r.iconUrl, r.fetchedAt,
            );
          }
          db.exec("COMMIT");
        } catch (e) {
          db.exec("ROLLBACK");
          throw e;
        }
      },
      snapshot(league) {
        const rows = db
          .prepare("SELECT name, normalized_name, price_divine, fetched_at FROM prices WHERE league = ?")
          .all(league) as Array<{
            name: string;
            normalized_name: string;
            price_divine: number | null;
            fetched_at: string;
          }>;
        return rowsToSnapshot(league, rows.map((r) => ({
          name: r.name,
          normalizedName: r.normalized_name,
          priceDivine: r.price_divine,
          fetchedAt: r.fetched_at,
        })));
      },
      count(league) {
        const r = db.prepare("SELECT COUNT(*) AS n FROM prices WHERE league = ?").get(league) as { n: number };
        return r?.n ?? 0;
      },
    };
  } catch {
    return null;
  }
}

interface SqliteDb {
  exec(sql: string): void;
  prepare(sql: string): { run(...a: unknown[]): unknown; get(...a: unknown[]): unknown; all(...a: unknown[]): unknown[] };
}

// ──────────────────────────── json backend ────────────────────────────

function jsonBackend(file: string): Db {
  type Store = Record<string, PriceRow[]>; // league → rows
  function load(): Store {
    try {
      if (existsSync(file)) return JSON.parse(readFileSync(file, "utf8")) as Store;
    } catch {
      /* corrupt/missing → empty */
    }
    return {};
  }
  function save(s: Store): void {
    try {
      mkdirSync(path.dirname(file), { recursive: true });
      writeFileSync(file, JSON.stringify(s));
    } catch {
      /* best-effort */
    }
  }
  let mem: Store | null = null;
  const get = (): Store => (mem ??= load());
  return {
    backend: "json",
    replaceLeague(league, rows) {
      const s = get();
      s[league] = rows;
      save(s);
    },
    snapshot(league) {
      return rowsToSnapshot(league, get()[league] ?? []);
    },
    count(league) {
      return (get()[league] ?? []).length;
    },
  };
}

// ─────────────────────────── shared helpers ───────────────────────────

function rowsToSnapshot(
  league: string,
  rows: Array<Pick<PriceRow, "name" | "normalizedName" | "priceDivine" | "fetchedAt">>,
): PriceSnapshot {
  const names: string[] = [];
  const byName: Record<string, PriceInfo> = {};
  let fetchedAt: string | null = null;
  for (const r of rows) {
    names.push(r.name);
    if (typeof r.priceDivine === "number" && Number.isFinite(r.priceDivine)) {
      // First write wins on duplicate normalized names (poe2scout ids are unique
      // but two display names can normalize equal — keep the first deterministic).
      byName[r.normalizedName] ??= { perUnit: r.priceDivine, unit: "div" };
    }
    if (!fetchedAt || (r.fetchedAt && r.fetchedAt > fetchedAt)) fetchedAt = r.fetchedAt;
  }
  return { league, names, byName, fetchedAt };
}

// ───────────────────────────── public API ─────────────────────────────

let db: Db | null = null;

/** Open (once) the price store under `dir`. Resolves the best available backend. */
export function openPriceStore(dir: string): { backend: Backend } {
  if (db) return { backend: db.backend };
  mkdirSync(dir, { recursive: true });
  db = trySqlite(path.join(dir, "prices.db")) ?? jsonBackend(path.join(dir, "prices.json"));
  return { backend: db.backend };
}

export function priceBackend(): Backend {
  return db?.backend ?? "memory";
}

/** Replace all cached rows for a league with a fresh fetch. */
export function replaceLeaguePrices(league: string, rows: PriceRow[]): void {
  db?.replaceLeague(league, rows);
}

/** Flattened snapshot for the renderer (names + normalized→price). */
export function priceSnapshot(league: string): PriceSnapshot {
  return db?.snapshot(league) ?? { league, names: [], byName: {}, fetchedAt: null };
}

export function priceCount(league: string): number {
  return db?.count(league) ?? 0;
}
