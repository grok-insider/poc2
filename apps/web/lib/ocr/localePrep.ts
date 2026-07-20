/// Pre-normalize OCR names before engine `resolveNames` with a client locale.
///
/// Spanish (and other) clients may show uncut gem levels as `(Nivel N)` instead
/// of English `(Level N)`. We strip or rewrite that so translation can hit the
/// bare gem name, then re-apply Level N for pricing.
///
/// Pure + unit-tested — no store/DOM.

/** English or Spanish level parenthetical. */
const LEVEL_PAREN = /\(\s*(?:level|nivel)\s*(\d{1,2})\s*\)\s*$/i;

export interface SplitLevelName {
  /** Name without trailing level paren. */
  base: string;
  /** 1–20 when present and valid; otherwise null. */
  level: number | null;
}

/** Split trailing `(Level N)` / `(Nivel N)` from an OCR name. */
export function splitLevelParen(raw: string): SplitLevelName {
  if (!raw) return { base: "", level: null };
  const m = LEVEL_PAREN.exec(raw);
  if (!m) return { base: raw.replace(/\s+/g, " ").trim(), level: null };
  const level = Number(m[1]);
  const base = raw.slice(0, m.index).replace(/\s+/g, " ").trim();
  if (!Number.isInteger(level) || level < 1 || level > 20) {
    return { base, level: null };
  }
  return { base, level };
}

/**
 * Rewrite Spanish `(Nivel N)` to English `(Level N)`.
 * Leaves non-matching strings unchanged.
 */
export function normalizeLocaleLevelSuffix(raw: string): string {
  if (!raw) return raw;
  return raw.replace(/\(\s*nivel\s*(\d{1,2})\s*\)\s*$/i, (_m, n: string) => `(Level ${n})`);
}

/**
 * If resolve returned a bare Uncut * Gem key and OCR had an explicit level,
 * rekey to the Level-N catalogue form for pricing.
 */
export function applyResolvedUncutLevel(
  key: string | null,
  ocrName: string,
): string | null {
  if (!key) return null;
  const { level } = splitLevelParen(ocrName);
  if (level === null) return key;
  if (!/^Uncut (?:Skill|Support|Spirit) Gem$/i.test(key.trim())) return key;
  const base = key.replace(/\s+/g, " ").trim().replace(/\bgem\b/i, "Gem");
  // Canonical casing
  const m = /^(Uncut) (skill|support|spirit) (Gem)$/i.exec(base);
  if (!m) return `${base} (Level ${level})`;
  const kind = m[2]!.charAt(0).toUpperCase() + m[2]!.slice(1).toLowerCase();
  return `Uncut ${kind} Gem (Level ${level})`;
}

/** Map store `clientLocale` to WASM `locale` (omit English). */
export function resolveLocaleArg(
  clientLocale: string | undefined,
): "de" | "fr" | "pt" | "ru" | "sp" | undefined {
  if (!clientLocale || clientLocale === "en") return undefined;
  if (
    clientLocale === "de" ||
    clientLocale === "fr" ||
    clientLocale === "pt" ||
    clientLocale === "ru" ||
    clientLocale === "sp"
  ) {
    return clientLocale;
  }
  return undefined;
}
