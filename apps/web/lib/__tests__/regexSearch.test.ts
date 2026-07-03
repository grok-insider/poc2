import { describe, expect, test } from "bun:test";
import { assembleSearch, MAX_SEARCH_LENGTH } from "../regex/searchString";
import { emptyVendorSettings, classPrefix, vendorTerms, VENDOR_CLASSES } from "../regex/vendor";

describe("assembleSearch", () => {
  test("all-mode: separate quoted AND terms", () => {
    const r = assembleSearch([{ pattern: "a" }, { pattern: "b" }], "all");
    expect(r.value).toBe('"a" "b"');
  });

  test("any-mode: one OR group", () => {
    const r = assembleSearch([{ pattern: "a" }, { pattern: "b" }], "any");
    expect(r.value).toBe('"a|b"');
  });

  test("negated terms merge into one !group", () => {
    const r = assembleSearch(
      [{ pattern: "keep" }, { pattern: "bad1", negate: true }, { pattern: "bad2", negate: true }],
      "all",
    );
    expect(r.value).toBe('"keep" "!bad1|bad2"');
  });

  test("custom text appends and empty terms drop", () => {
    const r = assembleSearch([{ pattern: "" }, { pattern: "x" }], "all", "  extra ");
    expect(r.value).toBe('"x" extra');
  });

  test("budget flag", () => {
    const long = "y".repeat(MAX_SEARCH_LENGTH);
    const r = assembleSearch([{ pattern: long }], "all");
    expect(r.overBudget).toBe(true);
    expect(assembleSearch([{ pattern: "ok" }], "all").overBudget).toBe(false);
  });
});

describe("vendor generator", () => {
  test("class prefixes are unique among all class names", () => {
    const seen = new Map<string, string>();
    for (const c of VENDOR_CLASSES) {
      const p = classPrefix(c);
      // No other class may start with this prefix.
      for (const other of VENDOR_CLASSES) {
        if (other === c) continue;
        expect(other.toLowerCase().startsWith(p)).toBe(false);
      }
      expect(seen.has(p)).toBe(false);
      seen.set(p, c);
    }
  });

  test("classic body/boots/bows disambiguation", () => {
    expect(classPrefix("Body Armours")).toBe("bod");
    expect(classPrefix("Boots")).toBe("boo");
    expect(classPrefix("Bows")).toBe("bow");
  });

  test("empty settings produce no terms", () => {
    expect(vendorTerms(emptyVendorSettings())).toEqual([]);
  });

  test("a combined shopping filter assembles as expected", () => {
    const s = emptyVendorSettings();
    s.classes = ["Boots"];
    s.rarity.rare = true;
    s.itemLevel = { min: 75, max: 0 };
    s.movementSpeeds = [30, 25];
    s.resists.fire = true;
    s.resists.cold = true;
    const terms = vendorTerms(s);
    const r = assembleSearch(terms, "all");
    expect(r.value).toContain('"s: boo"');
    expect(r.value).toContain('"y: r"');
    expect(r.value).toContain("m level: ");
    expect(r.value).toContain("% i.*mov");
    expect(r.value).toContain("(fi|co).*resi");
    expect(r.overBudget).toBe(false);

    // The movement pattern matches real tooltip lines for the picked
    // speeds and rejects others.
    const move = terms.find((t) => t.pattern.includes("mov"))!.pattern;
    const re = new RegExp(move);
    expect(re.test("30% increased movement speed")).toBe(true);
    expect(re.test("25% increased movement speed")).toBe(true);
    expect(re.test("20% increased movement speed")).toBe(false);

    // The ilvl pattern honors its \b guard.
    const ilvl = terms.find((t) => t.pattern.includes("m level"))!.pattern;
    const ire = new RegExp(ilvl);
    expect(ire.test("item level: 79")).toBe(true);
    expect(ire.test("item level: 74")).toBe(false);
  });
});
