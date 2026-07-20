// Uncut Skill / Support / Spirit gems (poe2scout category `uncutgems`).
//
// Market keys always carry an explicit level: "Uncut Skill Gem (Level 12)".
// In-game reward text omits the suffix for level 1 — bare "Uncut Support Gem"
// means Level 1. Levels are integers 1–20.
//
// Keep in lockstep with apps/desktop/src/prices/uncutGems.ts (no shared package).

const KIND = "skill|support|spirit";

/** OCR often misreads trailing "Gem" as "Gel". */
function fixUncutGemTypos(raw: string): string {
  return raw
    .replace(/\s+/g, " ")
    .trim()
    .replace(new RegExp(`\\b(uncut\\s+(?:${KIND})\\s+)gel\\b`, "i"), "$1Gem");
}

/**
 * Tolerant "(Level N)" / "(level N)" tail. Allows common OCR swaps inside the
 * word "level" (l/1/I, e/1).
 */
const LEVEL_PAREN =
  /\(\s*l[e1il]{0,2}v[e1il]{0,2}l?\s*(\d{1,2})\s*\)\s*$/i;

const BASE_ONLY = new RegExp(`^uncut\\s+(${KIND})\\s+gem$`, "i");

export interface ParsedUncutGem {
  /** Canonical base without level, e.g. "Uncut Support Gem". */
  base: string;
  /** 1–20. */
  level: number;
  /** Scout-style display name, e.g. "Uncut Support Gem (Level 1)". */
  canonical: string;
}

function titleKind(kind: string): string {
  const k = kind.toLowerCase();
  return k.charAt(0).toUpperCase() + k.slice(1);
}

/** Parse bare or levelled uncut gem text; bare ⇒ level 1. */
export function parseUncutGem(raw: string): ParsedUncutGem | null {
  if (!raw) return null;
  let s = fixUncutGemTypos(raw);
  let level = 1;
  let hadLevelParen = false;
  const lm = LEVEL_PAREN.exec(s);
  if (lm) {
    hadLevelParen = true;
    level = Number(lm[1]);
    s = s.slice(0, lm.index).trim();
  }
  if (!Number.isInteger(level) || level < 1 || level > 20) {
    if (hadLevelParen) return null;
    level = 1;
  }
  const bm = BASE_ONLY.exec(s);
  if (!bm) return null;
  const base = `Uncut ${titleKind(bm[1]!)} Gem`;
  return { base, level, canonical: `${base} (Level ${level})` };
}

/**
 * Query strings to try for resolve / fuzzy match. Canonical Level-N form is
 * first so the scout catalogue hits before the bare OCR string.
 */
export function expandUncutGemQuery(raw: string): string[] {
  const parsed = parseUncutGem(raw);
  if (!parsed) return raw ? [raw] : [];
  const out: string[] = [parsed.canonical];
  if (!out.includes(parsed.base)) out.push(parsed.base);
  const trimmed = raw.replace(/\s+/g, " ").trim();
  if (trimmed && !out.includes(trimmed)) out.push(trimmed);
  return out;
}

/**
 * If `catalogueName` is an explicit Level-1 scout row, return the bare in-game
 * alias ("Uncut Skill Gem"); otherwise null.
 */
export function bareAliasForLevel1CatalogueName(catalogueName: string): string | null {
  const m = catalogueName
    .replace(/\s+/g, " ")
    .trim()
    .match(new RegExp(`^(Uncut (?:Skill|Support|Spirit) Gem) \\(Level 1\\)$`, "i"));
  if (!m) return null;
  const parsed = parseUncutGem(m[1]!);
  return parsed?.base ?? null;
}
