/// Assembles final in-game search strings from generated terms.
///
/// PoE2 search-box semantics (stash / vendor / reveal search):
///   - space-separated quoted terms are AND-combined: `"a" "b"`
///   - `|` inside one quoted term is OR: `"a|b"`
///   - a leading `!` negates a term: `"!a"`
///   - max length: 250 characters.

export const MAX_SEARCH_LENGTH = 250;

export interface SearchTerm {
  /** Regex-lite pattern (no quotes). */
  pattern: string;
  /** Negated terms are prefixed with `!` and grouped together. */
  negate?: boolean;
}

export interface AssembledSearch {
  value: string;
  length: number;
  overBudget: boolean;
}

/** Quote a term (the game treats the quoted body as one pattern). */
function quote(body: string): string {
  return `"${body}"`;
}

/**
 * Combine terms into the final string.
 *
 * `mode: "all"` — every positive term must match (separate quoted terms).
 * `mode: "any"` — any positive term suffices (one quoted OR-group).
 * Negated terms are always OR-merged into a single `"!…"` group (an item
 * matching ANY unwanted pattern is rejected).
 */
export function assembleSearch(
  terms: SearchTerm[],
  mode: "all" | "any",
  customText = "",
): AssembledSearch {
  const positive = terms.filter((t) => !t.negate && t.pattern !== "");
  const negative = terms.filter((t) => t.negate && t.pattern !== "");

  const parts: string[] = [];
  if (positive.length > 0) {
    if (mode === "any") {
      parts.push(quote(positive.map((t) => t.pattern).join("|")));
    } else {
      parts.push(...positive.map((t) => quote(t.pattern)));
    }
  }
  if (negative.length > 0) {
    parts.push(quote(`!${negative.map((t) => t.pattern).join("|")}`));
  }
  const custom = customText.trim();
  if (custom !== "") parts.push(custom);

  const value = parts.join(" ");
  return { value, length: value.length, overBudget: value.length > MAX_SEARCH_LENGTH };
}
