// Pure helpers for the Evaluate price-check card (the PoE-Overlay-style rich
// card rendered by the hyproverlay Hyprland plugin). Everything here is
// side-effect-free and unit-tested — no engine worker, no fetch, no DOM — so
// the payload builder in `market.ts` stays deterministic and cheap.
//
// Two jobs:
//   1. Mod display: turn the captured item's stat lines into per-mod
//      { badge, text, score } triples. Tier badges (P2 / S3) and roll% come
//      from the "Advanced Mod Descriptions" clipboard format when present;
//      basic-format captures degrade to a bulletless mod line with no score.
//   2. Estimated value: aggregate the priced trade listings into a single
//      headline (`≈ 249.8 div`), a plausible range, and a reliability label.

// ---- item info -------------------------------------------------------------

export interface ItemInfo {
  rarity: string;
  ilvl: number | null;
  requiresLevel: number | null;
}

/** Pull the header facts the card shows above the mods (rarity / ilvl / level). */
export function parseItemInfo(text: string): ItemInfo {
  const lines = text.split(/\r?\n/).map((l) => l.trim());
  let rarity = "normal";
  let ilvl: number | null = null;
  let requiresLevel: number | null = null;
  for (const line of lines) {
    if (line.startsWith("Rarity:")) {
      rarity = line.slice("Rarity:".length).trim().toLowerCase() || "normal";
    } else if (/^Item Level:\s*\d+/.test(line)) {
      const m = line.match(/(\d+)/);
      if (m) ilvl = Number(m[1]);
    } else if (/^(Requires|Level):/i.test(line) || /^Requires\b/i.test(line)) {
      // "Requires Level 65" or "Level: 65" or "Requires: Level 65, 111 Str".
      const m = line.match(/Level[:\s]+(\d+)/i);
      if (m) requiresLevel = Number(m[1]);
    }
  }
  return { rarity, ilvl, requiresLevel };
}

// ---- mods ------------------------------------------------------------------

export interface EvaluatedMod {
  /** Tier badge, e.g. "P2" / "S3" / "" (unknown affix/tier). */
  badge: string;
  /** The human mod line (rolls intact), e.g. "38% total Elemental Resistance". */
  text: string;
  /** Roll quality within tier range in `[0,1]`, or null when unknown. */
  roll: number | null;
}

interface AdvancedMod {
  affix: "prefix" | "suffix" | "implicit";
  tier: number | null;
  lines: string[];
  rolls: Array<{ value: number; min: number; max: number }>;
}

// `{ Prefix Modifier "Glacial" (Tier: 2) — Cold }` (em dash or hyphen).
// Mirrors crates/parser's advanced-header grammar, precision-first: match the
// affix kind, then read the tier separately so the optional `(Tier: N)` group
// can't be skipped by lazy backtracking.
const ADV_HEADER = /^\{\s*(Prefix|Suffix|Implicit)\b[^}]*\}$/i;
const ADV_TIER = /\(Tier:\s*(\d+)\)/i;
// Rolled value with its tier range: `90(80-91)` / `1.5(1-2)` / `-12(-20--5)`.
const ROLL_RE = /(-?\d+(?:\.\d+)?)\((-?\d+(?:\.\d+)?)-(-?\d+(?:\.\d+)?)\)/g;

function parseRolls(line: string): Array<{ value: number; min: number; max: number }> {
  const out: Array<{ value: number; min: number; max: number }> = [];
  for (const m of line.matchAll(ROLL_RE)) {
    const value = Number(m[1]);
    const min = Number(m[2]);
    const max = Number(m[3]);
    if (Number.isFinite(value) && Number.isFinite(min) && Number.isFinite(max)) {
      out.push({ value, min, max });
    }
  }
  return out;
}

/** Strip the `(min-max)` range annotations, leaving the readable roll text. */
export function cleanModText(line: string): string {
  return line.replace(ROLL_RE, "$1").replace(/\s{2,}/g, " ").trim();
}

/** Roll quality of a mod: mean of each stat's position within its tier range. */
export function rollPercent(
  rolls: Array<{ value: number; min: number; max: number }>,
): number | null {
  if (rolls.length === 0) return null;
  let sum = 0;
  let n = 0;
  for (const r of rolls) {
    const span = r.max - r.min;
    if (span <= 0) {
      // Single-value tier: a present roll is a full roll.
      sum += 1;
    } else {
      sum += Math.min(1, Math.max(0, (r.value - r.min) / span));
    }
    n += 1;
  }
  return n === 0 ? null : sum / n;
}

/** Badge letter+ordinal from affix kind and tier, e.g. prefix/2 → "P2". */
export function tierBadge(affix: AdvancedMod["affix"], tier: number | null): string {
  const letter = affix === "prefix" ? "P" : affix === "suffix" ? "S" : "I";
  return tier === null ? letter : `${letter}${tier}`;
}

/**
 * Parse the advanced-format explicit block into typed mods. Returns `null`
 * when the text is not advanced format (no `{ … Modifier … }` headers), so
 * callers can fall back to plain stat lines.
 */
function parseAdvancedMods(text: string): AdvancedMod[] | null {
  const all = text.split(/\r?\n/).map((l) => l.trim());
  const mods: AdvancedMod[] = [];
  let current: AdvancedMod | null = null;
  let sawHeader = false;
  for (const line of all) {
    const header = line.match(ADV_HEADER);
    if (header) {
      sawHeader = true;
      if (current) mods.push(current);
      const tierMatch = line.match(ADV_TIER);
      current = {
        affix: header[1].toLowerCase() as AdvancedMod["affix"],
        tier: tierMatch ? Number(tierMatch[1]) : null,
        lines: [],
        rolls: [],
      };
      continue;
    }
    if (!current) continue;
    if (line.length === 0 || /^-{4,}$/.test(line)) {
      mods.push(current);
      current = null;
      continue;
    }
    current.lines.push(line);
    current.rolls.push(...parseRolls(line));
  }
  if (current) mods.push(current);
  if (!sawHeader) return null;
  // Implicit affixes are shown separately in-game; keep prefixes/suffixes here.
  return mods.filter((m) => m.affix !== "implicit" && m.lines.length > 0);
}

/**
 * Best-effort mod list for the card. Advanced format yields tier badges + roll
 * scores; basic format (or unparseable) yields the given plain stat lines with
 * empty badges/scores. `plainLines` is the already-extracted stat-line list.
 */
export function evaluateMods(text: string, plainLines: string[]): EvaluatedMod[] {
  const advanced = parseAdvancedMods(text);
  if (advanced && advanced.length > 0) {
    return advanced.map((mod) => ({
      badge: tierBadge(mod.affix, mod.tier),
      text: cleanModText(mod.lines.join(" ")),
      roll: rollPercent(mod.rolls),
    }));
  }
  return plainLines
    .filter((line) => !isNonModLine(line))
    .map((line) => ({
      badge: "",
      text: cleanModText(line),
      roll: null,
    }));
}

// Requirement / property lines that slip through basic-format stat extraction
// (e.g. "Requires Level 68" has no colon so it isn't caught upstream). These
// are never mods and must not appear in the Evaluate mod list.
const NON_MOD_RE =
  /^(Requires|Requirements|Item Level|Quality|Sockets|Rune Sockets|Level|Str|Dex|Int|Corrupted|Mirrored|Unidentified|Sanctified)\b/i;

function isNonModLine(line: string): boolean {
  return NON_MOD_RE.test(line.trim());
}

// ---- estimated value -------------------------------------------------------

export interface PricedListing {
  amount: number;
  currency: string | null;
}

export interface ValueEstimate {
  approx: number;
  unit: string;
  low: number;
  high: number;
  reliability: "High" | "Moderate" | "Low";
  count: number;
}

function quantile(sorted: number[], q: number): number {
  if (sorted.length === 0) return 0;
  if (sorted.length === 1) return sorted[0];
  const pos = (sorted.length - 1) * q;
  const base = Math.floor(pos);
  const rest = pos - base;
  const next = sorted[base + 1] ?? sorted[base];
  return sorted[base] + rest * (next - sorted[base]);
}

function round1(v: number): number {
  return Math.round(v * 10) / 10;
}

/**
 * Aggregate priced listings into a single estimate. Uses the dominant currency
 * (by count), trims to that currency, and takes the lower-quartile-weighted
 * central value (sellers over-list; the cheap end is the real market). Range is
 * p10–p90; reliability grows with listing count and shrinks with spread.
 */
export function estimateValue(listings: PricedListing[]): ValueEstimate | null {
  const priced = listings.filter((l) => Number.isFinite(l.amount) && l.amount > 0);
  if (priced.length === 0) return null;

  // Pick the most common currency so we never mix div and ex in one number.
  const byCurrency = new Map<string, number[]>();
  for (const l of priced) {
    const key = l.currency ?? "";
    const arr = byCurrency.get(key) ?? [];
    arr.push(l.amount);
    byCurrency.set(key, arr);
  }
  let unit = "";
  let amounts: number[] = [];
  for (const [key, arr] of byCurrency) {
    if (arr.length > amounts.length) {
      unit = key;
      amounts = arr;
    }
  }
  amounts = [...amounts].sort((a, b) => a - b);

  const low = quantile(amounts, 0.1);
  const high = quantile(amounts, 0.9);
  // Central value biased toward the cheap end (p35): matches how the community
  // reads "what it's worth" from a sorted listing wall.
  const approx = quantile(amounts, 0.35);

  const median = quantile(amounts, 0.5) || 1;
  const spread = median > 0 ? (high - low) / median : 1;
  let reliability: ValueEstimate["reliability"];
  if (amounts.length >= 8 && spread <= 1.2) reliability = "High";
  else if (amounts.length >= 4 && spread <= 2.5) reliability = "Moderate";
  else reliability = "Low";

  return {
    approx: round1(approx),
    unit,
    low: round1(low),
    high: round1(high),
    reliability,
    count: amounts.length,
  };
}

/** Compact currency label: "div"/"ex" short forms, else the raw currency. */
export function shortCurrency(currency: string | null): string {
  if (!currency) return "";
  const c = currency.toLowerCase();
  if (c === "divine" || c === "div") return "div";
  if (c === "exalted" || c === "exalt" || c === "ex") return "ex";
  if (c === "chaos") return "chaos";
  return currency;
}
