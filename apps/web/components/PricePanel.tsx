"use client";

/// Price check — matches the imported item's stat lines against the official
/// trade2 stat ids (public/trade-stats.json) and builds a search query. In the
/// desktop shell the query runs through the main-process proxy (rate-limited,
/// no CORS); in a plain browser only the deep link works.

import { useCallback, useEffect, useMemo, useState } from "react";
import { ExternalLink, RefreshCw, Search } from "lucide-react";
import { useCraft } from "@/lib/store";
import { getDesktopBridge, isDesktop, type OverlayMarketHistoryEntry } from "@/lib/desktop";
import {
  buildIndex,
  loadTradeStats,
  type IndexedStat,
  type TradeStatsFile,
} from "@/lib/trade/statIndex";
import {
  buildTradeQuery,
  extractItemLines,
  tradeSiteUrl,
  type BuiltTradeQuery,
} from "@/lib/trade/queryBuilder";
import { BaseIcon } from "@/components/BaseIcon";
import { humanizeId, rarityClass } from "@/lib/format";
import type { Item } from "@/lib/types";
import styles from "./PricePanel.module.css";

type StatsState = "loading" | "missing" | "ready";

interface RowOverride {
  enabled?: boolean;
  /** Raw input text for the bound field (kept stringly while editing). */
  bound?: string;
}

interface Listing {
  account: string;
  amount: number | null;
  currency: string | null;
  ilvl: number | null;
}

interface ListingGroup extends Listing {
  count: number;
}

/** Defensively map the trade2 fetch JSON (r.listing.price / account, r.item). */
function parseListings(json: unknown): Listing[] {
  const result = (json as { result?: unknown } | null)?.result;
  if (!Array.isArray(result)) return [];
  const out: Listing[] = [];
  for (const raw of result) {
    if (!raw || typeof raw !== "object") continue;
    const r = raw as {
      listing?: {
        price?: { amount?: unknown; currency?: unknown };
        account?: { name?: unknown };
      };
      item?: { ilvl?: unknown };
    };
    out.push({
      account: typeof r.listing?.account?.name === "string" ? r.listing.account.name : "—",
      amount: typeof r.listing?.price?.amount === "number" ? r.listing.price.amount : null,
      currency:
        typeof r.listing?.price?.currency === "string" ? r.listing.price.currency : null,
      ilvl: typeof r.item?.ilvl === "number" ? r.item.ilvl : null,
    });
  }
  return out;
}

/** Collapse identical (account, price) listings into one ×n row. */
function groupListings(listings: Listing[]): ListingGroup[] {
  const groups = new Map<string, ListingGroup>();
  for (const l of listings) {
    const key = `${l.account}|${l.amount}|${l.currency}`;
    const existing = groups.get(key);
    if (existing) existing.count += 1;
    else groups.set(key, { ...l, count: 1 });
  }
  return [...groups.values()];
}

function fmtPrice(l: Listing): string {
  if (l.amount === null) return "no price";
  return `${l.amount} ${l.currency ?? ""}`.trim();
}

function fmtHistoryTime(iso: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return iso;
  return date.toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

/* ---------- stat rows + actions + results (state resets per import) ----- */

function SearchSection({
  index,
  lines,
  item,
  baseName,
  league,
  desktop,
}: {
  index: Map<string, IndexedStat[]>;
  lines: string[];
  item: Item;
  baseName: string | null;
  league: string;
  desktop: boolean;
}) {
  const [overrides, setOverrides] = useState<Record<number, RowOverride>>({});
  const [includeIlvl, setIncludeIlvl] = useState(false);
  const [searching, setSearching] = useState(false);
  const [searchError, setSearchError] = useState<string | null>(null);
  const [searchNote, setSearchNote] = useState<string | null>(null);
  const [listings, setListings] = useState<Listing[] | null>(null);
  const [total, setTotal] = useState(0);

  const built: BuiltTradeQuery | null = useMemo(() => {
    if (lines.length === 0) return null;
    const base = {
      lines,
      baseName,
      rarity: item.rarity,
      ilvl: item.ilvl,
      league,
      index,
    };
    // First pass discovers matched rows; the second applies user overrides
    // (both are pure and cheap — the index lookup dominates).
    const draft = buildTradeQuery(base);
    const enabled = new Set<number>();
    const bounds = new Map<number, number>();
    draft.rows.forEach((row, i) => {
      if (!row.id) return;
      if (overrides[i]?.enabled !== false) enabled.add(i);
      const bound = Number(overrides[i]?.bound);
      if (overrides[i]?.bound !== undefined && Number.isFinite(bound)) {
        bounds.set(i, bound);
      }
    });
    return buildTradeQuery({ ...base, enabled, bounds, includeIlvl });
  }, [index, lines, baseName, item.rarity, item.ilvl, league, overrides, includeIlvl]);

  const rows = built?.rows ?? [];
  const matchedCount = rows.filter((r) => r.id !== null).length;

  function setRowEnabled(i: number, enabled: boolean) {
    setOverrides((o) => ({ ...o, [i]: { ...o[i], enabled } }));
  }

  function setRowBound(i: number, bound: string) {
    setOverrides((o) => ({ ...o, [i]: { ...o[i], bound } }));
  }

  async function runSearch() {
    const bridge = getDesktopBridge();
    if (!bridge || !built) return;
    setSearching(true);
    setSearchError(null);
    setSearchNote(null);
    setListings(null);
    try {
      let query = built.query;
      let search;
      try {
        search = await bridge.tradeSearch(league, query);
      } catch (e) {
        // Bases the trade API doesn't know (renamed/legacy/unresolved) 400
        // with "Unknown item base type" — degrade to a stats-only search
        // rather than failing the whole check.
        const msg = e instanceof Error ? e.message : String(e);
        if (/unknown item base type/i.test(msg) && query.query.type) {
          const inner = { ...query.query };
          delete inner.type;
          delete inner.name;
          query = { ...query, query: inner };
          search = await bridge.tradeSearch(league, query);
          setSearchNote("base not recognized by the trade site — searched by stats only");
        } else {
          throw e;
        }
      }
      const ids = search.result.slice(0, 10);
      const fetched = ids.length > 0 ? await bridge.tradeFetch(ids, search.id) : null;
      setListings(parseListings(fetched));
      setTotal(search.total);
    } catch (e) {
      setSearchError(e instanceof Error ? e.message : String(e));
    } finally {
      setSearching(false);
    }
  }

  function openTradeSite() {
    if (!built) return;
    const url = tradeSiteUrl(league, built.query);
    const bridge = getDesktopBridge();
    if (bridge) bridge.openExternal(url);
    else window.open(url, "_blank", "noopener");
  }

  const priced = (listings ?? []).filter((l) => l.amount !== null);
  const cheapest = priced[0] ?? null;
  const median = priced.length > 0 ? priced[Math.floor(priced.length / 2)] : null;
  const groups = listings ? groupListings(listings) : [];

  return (
    <>
      {/* ---- STAT FILTERS ---- */}
      <section className={`card ${styles.section}`}>
        <div className={styles.sectionHead}>
          <span className="eyebrow">Stat filters</span>
          <span className="num faint">
            {matchedCount}/{rows.length} matched
          </span>
        </div>

        <div className={styles.rows}>
          {rows.map((row, i) =>
            row.id !== null ? (
              <div key={i} className={styles.row}>
                <input
                  type="checkbox"
                  checked={row.enabled}
                  onChange={(e) => setRowEnabled(i, e.target.checked)}
                  aria-label={`Include ${row.line}`}
                />
                <span className={`${styles.rowText} r-magic`}>{row.line}</span>
                <span className={styles.boundTag}>
                  {row.stat?.better === -1 ? "max" : "min"}
                </span>
                <input
                  type="number"
                  className={styles.boundInput}
                  value={overrides[i]?.bound ?? String(row.min ?? row.max ?? "")}
                  onChange={(e) => setRowBound(i, e.target.value)}
                  disabled={!row.enabled}
                  step="0.1"
                  aria-label={`Bound for ${row.line}`}
                />
              </div>
            ) : (
              <div key={i} className={`${styles.row} ${styles.rowUnmatched}`}>
                <span className={styles.rowText}>{row.line}</span>
                <span className={`${styles.noId} faint`}>no trade id</span>
              </div>
            ),
          )}
          <div className={styles.row}>
            <input
              type="checkbox"
              checked={includeIlvl}
              onChange={(e) => setIncludeIlvl(e.target.checked)}
              aria-label="Filter by item level"
            />
            <span className={`${styles.rowText} muted`}>
              Item level ≥ <span className="num">{item.ilvl}</span>
            </span>
          </div>
        </div>

        {/* ---- ACTIONS ---- */}
        <div className={styles.actions}>
          <button
            className="btn btn-primary"
            onClick={() => void runSearch()}
            disabled={!desktop || searching || !built}
            title={
              desktop
                ? "Search the official trade site"
                : "Requires the desktop app — the trade API blocks browser requests (CORS)"
            }
          >
            {searching ? (
              <RefreshCw size={13} className={styles.spin} />
            ) : (
              <Search size={13} />
            )}
            {searching ? "Searching…" : "Search"}
          </button>
          <button
            className="btn"
            onClick={openTradeSite}
            disabled={!built}
            title="Open this search on pathofexile.com/trade2"
          >
            <ExternalLink size={13} />
            Open on trade site ▸
          </button>
        </div>
      </section>

      {/* ---- RESULTS ---- */}
      {searchError && <div className={styles.error}>{searchError}</div>}
      {searchNote && <div className={styles.note}>{searchNote}</div>}

      {listings && (
        <section className={`card ${styles.section}`}>
          <div className={styles.sectionHead}>
            <span className="eyebrow">Listings</span>
            {cheapest && median && (
              <span className={`${styles.summary} muted`}>
                cheapest <span className={styles.price}>{fmtPrice(cheapest)}</span>
                {" · "}median <span className={styles.price}>{fmtPrice(median)}</span>
                {" · "}
                <span className="num">{total}</span> total
              </span>
            )}
          </div>
          {groups.length === 0 ? (
            <p className={`${styles.note} faint`}>No listings found.</p>
          ) : (
            <table className={styles.table}>
              <thead>
                <tr>
                  <th>Price</th>
                  <th>Account</th>
                  <th>ilvl</th>
                </tr>
              </thead>
              <tbody>
                {groups.map((g, i) => (
                  <tr key={i}>
                    <td className={styles.price}>
                      {fmtPrice(g)}
                      {g.count > 1 && <span className="faint num"> ×{g.count}</span>}
                    </td>
                    <td className="muted">{g.account}</td>
                    <td className="num muted">{g.ilvl ?? "—"}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </section>
      )}
    </>
  );
}

/* ---------- panel shell -------------------------------------------------- */

function OverlayHistorySection({ desktop }: { desktop: boolean }) {
  const [entries, setEntries] = useState<OverlayMarketHistoryEntry[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    const bridge = getDesktopBridge();
    if (!desktop || !bridge) return;
    setLoading(true);
    setError(null);
    try {
      setEntries((await bridge.marketHistoryList()).slice(0, 20));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, [desktop]);

  useEffect(() => {
    if (!desktop) return;
    let alive = true;
    queueMicrotask(() => {
      if (alive) void load();
    });
    return () => {
      alive = false;
    };
  }, [desktop, load]);

  if (!desktop) return null;

  return (
    <section className={`card ${styles.section}`}>
      <div className={styles.sectionHead}>
        <span className="eyebrow">Overlay history</span>
        <button
          className="btn btn-ghost"
          onClick={() => void load()}
          disabled={loading}
          title="Reload overlay market history"
        >
          <RefreshCw size={13} className={loading ? styles.spin : undefined} />
          Refresh
        </button>
      </div>

      {error && <div className={styles.error}>{error}</div>}
      {!error && entries.length === 0 && (
        <p className={`${styles.note} faint`}>
          No overlay market checks yet. Use the in-game price-check or reward-scan hotkeys.
        </p>
      )}

      {entries.length > 0 && (
        <div className={styles.historyList}>
          {entries.map((entry) => (
            <div key={entry.id} className={styles.historyEntry}>
              <div className={styles.historyHead}>
                <span className={styles.historyTitle}>{entry.title}</span>
                <span className="tag">
                  {entry.kind === "item-price" ? "item" : "rewards"}
                </span>
              </div>
              <div className={styles.historyMeta}>
                <span>{fmtHistoryTime(entry.createdAt)}</span>
                {entry.league && <span>{entry.league}</span>}
                <span>{entry.summary}</span>
              </div>
              <div className={styles.historyRows}>
                {entry.rows.slice(0, 4).map((row, i) => (
                  <span key={`${entry.id}-${i}`} className={styles.historyRow}>
                    <span>{row.label}</span>
                    {row.value && <span className={styles.price}>{row.value}</span>}
                  </span>
                ))}
              </div>
            </div>
          ))}
        </div>
      )}
    </section>
  );
}

export function PricePanel() {
  const item = useCraft((s) => s.item);
  const league = useCraft((s) => s.league);
  const lastItemText = useCraft((s) => s.lastItemText);

  const [statsFile, setStatsFile] = useState<TradeStatsFile | null>(null);
  const [statsState, setStatsState] = useState<StatsState>("loading");
  const [desktop, setDesktop] = useState(false);

  useEffect(() => {
    let alive = true;
    void loadTradeStats().then((f) => {
      if (!alive) return;
      setStatsFile(f);
      setStatsState(f ? "ready" : "missing");
    });
    return () => {
      alive = false;
    };
  }, []);

  // Bridge detection happens post-mount so SSR markup matches hydration.
  useEffect(() => setDesktop(isDesktop()), []);

  const index = useMemo(
    () => (statsFile ? buildIndex(statsFile.stats) : null),
    [statsFile],
  );

  const extracted = useMemo(
    () => (lastItemText ? extractItemLines(lastItemText) : null),
    [lastItemText],
  );

  const baseName = item.base_display_name ?? null;
  const displayName =
    (item.rarity === "unique" || item.rarity === "rare" ? extracted?.name : null) ??
    baseName ??
    humanizeId(item.base);

  // The unique name line leads `lines` so the builder can pin `query.name`.
  const lines = useMemo(() => {
    if (!extracted) return [];
    return item.rarity === "unique" && extracted.name
      ? [extracted.name, ...extracted.lines]
      : extracted.lines;
  }, [extracted, item.rarity]);

  return (
    <div className="pane">
      <div className="pane-head">
        <div className="pane-title">Price Check</div>
        <span className="chip num">{league}</span>
      </div>

      <div className="pane-scroll">
        <div className={styles.stack}>
          {/* ---- CURRENT ITEM ---- */}
          <section className={`card ${styles.itemHead}`}>
            <BaseIcon
              baseId={item.base_type_id ?? item.base}
              name={displayName}
              rarity={item.rarity}
              size={44}
            />
            <div className={styles.itemMeta}>
              <span className={`${styles.itemName} ${rarityClass(item.rarity)}`}>
                {displayName}
              </span>
              <span className="num faint">ilvl {item.ilvl}</span>
            </div>
          </section>

          {statsState === "missing" && (
            <section className={`card ${styles.section}`}>
              <span className="eyebrow">Trade stats</span>
              <p className={`${styles.note} muted`}>
                <code>trade-stats.json</code> is not present — the price check
                cannot map stat lines to trade ids. Run:
              </p>
              <span className={`${styles.cmd} num`}>
                cargo run -p poc2-pipeline --bin fetch-trade-stats
              </span>
            </section>
          )}

          {statsState === "ready" && !lastItemText && (
            <section className={`card ${styles.section}`}>
              <span className="eyebrow">No item</span>
              <p className={`${styles.note} muted`}>
                Import an item first — paste its clipboard text in the Item pane
                (or capture from the game in the desktop app). The price check
                works from the raw item text.
              </p>
            </section>
          )}

          {statsState === "ready" && lastItemText && index && (
            // Keyed by the import so row tweaks + results reset per item.
            <SearchSection
              key={lastItemText}
              index={index}
              lines={lines}
              item={item}
              baseName={baseName}
              league={league}
              desktop={desktop}
            />
          )}

          <OverlayHistorySection desktop={desktop} />
        </div>
      </div>
    </div>
  );
}
