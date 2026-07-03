import { describe, expect, test } from "bun:test";
import {
  coverFragment,
  escapeFragment,
  normalizeLine,
  selectPatterns,
  toCorpusLine,
} from "../regex/shortestUnique";

/** A realistic pool of normalized template lines (body-armour flavored). */
const POOL = [
  "#% increased Energy Shield",
  "+# to maximum Energy Shield",
  "#% increased Energy Shield\n+# to maximum Life",
  "+# to maximum Life",
  "+# to maximum Mana",
  "#% increased Armour",
  "+# to Armour",
  "#% increased Evasion Rating",
  "+#% to Fire Resistance",
  "+#% to Cold Resistance",
  "+#% to Lightning Resistance",
  "+#% to Chaos Resistance",
  "#% increased Rarity of Items found",
  "+# to Spirit",
  "#% reduced Attribute Requirements",
  "Regenerate # Life per second",
].flatMap((t) => t.split("\n"));

/** Check `pattern` (regex-lite) against a rendered line with rolls. */
function render(line: string): string {
  return normalizeLine(line).replace(/#/g, "42");
}

function matchesLine(pattern: string, line: string): boolean {
  return new RegExp(pattern).test(render(line));
}

describe("selectPatterns", () => {
  test("every pool line gets a pattern that matches ONLY itself", () => {
    for (const target of POOL) {
      const { patterns, exact } = selectPatterns([target], POOL);
      expect(exact).toBe(true);
      expect(patterns.length).toBe(1);
      const p = patterns[0];
      expect(matchesLine(p, target)).toBe(true);
      for (const other of POOL) {
        if (normalizeLine(other) === normalizeLine(target)) continue;
        if (matchesLine(p, other)) {
          throw new Error(`pattern /${p}/ for "${target}" also matches "${other}"`);
        }
      }
    }
  });

  test("covering fragment across related lines when one exists", () => {
    const targets = ["+#% to Fire Resistance", "+#% to Cold Resistance"];
    const { patterns } = selectPatterns(targets, POOL);
    // Either a single covering fragment or two per-line fragments — both
    // must match all targets collectively and nothing else.
    for (const t of targets) {
      expect(patterns.some((p) => matchesLine(p, t))).toBe(true);
    }
    for (const other of POOL) {
      if (targets.some((t) => normalizeLine(t) === normalizeLine(other))) continue;
      for (const p of patterns) {
        expect(matchesLine(p, other)).toBe(false);
      }
    }
  });

  test("identical templates dedupe into one target", () => {
    const { patterns } = selectPatterns(
      ["+# to maximum Life", "+# to maximum life"],
      POOL,
    );
    expect(patterns.length).toBe(1);
  });

  test("value-anchored mode disallows ^ anchors", () => {
    // Truncated post-number text — a ^ anchor would be wrong mid-line.
    const target = toCorpusLine(" to maximum life");
    const others = POOL.filter((l) => normalizeLine(l) !== "+# to maximum life").map(toCorpusLine);
    const frag = coverFragment([target], others, { noStartAnchor: true });
    expect(frag).not.toBeNull();
    expect(frag!.startsWith("^")).toBe(false);
  });

  test("fragments never contain digits (roll-safe uniqueness)", () => {
    const pool = [...POOL, "Gain # Rage on Melee Hit for 50 something"];
    for (const target of pool) {
      const { patterns } = selectPatterns([target], pool);
      for (const p of patterns) {
        expect(/\d/.test(p.replace(/\\d/g, ""))).toBe(false);
      }
    }
  });
});

describe("escapeFragment", () => {
  test("escapes regex metacharacters", () => {
    expect(escapeFragment("+5% (a|b)")).toBe("\\+5% \\(a\\|b\\)");
    expect(new RegExp(escapeFragment("+# to (x)")).test("+# to (x)")).toBe(true);
  });
});
