"use client";

/// Pure builder for official trade2 search queries. Takes the imported item's
/// stat lines + the matcher index and produces the POST body the API expects
/// (shape verified against Exiled-Exchange-2) plus per-line rows the Price
/// panel renders. No fetching here — the desktop bridge / deep link transport
/// the query.

import { matchLine, type IndexedStat, type TradeStat } from "./statIndex";

export interface TradeStatFilter {
  id: string;
  value?: { min?: number; max?: number };
  disabled: boolean;
}

export interface TradeQueryBody {
  query: {
    status: { option: string };
    /** Item name — set for uniques only. */
    name?: string;
    /** Base type, when the base resolved. */
    type?: string;
    stats: { type: string; filters: TradeStatFilter[] }[];
    filters?: { misc_filters: { filters: { ilvl: { min: number } } } };
  };
  sort: { price: string };
}

/** One input line's match outcome — drives a Price-panel row. */
export interface MatchedRow {
  /** The raw line (tags intact) — what the UI displays. */
  line: string;
  stat: TradeStat | null;
  /** Chosen trade id, or `null` when the line matched nothing. */
  id: string | null;
  /** Representative roll (average of the line's numbers). */
  value: number | null;
  /** Derived lower bound (better = 1). */
  min: number | null;
  /** Derived upper bound (better = -1). */
  max: number | null;
  enabled: boolean;
}

export interface TradeQueryInput {
  lines: string[];
  baseName: string | null;
  rarity: string;
  ilvl: number;
  league: string;
  index: Map<string, IndexedStat[]>;
  /** Row indices to include; absent ⇒ every matched row. */
  enabled?: Set<number>;
  /** Lower-bound looseness for better=1 stats (default 0.9 ⇒ 90% of roll). */
  minFactor?: number;
  /** Add the `ilvl ≥ input.ilvl` misc filter (UI row, default off). */
  includeIlvl?: boolean;
  /** Per-row user overrides for the derived bound (row index → bound). */
  bounds?: ReadonlyMap<number, number>;
}

export interface BuiltTradeQuery {
  query: TradeQueryBody;
  rows: MatchedRow[];
}

// Stat-id bucket preference: a mod is usually listed under its explicit id;
// pseudo ids aggregate across sources, the rest are niche placements.
const BUCKET_PRIORITY = ["explicit", "pseudo", "implicit", "fractured", "rune", "enchant"];

/** First id of the first non-empty bucket, in priority order. */
export function chooseStatId(ids: Record<string, string[]>): string | null {
  for (const bucket of BUCKET_PRIORITY) {
    const list = ids[bucket];
    if (list && list.length > 0) return list[0];
  }
  return null;
}

/** Round a derived bound to one decimal (floor for mins, ceil for maxes). */
function floor1(v: number): number {
  return Math.floor(v * 10) / 10;
}
function ceil1(v: number): number {
  return Math.ceil(v * 10) / 10;
}

export function buildTradeQuery(input: TradeQueryInput): BuiltTradeQuery {
  const minFactor = input.minFactor ?? 0.9;

  // For uniques the clipboard name line leads `lines`; it pins `query.name`
  // (it never matches a stat template) instead of becoming a row.
  let name: string | null = null;
  let lines = input.lines;
  if (input.rarity === "unique" && lines.length > 0 && !matchLine(input.index, lines[0])) {
    name = lines[0];
    lines = lines.slice(1);
  }

  const rows: MatchedRow[] = lines.map((line, i) => {
    const m = matchLine(input.index, line);
    const id = m ? chooseStatId(m.stat.ids) : null;
    if (!m || !id) {
      return { line, stat: m?.stat ?? null, id: null, value: null, min: null, max: null, enabled: false };
    }
    const value = m.values.length
      ? m.values.reduce((a, b) => a + b, 0) / m.values.length
      : null;
    const bound = input.bounds?.get(i);
    let min: number | null = null;
    let max: number | null = null;
    if (m.stat.better === -1) {
      // Lower is better: cap at 110% of the roll instead of flooring.
      max = bound ?? (value !== null ? ceil1(value * 1.1) : null);
    } else {
      min = bound ?? (value !== null ? floor1(value * minFactor) : null);
    }
    const enabled = input.enabled ? input.enabled.has(i) : true;
    return { line, stat: m.stat, id, value, min, max, enabled };
  });

  const filters: TradeStatFilter[] = rows
    .filter((r) => r.enabled && r.id !== null)
    .map((r) => ({
      id: r.id as string,
      ...(r.min !== null
        ? { value: { min: r.min } }
        : r.max !== null
          ? { value: { max: r.max } }
          : {}),
      disabled: false,
    }));

  const query: TradeQueryBody = {
    query: {
      status: { option: "online" },
      ...(name !== null ? { name } : {}),
      ...(input.baseName !== null ? { type: input.baseName } : {}),
      stats: [{ type: "and", filters }],
      ...(input.includeIlvl
        ? { filters: { misc_filters: { filters: { ilvl: { min: input.ilvl } } } } }
        : {}),
    },
    sort: { price: "asc" },
  };

  return { query, rows };
}

/** Deep link to the trade website (the `poe2` realm segment exists only in
 * website URLs, not in the API path). */
export function tradeSiteUrl(league: string, query: TradeQueryBody): string {
  const q = encodeURIComponent(JSON.stringify(query));
  return `https://www.pathofexile.com/trade2/search/poe2/${encodeURIComponent(league)}?q=${q}`;
}

// ---- clipboard-text extraction -----------------------------------------

export interface ExtractedItemText {
  /** Header name line (the unique/rare name; the base for magic/normal). */
  name: string | null;
  /** Header base line, when the name and base are separate lines. */
  baseName: string | null;
  /** Candidate stat lines (separators / properties / markers dropped). */
  lines: string[];
}

// Known `Label: value` property labels — kept in sync with the Rust parser's
// `is_item_property_line` (crates/parser/src/text.rs), plus requirement
// attributes which appear line-per-line in clipboard text.
const PROPERTY_LABELS = new Set([
  "Quality",
  "Armour",
  "Evasion",
  "Evasion Rating",
  "Energy Shield",
  "Ward",
  "Spirit",
  "Block",
  "Block chance",
  "Physical Damage",
  "Elemental Damage",
  "Chaos Damage",
  "Critical Hit Chance",
  "Critical Strike Chance",
  "Attacks per Second",
  "Weapon Range",
  "Item Level",
  "Requires",
  "Requirements",
  "Sockets",
  "Rune Sockets",
  "Stack Size",
  "Level",
  "Experience",
  "Limited to",
  "Radius",
  "Reload Time",
  "Grants Skill",
  "Grants",
  "Charm Slots",
  "Duration",
  "Charges",
  "Note",
  "Item Class",
  "Rarity",
  "Str",
  "Dex",
  "Int",
]);

// State markers that trail the listing — never stat lines.
const MARKERS = new Set(["Corrupted", "Mirrored", "Sanctified", "Unidentified"]);

const SEPARATOR_RE = /^-{4,}$/;

function isPropertyLine(line: string): boolean {
  const colon = line.indexOf(":");
  return colon > 0 && PROPERTY_LABELS.has(line.slice(0, colon));
}

/** Split raw PoE2 clipboard text into the header names + candidate stat
 * lines. Flavour text survives (it simply won't match any trade id). */
export function extractItemLines(text: string): ExtractedItemText {
  const all = text.split(/\r?\n/).map((l) => l.trim());
  const firstSep = all.findIndex((l) => SEPARATOR_RE.test(l));
  const header = firstSep === -1 ? all : all.slice(0, firstSep);

  // Header: "Item Class: …", "Rarity: …", then name (+ base for rare/unique).
  const rarityIdx = header.findIndex((l) => l.startsWith("Rarity:"));
  const named = header.slice(rarityIdx + 1).filter((l) => l.length > 0);
  const name = named[0] ?? null;
  const baseName = named[1] ?? null;

  const body = firstSep === -1 ? [] : all.slice(firstSep + 1);
  const lines = body.filter(
    (l) =>
      l.length > 0 && !SEPARATOR_RE.test(l) && !isPropertyLine(l) && !MARKERS.has(l),
  );
  return { name, baseName, lines };
}
