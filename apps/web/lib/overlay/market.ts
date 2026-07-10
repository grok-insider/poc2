import type {
  HyprOverlayPayload,
  OverlayMarketHistoryEntry,
  Poc2DesktopBridge,
} from "@/lib/desktop";
import {
  buildTradeQuery,
  extractItemLines,
  type BuiltTradeQuery,
} from "@/lib/trade/queryBuilder";
import { buildIndex, loadTradeStats } from "@/lib/trade/statIndex";
import {
  estimateValue,
  evaluateMods,
  parseItemInfo,
  shortCurrency,
  type EvaluatedMod,
  type ValueEstimate,
} from "@/lib/overlay/evaluate";

// Card palette (hex+alpha) — game-exact bronze/gold/rarity per DESIGN.md. The
// hyproverlay `style` block sets the base three tones; these are the per-row /
// per-cell overrides the Evaluate layout needs.
const COL = {
  fg: "#e8e0d2ff",
  muted: "#a89a85ff",
  faint: "#6f6354ff",
  accent: "#d29933ff",
  gold: "#e7b478ff",
  magic: "#8888ffff",
  rare: "#ffff77ff",
  headerBg: "#2a1609cc",
  plaqueBg: "#d2993321",
  badgeBg: "#d2993329",
  listHeadBg: "#ffffff0a",
  modBg: "#ffffff08",
} as const;

interface Listing {
  account: string;
  amount: number | null;
  currency: string | null;
  ilvl: number | null;
  quality: number | null;
  indexed: string | null;
}

interface ListingGroup extends Listing {
  count: number;
}

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
        indexed?: unknown;
      };
      item?: { ilvl?: unknown; properties?: unknown };
    };
    out.push({
      account: typeof r.listing?.account?.name === "string" ? r.listing.account.name : "-",
      amount: typeof r.listing?.price?.amount === "number" ? r.listing.price.amount : null,
      currency:
        typeof r.listing?.price?.currency === "string" ? r.listing.price.currency : null,
      ilvl: typeof r.item?.ilvl === "number" ? r.item.ilvl : null,
      quality: parseQuality(r.item?.properties),
      indexed: typeof r.listing?.indexed === "string" ? r.listing.indexed : null,
    });
  }
  return out;
}

function parseQuality(properties: unknown): number | null {
  if (!Array.isArray(properties)) return null;
  for (const raw of properties) {
    if (!raw || typeof raw !== "object") continue;
    const prop = raw as { name?: unknown; values?: unknown };
    if (typeof prop.name !== "string" || !/quality/i.test(prop.name)) continue;
    const values = prop.values;
    if (!Array.isArray(values)) continue;
    const first = values[0];
    const text = Array.isArray(first) && typeof first[0] === "string" ? first[0] : null;
    const match = text?.match(/([+-]?\d+)/);
    if (match) return Number(match[1]);
  }
  return null;
}

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

function fmtListing(l: Listing): string {
  if (l.amount === null) return "no price";
  return `${l.amount} ${l.currency ?? ""}`.trim();
}

function fmtAge(indexed: string | null): string {
  if (!indexed) return "-";
  const time = Date.parse(indexed);
  if (!Number.isFinite(time)) return "-";
  const minutes = Math.max(1, Math.floor((Date.now() - time) / 60_000));
  if (minutes < 60) return `${minutes}m`;
  const hours = Math.floor(minutes / 60);
  if (hours < 48) return `${hours}h`;
  return `${Math.floor(hours / 24)}d`;
}

function parseRarity(text: string): string {
  const line = text.split(/\r?\n/).find((l) => l.startsWith("Rarity:"));
  return (line?.slice("Rarity:".length).trim().toLowerCase() || "normal");
}

function parseIlvl(text: string): number {
  const match = text.match(/^Item Level:\s*(\d+)/m);
  return match ? Number(match[1]) : 0;
}

function defaultRect(): { x: number; y: number; width: number; height: number } {
  const width = 460;
  const height = 260;
  if (typeof window === "undefined") return { x: 80, y: 80, width, height };
  return {
    x: Math.max(12, window.screen.width - width - 36),
    y: 80,
    width,
    height,
  };
}

function evaluateRect(): { x: number; y: number; width: number; height: number } {
  const width = 460;
  const height = 560;
  if (typeof window === "undefined") return { x: 80, y: 80, width, height };
  return {
    x: Math.max(12, window.screen.width - width - 36),
    y: 80,
    width,
    height,
  };
}

const POE_STYLE: NonNullable<HyprOverlayPayload["style"]> = {
  font: "Fontin",
  fontSize: 13,
  radius: 3,
  padding: 10,
  gap: 4,
  background: "#080604ec",
  border: "#b2915566",
  text: "#e8e0d2ff",
  muted: "#a89a85ff",
  accent: "#d29933ff",
};

type Row = NonNullable<HyprOverlayPayload["rows"]>[number];

const RARITY_HEX: Record<string, string> = {
  normal: COL.fg,
  magic: COL.magic,
  rare: COL.rare,
  unique: "#ef6916ff",
  currency: "#aa9e82ff",
};

function rarityColor(rarity: string): string {
  return RARITY_HEX[rarity] ?? COL.fg;
}

/** Capitalize a rarity word for the info row ("rare" → "Rare"). */
function titleCase(s: string): string {
  return s ? s[0].toUpperCase() + s.slice(1) : s;
}

/** Two-column label/value info row (e.g. Item Level · 80). */
function infoRow(label: string, value: string, valueColor: string = COL.fg): Row {
  return {
    kind: "columns",
    cells: [
      { text: label, color: COL.muted, weight: 2 },
      { text: value, color: valueColor, align: "right", weight: 1.4 },
    ],
  };
}

/** One mod line: [tier badge] [mod text (magic blue)] [roll% score]. */
function modRow(mod: EvaluatedMod): Row {
  const badge = mod.badge
    ? { text: mod.badge, color: COL.accent, bg: COL.badgeBg, align: "center" as const, weight: 1 }
    : { text: "•", color: COL.faint, align: "center" as const, weight: 1 };
  const cells: NonNullable<Row["cells"]> = [
    badge,
    { text: mod.text, color: COL.magic, weight: 6 },
  ];
  if (mod.roll !== null) {
    cells.push({
      text: `${Math.round(mod.roll * 100)}%`,
      color: COL.fg,
      align: "right",
      weight: 1.4,
    });
  } else {
    cells.push({ text: "", weight: 1.4 });
  }
  return { kind: "columns", bg: COL.modBg, cells };
}

function estimatePlaque(est: ValueEstimate): Row[] {
  const unit = shortCurrency(est.unit);
  const rangeUnit = unit ? ` ${unit}` : "";
  return [
    { kind: "header", label: "Estimated Value", color: COL.fg, bg: COL.plaqueBg, size: 14 },
    {
      label: `≈ ${est.approx}${unit ? ` ${unit}` : ""}`,
      value: est.reliability,
      detail: `range ${est.low}–${est.high}${rangeUnit}`,
      color: COL.gold,
      valueColor: est.reliability === "High" ? "#46a239ff" : est.reliability === "Low" ? "#a89a85ff" : COL.accent,
      emphasis: true,
      size: 16,
    },
  ];
}

/** The full PoE-Overlay-style Evaluate card. */
function richPricePayload(
  itemText: string,
  title: string,
  statLines: string[],
  opts: {
    cheapest: Listing | null;
    median: Listing | null;
    matched: number;
    totalRows: number;
    totalResults: number;
    shown: number;
    listings: Listing[];
    estimate: ValueEstimate | null;
  },
): HyprOverlayPayload {
  const rect = evaluateRect();
  const info = parseItemInfo(itemText);
  const mods = evaluateMods(itemText, statLines).slice(0, 12);

  const rows: NonNullable<HyprOverlayPayload["rows"]> = [
    { kind: "header", label: title, color: rarityColor(info.rarity), bg: COL.headerBg, size: 16 },
    infoRow("Item Rarity", titleCase(info.rarity), rarityColor(info.rarity)),
  ];
  if (info.ilvl !== null) rows.push(infoRow("Item Level", String(info.ilvl)));
  if (info.requiresLevel !== null) rows.push(infoRow("Requires Level", String(info.requiresLevel)));

  if (mods.length > 0) {
    rows.push({ kind: "separator" });
    for (const mod of mods) rows.push(modRow(mod));
  }

  rows.push({ kind: "separator" });
  if (opts.estimate) {
    rows.push(...estimatePlaque(opts.estimate));
  } else {
    rows.push({
      label: "Cheapest",
      value: opts.cheapest ? fmtListing(opts.cheapest) : "none",
      detail: opts.median ? `median ${fmtListing(opts.median)}` : undefined,
      color: COL.gold,
      emphasis: true,
    });
  }
  rows.push({
    kind: "columns",
    cells: [
      { text: `${opts.shown}/${opts.totalResults}`, color: COL.muted, weight: 1 },
      { text: `matched ${opts.matched}/${opts.totalRows}`, color: COL.muted, align: "center", weight: 2 },
      { text: "pathofexile.com/trade", color: COL.accent, align: "right", weight: 3 },
    ],
  });

  rows.push({ kind: "separator" });
  rows.push({
    kind: "columns",
    bg: COL.listHeadBg,
    cells: [
      { text: "Price", color: COL.accent, weight: 1.3 },
      { text: "iLvl", color: COL.accent, align: "right", weight: 0.8 },
      { text: "Q%", color: COL.accent, align: "right", weight: 0.8 },
      { text: "Account", color: COL.accent, weight: 2 },
      { text: "Listed", color: COL.accent, align: "right", weight: 1 },
    ],
  });
  for (const listing of opts.listings.slice(0, 8)) {
    rows.push({
      kind: "columns",
      cells: [
        { text: fmtListing(listing), color: COL.fg, weight: 1.3 },
        { text: listing.ilvl === null ? "-" : String(listing.ilvl), align: "right", weight: 0.8 },
        { text: listing.quality === null ? "-" : String(listing.quality), align: "right", weight: 0.8 },
        { text: listing.account, color: COL.muted, weight: 2 },
        { text: fmtAge(listing.indexed), align: "right", weight: 1 },
      ],
    });
  }

  return {
    mode: "cards",
    visible: true,
    rect: { x: rect.x, y: rect.y, w: rect.width, h: rect.height },
    ttlMs: 20_000,
    style: POE_STYLE,
    rows,
  };
}

export function cardPayload(
  rows: NonNullable<HyprOverlayPayload["rows"]>,
  opts: { title?: string; ttlMs?: number; rect?: { x: number; y: number; width: number; height: number } } = {},
): HyprOverlayPayload {
  const rect = opts.rect ?? defaultRect();
  return {
    mode: "cards",
    visible: true,
    rect: { x: rect.x, y: rect.y, w: rect.width, h: rect.height },
    ttlMs: opts.ttlMs ?? 20_000,
    rows: opts.title ? [{ label: opts.title, emphasis: true }, ...rows] : rows,
  };
}

export function errorPayload(title: string, detail: string): HyprOverlayPayload {
  return cardPayload([{ label: detail, value: "failed" }], { title, ttlMs: 10_000 });
}

function buildItemQuery(text: string, league: string, index: ReturnType<typeof buildIndex>): {
  displayName: string;
  built: BuiltTradeQuery;
  statLines: string[];
} {
  const extracted = extractItemLines(text);
  const rarity = parseRarity(text);
  const ilvl = parseIlvl(text);
  const baseName = extracted.baseName ?? extracted.name;
  const displayName =
    rarity === "unique" || rarity === "rare"
      ? (extracted.name ?? baseName ?? "Captured item")
      : (baseName ?? extracted.name ?? "Captured item");
  const lines =
    rarity === "unique" && extracted.name
      ? [extracted.name, ...extracted.lines]
      : extracted.lines;
  const built = buildTradeQuery({
    lines,
    baseName,
    rarity,
    ilvl,
    league,
    index,
  });
  return { displayName, built, statLines: extracted.lines };
}

export async function priceCheckItemOverlay(
  bridge: Poc2DesktopBridge,
  itemText: string,
  league: string,
): Promise<{ payload: HyprOverlayPayload; history: Omit<OverlayMarketHistoryEntry, "id" | "createdAt"> | null }> {
  const statsFile = await loadTradeStats();
  if (!statsFile) {
    return { payload: errorPayload("Item Price", "trade-stats.json missing"), history: null };
  }
  const index = buildIndex(statsFile.stats);
  const { displayName, built, statLines } = buildItemQuery(itemText, league, index);
  const matched = built.rows.filter((r) => r.id !== null).length;
  const totalRows = built.rows.length;
  if (matched === 0) {
    const rows = [{ label: "No trade stat matches", value: `${totalRows} lines` }];
    return {
      payload: cardPayload(rows, { title: displayName }),
      history: { kind: "item-price", title: displayName, league, summary: "No trade stat matches", rows },
    };
  }

  let query = built.query;
  let search;
  try {
    search = await bridge.tradeSearch(league, query);
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e);
    if (/unknown item base type/i.test(msg) && query.query.type) {
      const inner = { ...query.query };
      delete inner.type;
      delete inner.name;
      query = { ...query, query: inner };
      search = await bridge.tradeSearch(league, query);
    } else {
      throw e;
    }
  }

  // trade2's fetch endpoint caps at 10 ids/call; pull up to 20 (two batches)
  // so the estimate + listings table have a real sample. The second batch is
  // best-effort — a failure there still leaves the first 10 rendered.
  const ids = search.result.slice(0, 20);
  const batches = [ids.slice(0, 10), ids.slice(10, 20)].filter((b) => b.length > 0);
  const listings: Listing[] = [];
  for (const batch of batches) {
    try {
      listings.push(...parseListings(await bridge.tradeFetch(batch, search.id)));
    } catch {
      break;
    }
  }
  const priced = listings.filter((l) => l.amount !== null);
  const cheapest = priced[0] ?? null;
  const median = priced.length > 0 ? priced[Math.floor(priced.length / 2)] : null;
  const groups = groupListings(listings).slice(0, 4);
  const estimate = estimateValue(
    priced.map((l) => ({ amount: l.amount as number, currency: l.currency })),
  );

  const historyRows: NonNullable<OverlayMarketHistoryEntry["rows"]> = [
    {
      label: "cheapest",
      value: cheapest ? fmtListing(cheapest) : "none",
      detail: median ? `median ${fmtListing(median)}` : undefined,
    },
    { label: "matched stats", value: `${matched}/${totalRows}` },
    { label: "trade results", value: String(search.total) },
    ...groups.map((g) => ({
      label: g.account,
      value: fmtListing(g),
      detail: g.count > 1 ? `x${g.count}` : g.ilvl ? `ilvl ${g.ilvl}` : undefined,
    })),
  ];
  const headline = estimate
    ? `≈ ${estimate.approx} ${shortCurrency(estimate.unit)}`.trim()
    : cheapest
      ? `Cheapest ${fmtListing(cheapest)}`
      : "none";
  const summary = `${headline} · ${search.total} listed`;
  return {
    payload: richPricePayload(itemText, displayName, statLines, {
      cheapest,
      median,
      matched,
      totalRows,
      totalResults: search.total,
      shown: listings.length,
      listings,
      estimate,
    }),
    history: { kind: "item-price", title: displayName, league, summary, rows: historyRows },
  };
}
