/// Shortest-unique-fragment search over a corpus of mod text lines.
///
/// The in-game search box matches case-insensitive regex-lite patterns
/// against an item's tooltip lines. To reference ONE mod without false
/// positives we find the shortest fragment of its display line that no
/// other line in the pool can contain ("hains$" ⇒ Temporal Chains).
///
/// Lines come from the engine's `text_template`s via `modLines()`
/// ("+# to maximum Life") where `#` marks the rolled number. Fragments
/// are digit-free and never span a `#`, so a fragment can only match
/// inside literal text — matching a candidate against another line
/// reduces to matching against that line's literal segments, which makes
/// the uniqueness check exact regardless of what number is rolled.
///
/// Clean-room implementation (poe2.re precomputes equivalents offline;
/// it is unlicensed, so nothing was copied — we compute at runtime
/// against the live bundle pool instead).

/** Regex-escape a literal fragment for the emitted pattern. */
export function escapeFragment(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

/** Normalize a display line for matching (the game is case-insensitive). */
export function normalizeLine(line: string): string {
  return line.toLowerCase().replace(/\s+/g, " ").trim();
}

/** A corpus line split into literal segments around `#` roll markers. */
export interface CorpusLine {
  /** Normalized full line (with `#` markers). */
  line: string;
  /** Literal segments (digit-positions removed). */
  segments: string[];
}

export function toCorpusLine(raw: string): CorpusLine {
  const line = normalizeLine(raw);
  return { line, segments: line.split("#").filter((s) => s.length > 0) };
}

/** True when `fragment` (plain lowercase text) occurs in `line`'s literal segments. */
function lineContains(l: CorpusLine, fragment: string): boolean {
  return l.segments.some((seg) => seg.includes(fragment));
}

interface Candidate {
  /** Emitted pattern (escaped, possibly anchored). */
  pattern: string;
  /** Raw fragment text. */
  text: string;
  anchoredStart: boolean;
  anchoredEnd: boolean;
}

/** All fragments of `l` of length `len`, with `^`/`$` anchored variants. */
function candidatesOfLength(l: CorpusLine, len: number, noStartAnchor: boolean): Candidate[] {
  const out: Candidate[] = [];
  const first = l.segments[0] ?? "";
  const last = l.segments[l.segments.length - 1] ?? "";
  const lineStartsLiteral = !noStartAnchor && l.line.length > 0 && !l.line.startsWith("#");
  const lineEndsLiteral = l.line.length > 0 && !l.line.endsWith("#");

  for (const seg of l.segments) {
    for (let i = 0; i + len <= seg.length; i++) {
      const text = seg.slice(i, i + len);
      // Digit-free fragments only: keeps the uniqueness check exact
      // (numbers roll per item and would break segment-based matching).
      if (/\d/.test(text)) continue;
      out.push({ pattern: escapeFragment(text), text, anchoredStart: false, anchoredEnd: false });
    }
  }
  // Anchored variants (count 1 extra char of budget each, huge win for
  // uniqueness: many lines share interiors but few share ends).
  if (lineStartsLiteral && first.length >= len) {
    const text = first.slice(0, len);
    if (!/\d/.test(text)) {
      out.push({ pattern: `^${escapeFragment(text)}`, text, anchoredStart: true, anchoredEnd: false });
    }
  }
  if (lineEndsLiteral && last.length >= len) {
    const text = last.slice(last.length - len);
    if (!/\d/.test(text)) {
      out.push({ pattern: `${escapeFragment(text)}$`, text, anchoredStart: false, anchoredEnd: true });
    }
  }
  return out;
}

/** Does `c` match corpus line `l` (segment-contained + anchor rules)? */
function matches(c: Candidate, l: CorpusLine): boolean {
  if (c.anchoredStart) {
    return !l.line.startsWith("#") && (l.segments[0] ?? "").startsWith(c.text);
  }
  if (c.anchoredEnd) {
    return !l.line.endsWith("#") && (l.segments[l.segments.length - 1] ?? "").endsWith(c.text);
  }
  return lineContains(l, c.text);
}

export interface UniqueFragmentOptions {
  /** Max fragment length to try before giving up (budget guard). */
  maxLen?: number;
  /** Disallow `^`-anchored candidates (the caller prepends a value
   * pattern, so the fragment sits mid-line on the real item). */
  noStartAnchor?: boolean;
}

/**
 * Shortest emitted pattern that matches EVERY line in `targets` and NO
 * line in `others`. Candidates are enumerated from the first target and
 * filtered to those all targets contain. Returns `null` when no such
 * fragment exists (caller falls back to per-target fragments).
 */
export function coverFragment(
  targets: CorpusLine[],
  others: CorpusLine[],
  opts: UniqueFragmentOptions = {},
): string | null {
  if (targets.length === 0) return null;
  const maxLen = opts.maxLen ?? 40;
  const source = targets[0];

  for (let len = 2; len <= maxLen; len++) {
    // Order: prefer unanchored (candidatesOfLength appends anchored after,
    // and equal emitted length ties resolve by iteration order).
    const cands = candidatesOfLength(source, len, opts.noStartAnchor ?? false)
      .sort((a, b) => a.pattern.length - b.pattern.length);
    for (const c of cands) {
      if (!targets.every((t) => matches(c, t))) continue;
      if (others.some((o) => matches(c, o))) continue;
      return c.pattern;
    }
  }
  return null;
}

/**
 * One pattern per target line, each unique against `others` ∪ the other
 * targets' NON-shared text — practical fallback when no single fragment
 * covers all targets. Targets that yield no unique fragment fall back
 * to their longest literal segment (escaped), which may over-match; the
 * caller surfaces that as a warning.
 */
export function perLineFragments(
  targets: CorpusLine[],
  others: CorpusLine[],
  opts: UniqueFragmentOptions = {},
): { pattern: string; exact: boolean }[] {
  return targets.map((t) => {
    const rest = others;
    const frag = coverFragment([t], rest, opts);
    if (frag !== null) return { pattern: frag, exact: true };
    const longest = [...t.segments].sort((a, b) => b.length - a.length)[0] ?? t.line;
    return { pattern: escapeFragment(longest.trim()), exact: false };
  });
}

/**
 * Convenience: patterns selecting `targets` out of a full `pool`
 * (targets are excluded from the "others" side automatically; duplicate
 * normalized lines are deduped). Tries a single covering fragment first,
 * then falls back to an OR of per-line fragments.
 */
export function selectPatterns(
  targetLines: string[],
  poolLines: string[],
  opts: UniqueFragmentOptions = {},
): { patterns: string[]; exact: boolean } {
  const targetSet = new Set(targetLines.map(normalizeLine));
  const targets = [...targetSet].map(toCorpusLine);
  const others = [...new Set(poolLines.map(normalizeLine))]
    .filter((l) => !targetSet.has(l))
    .map(toCorpusLine);

  if (targets.length === 0) return { patterns: [], exact: true };

  const cover = coverFragment(targets, others, opts);
  if (cover !== null) return { patterns: [cover], exact: true };

  const per = perLineFragments(targets, others, opts);
  return { patterns: per.map((p) => p.pattern), exact: per.every((p) => p.exact) };
}
