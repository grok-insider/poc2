/// Parse OCR'd price-panel text into structured rows (ADR-0013).
///
/// The in-game price panel (and poe2scout/poe.ninja-style currency lists) is a
/// stack of `<name> ×<qty>` rows. Tesseract returns one big text blob; this
/// turns it into `{ name, quantity }[]`, rejecting UI noise and OCR garbage:
///
///   - a leading OR trailing quantity multiplier (`3x`, `x3`, `×3`, `3 ×`) is
///     pulled off into `quantity` and stripped from the name;
///   - leading non-letter noise (icon bleed: bullets, digits, punctuation) is
///     trimmed off the front of the name;
///   - rows shorter than 4 chars, or with no run of 4+ letters, are dropped
///     (these are almost always icon-column artefacts or separators).
///
/// Pure + unit-tested — no DOM, no Tesseract import.

export interface OcrRow {
  /** Cleaned item/currency name (noise + quantity stripped). */
  name: string;
  /** Stack quantity (the `Nx` multiplier); defaults to 1 when absent. */
  quantity: number;
}

/** Minimum cleaned-name length to keep a row. */
const MIN_NAME_CHARS = 4;

/** A name must contain at least one run of this many consecutive letters. */
const MIN_WORD_LETTERS = 4;

// Quantity at the END: "Chaos Orb x3", "Chaos Orb ×3", "Chaos Orb 3x", "... 3".
// Require an explicit x/× for a trailing bare number UNLESS it's clearly a
// count (handled by the leading form); a bare trailing integer is ambiguous
// (could be a roll value) so we only accept it with an x/×.
const TRAILING_QTY = /[\s]*[x×]?\s*(\d{1,4})\s*[x×]\s*$|[\s]*[x×]\s*(\d{1,4})\s*$/i;

// Quantity at the START: "3x Chaos Orb", "x3 Chaos Orb", "×3 Chaos Orb".
const LEADING_QTY = /^\s*(?:(\d{1,4})\s*[x×]|[x×]\s*(\d{1,4}))\s+/i;

/** Leading noise: bullets, stray punctuation, and digit/letter icon bleed. */
const LEADING_NOISE = /^[^\p{L}]+/u;

/** Collapse internal whitespace and normalize the unicode multiplier glyph. */
function normalizeWhitespace(s: string): string {
  return s.replace(/\s+/g, " ").trim();
}

function hasLongWord(s: string): boolean {
  return new RegExp(`[\\p{L}]{${MIN_WORD_LETTERS},}`, "u").test(s);
}

/**
 * Parse a single raw line into a row, or `null` if it's noise.
 * Exported for focused unit tests of the per-row rules.
 */
export function parseRow(raw: string): OcrRow | null {
  let line = normalizeWhitespace(raw);
  if (!line) return null;

  let quantity = 1;

  // Leading multiplier first (so "3x Foo" doesn't get mis-stripped as noise).
  const lead = LEADING_QTY.exec(line);
  if (lead) {
    quantity = Number(lead[1] ?? lead[2]);
    line = line.slice(lead[0].length);
  } else {
    // Trailing multiplier ("Foo x3" / "Foo 3x").
    const tail = TRAILING_QTY.exec(line);
    if (tail) {
      quantity = Number(tail[1] ?? tail[2]);
      line = line.slice(0, tail.index);
    }
  }

  // Strip leading non-letter noise (icon bleed, bullets, separators).
  line = line.replace(LEADING_NOISE, "");
  line = normalizeWhitespace(line);

  if (line.length < MIN_NAME_CHARS) return null;
  if (!hasLongWord(line)) return null;
  if (!Number.isFinite(quantity) || quantity < 1) quantity = 1;

  return { name: line, quantity };
}

/**
 * Turn a Tesseract text blob into structured price rows. Splits on newlines,
 * parses each, and drops noise rows.
 */
export function extractRows(ocrText: string): OcrRow[] {
  if (!ocrText) return [];
  const out: OcrRow[] = [];
  for (const raw of ocrText.split(/\r?\n/)) {
    const row = parseRow(raw);
    if (row) out.push(row);
  }
  return out;
}
