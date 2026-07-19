/// Name normalization for price lookups — MUST match the Rust matcher's
/// `name_match::normalize` (crates/market/src/name_match.rs) AND the desktop
/// cache's `normalizeName` (apps/desktop/src/prices/normalize.ts): lowercase,
/// flatten every non-alphanumeric run to a single space, trim. Keeping all
/// three in lockstep is what lets an OCR'd name, the fuzzy resolve, and the
/// cached price line up on the same key.
export function normalizeName(s: string): string {
  let out = "";
  let pendingSpace = false;
  for (const ch of s) {
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
