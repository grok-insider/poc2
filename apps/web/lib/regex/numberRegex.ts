/// Digit-pattern generators for PoE2's in-game search boxes (stash /
/// vendor / reveal search). The search accepts a regex-lite dialect —
/// `[a-b]` classes, `.`, `\d`, `|`, `(...)`, `^`/`$` — with a 250-char
/// budget, so every pattern here optimizes for LENGTH over strictness.
///
/// Matching is substring-based against the item's tooltip lines, so a
/// "number" has no word boundaries: `atLeastRegex(30)` intentionally
/// also matches inside `130` (any 3-digit number ≥ the pattern's floor
/// is a superset match we accept — the same tradeoff every community
/// regex tool makes to stay short).
///
/// Clean-room implementation (inspired by the behaviour of poe2.re,
/// which is unlicensed — no code was copied).

/** Regex matching any integer with exactly `n` digits. */
function anyDigits(n: number): string {
  return "\\d".repeat(n);
}

/** `[a-b]` (or the single digit when a === b, `\d` when 0-9). */
function digitClass(a: number, b: number): string {
  if (a === b) return `${a}`;
  if (a === 0 && b === 9) return "\\d";
  return `[${a}-${b}]`;
}

/**
 * Alternatives matching numbers with the same digit-count as `digits`
 * that are numerically >= `digits`. Standard prefix decomposition:
 * for d·rest — `d(rest≥)` plus `[d+1-9]` followed by free digits.
 */
function geSameLength(digits: string): string[] {
  const d = Number(digits[0]);
  const rest = digits.slice(1);
  if (rest.length === 0) return [digitClass(d, 9)];

  const out: string[] = [];
  const restGe = geSameLength(rest);
  // First digit equal, remainder must still be >= rest.
  if (restGe.length === 1) {
    out.push(`${d}${restGe[0]}`);
  } else {
    out.push(`${d}(${restGe.join("|")})`);
  }
  // First digit strictly greater — remainder free.
  if (d < 9) out.push(`${digitClass(d + 1, 9)}${anyDigits(rest.length)}`);
  return out;
}

/** Collapse `d\d\d…`-style alternatives where possible: if the "rest"
 * of `digits` is all zeros, `d(rest≥)` is just `d` + free digits and the
 * two branches merge into one `[d-9]\d…`. */
function geAlternatives(n: number): string[] {
  const digits = String(n);
  const rest = digits.slice(1);
  if (rest.length > 0 && /^0+$/.test(rest)) {
    return [`${digitClass(Number(digits[0]), 9)}${anyDigits(rest.length)}`];
  }
  return geSameLength(digits);
}

/**
 * Shortest-effort pattern matching any integer >= `n` (as a substring of
 * a tooltip line). Also matches longer numbers via the extra-digit arm.
 * Returns `""` for n <= 0 (no constraint) — callers must skip it.
 */
export function atLeastRegex(n: number): string {
  if (!Number.isFinite(n) || n <= 0) return "";
  const floor = Math.floor(n);
  const alts = geAlternatives(floor);
  // Any number with more digits than n is automatically >= n.
  alts.push(anyDigits(String(floor).length + 1));
  return alts.length === 1 ? alts[0] : `(${alts.join("|")})`;
}

/**
 * Compact alternation matching EXACTLY the given values (1–2 digit
 * integers), compressing shared ones-digits into tens classes:
 * `[10,20,30]` → `[1-3]0`; `[15,25]` → `[12]5`; `[10,15]` → `(10|15)`.
 * Used by the vendor tab's movement-speed style pickers.
 */
export function exactAlternation(values: number[]): string {
  const uniq = [...new Set(values.filter((v) => Number.isInteger(v) && v >= 0 && v <= 99))].sort(
    (a, b) => a - b,
  );
  if (uniq.length === 0) return "";
  if (uniq.length === 1) return String(uniq[0]);

  // Group two-digit values by their ones digit; keep single digits as-is.
  const byOnes = new Map<number, number[]>();
  const singles: string[] = [];
  for (const v of uniq) {
    if (v < 10) {
      singles.push(String(v));
      continue;
    }
    const ones = v % 10;
    const tens = Math.floor(v / 10);
    const arr = byOnes.get(ones) ?? [];
    arr.push(tens);
    byOnes.set(ones, arr);
  }

  const parts: string[] = [...singles];
  for (const [ones, tens] of [...byOnes.entries()].sort((a, b) => a[0] - b[0])) {
    if (tens.length === 1) {
      parts.push(`${tens[0]}${ones}`);
      continue;
    }
    const sorted = [...tens].sort((a, b) => a - b);
    const contiguous = sorted[sorted.length - 1] - sorted[0] === sorted.length - 1;
    const cls = contiguous && sorted.length > 2 ? `[${sorted[0]}-${sorted[sorted.length - 1]}]` : `[${sorted.join("")}]`;
    parts.push(`${cls}${ones}`);
  }
  return parts.length === 1 ? parts[0] : `(${parts.join("|")})`;
}

/**
 * Pattern for an inclusive [min, max] integer range, 0–99 only (the
 * vendor ilvl/level filters). Falls back to `atLeastRegex` when only a
 * min is usable, or `""` when unconstrained/invalid.
 *
 * NOTE: unlike `atLeastRegex` this does NOT add an extra-digit arm — a
 * range filter that silently matched 3-digit numbers would be wrong.
 */
export function rangeRegex(min: number, max: number): string {
  const lo = Math.max(0, Math.floor(min) || 0);
  const hi = Math.floor(max) || 0;
  if (lo === 0 && hi === 0) return "";
  if (hi === 0) return atLeastRegex(lo); // open-ended ">= lo"
  if (hi > 99 || hi < lo) return "";
  if (lo === hi) return String(lo);

  const parts: string[] = [];
  // Single-digit stretch.
  if (lo <= 9) parts.push(digitClass(lo, Math.min(hi, 9)));
  // Two-digit stretch, decomposed by tens.
  if (hi >= 10) {
    const a = Math.max(lo, 10);
    const at = Math.floor(a / 10);
    const ao = a % 10;
    const bt = Math.floor(hi / 10);
    const bo = hi % 10;
    if (at === bt) {
      parts.push(`${at}${digitClass(ao, bo)}`);
    } else {
      if (ao > 0) {
        parts.push(`${at}${digitClass(ao, 9)}`);
      }
      const fullLo = ao === 0 ? at : at + 1;
      const fullHi = bo === 9 ? bt : bt - 1;
      if (fullLo <= fullHi) {
        parts.push(`${digitClass(fullLo, fullHi)}\\d`);
      }
      if (bo < 9) {
        parts.push(`${bt}${digitClass(0, bo)}`);
      }
    }
  }
  return parts.length === 1 ? parts[0] : `(${parts.join("|")})`;
}
