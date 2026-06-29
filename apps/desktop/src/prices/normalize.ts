// Name normalization for price lookups — MUST match the Rust matcher's
// `name_match::normalize` (crates/market/src/name_match.rs) byte-for-byte so an
// OCR'd name, the engine's fuzzy resolve, and this cache's keys all line up:
// lowercase, flatten every non-alphanumeric run to a single space, trim.
//
//   "  Greater   Vision-Rune!! "  →  "greater vision rune"

export function normalizeName(s: string): string {
  let out = "";
  let pendingSpace = false;
  for (const ch of s) {
    // \p{L} (letters) + \p{N} (numbers) mirrors Rust's char::is_alphanumeric.
    if (/[\p{L}\p{N}]/u.test(ch)) {
      if (pendingSpace && out.length > 0) out += " ";
      pendingSpace = false;
      out += ch.toLowerCase();
    } else {
      pendingSpace = true;
    }
  }
  return out;
}
