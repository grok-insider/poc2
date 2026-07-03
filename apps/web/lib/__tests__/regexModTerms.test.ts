import { describe, expect, test } from "bun:test";
import { termsForMods, termsForSpec } from "../regex/modTerms";
import type { EligibleModView, TargetSpec } from "../types";

function view(partial: Partial<EligibleModView> & { mod_id: string }): EligibleModView {
  return {
    name: null,
    mod_group: partial.mod_id.replace(/\d+$/, ""),
    affix_type: "prefix",
    kind: "explicit",
    concepts: [],
    tags: [],
    tier_index: 1,
    tier_count: 1,
    required_level: 1,
    eligible_now: true,
    blocked_by_min_level: false,
    blocked_by_group: false,
    weight: 100,
    weight_share: 0.1,
    text_template: null,
    stats: [],
    is_hybrid: false,
    is_essence_only: false,
    is_desecrated_only: false,
    is_local: false,
    ...partial,
  };
}

const POOL: EligibleModView[] = [
  view({
    mod_id: "IncreasedLife1",
    affix_type: "prefix",
    concepts: ["Life"],
    text_template: "+(70-89) to maximum Life",
    stats: [{ stat_id: "life", min: 70, max: 89 }],
    tier_index: 1,
    tier_count: 2,
  }),
  view({
    mod_id: "IncreasedLife2",
    affix_type: "prefix",
    concepts: ["Life"],
    text_template: "+(50-69) to maximum Life",
    stats: [{ stat_id: "life", min: 50, max: 69 }],
    tier_index: 2,
    tier_count: 2,
  }),
  view({
    mod_id: "IncreasedMana1",
    affix_type: "prefix",
    concepts: ["Mana"],
    text_template: "+(40-59) to maximum Mana",
    stats: [{ stat_id: "mana", min: 40, max: 59 }],
  }),
  view({
    mod_id: "IncreasedES1",
    affix_type: "prefix",
    concepts: ["EnergyShield"],
    text_template: "(80-91)% increased [EnergyShield|Energy Shield]",
    stats: [{ stat_id: "es_pct", min: 80, max: 91 }],
  }),
  view({
    mod_id: "HybridESLife1",
    affix_type: "prefix",
    concepts: ["EnergyShield", "Life"],
    text_template: "(30-39)% increased [EnergyShield|Energy Shield]\n+(10-19) to maximum Life",
    stats: [
      { stat_id: "es_pct", min: 30, max: 39 },
      { stat_id: "life", min: 10, max: 19 },
    ],
    is_hybrid: true,
  }),
  view({
    mod_id: "FireRes1",
    affix_type: "suffix",
    concepts: ["FireResistance"],
    text_template: "+(31-35)% to [Resistances|Fire Resistance]",
    stats: [{ stat_id: "fire_res", min: 31, max: 35 }],
  }),
];

describe("termsForMods", () => {
  test("single mod gets a term matching only its line", () => {
    const target = POOL.find((m) => m.mod_id === "IncreasedMana1")!;
    const { terms, exact } = termsForMods([target], POOL);
    expect(exact).toBe(true);
    expect(terms.length).toBeGreaterThan(0);
    const re = new RegExp(terms[0].pattern);
    expect(re.test("+52 to maximum mana")).toBe(true);
    expect(re.test("+85 to maximum life")).toBe(false);
    expect(re.test("87% increased energy shield")).toBe(false);
  });

  test("value floor prefixes the roll and matches only big rolls", () => {
    const target = POOL.find((m) => m.mod_id === "IncreasedLife1")!;
    const { terms } = termsForMods([target], POOL, 70);
    expect(terms.length).toBe(1);
    const re = new RegExp(terms[0].pattern);
    expect(re.test("+85 to maximum life")).toBe(true);
    expect(re.test("+52 to maximum life")).toBe(false);
  });

  test("no numeric roll → value floor ignored with a warning", () => {
    const flat = view({
      mod_id: "CannotBeFrozen1",
      concepts: ["Ailment"],
      text_template: "Cannot be Frozen",
    });
    const { terms, warnings } = termsForMods([flat], [...POOL, flat], 10);
    expect(terms.length).toBe(1);
    expect(warnings.some((w) => w.includes("value floor"))).toBe(true);
    expect(terms[0].pattern.includes("\\d")).toBe(false);
  });
});

describe("termsForSpec", () => {
  test("concept spec covers flat + hybrid lines and nothing else", () => {
    const spec: TargetSpec = { concept: "EnergyShield" };
    const res = termsForSpec(spec, "prefix", POOL);
    expect(res.terms.length).toBeGreaterThan(0);
    const patterns = res.terms.map((t) => t.pattern);
    const anyMatch = (line: string) => patterns.some((p) => new RegExp(p).test(line));
    expect(anyMatch("87% increased energy shield")).toBe(true);
    expect(anyMatch("+85 to maximum life")).toBe(false);
    expect(anyMatch("+52 to maximum mana")).toBe(false);
  });

  test("allow_hybrid: false without a tier floor is honestly unmatchable", () => {
    // The pure Life template is textually identical to the hybrid's life
    // line and no roll floor separates them — precision-first semantics
    // skip the group and warn instead of emitting a false-positive term.
    const spec: TargetSpec = { concept: "Life", allow_hybrid: false };
    const res = termsForSpec(spec, "prefix", POOL);
    expect(res.terms.length).toBe(0);
    expect(res.warnings.some((w) => w.includes("skipped") || w.includes("unique"))).toBe(true);
  });

  test("allow_hybrid: false WITH a tier floor separates by roll range", () => {
    // T1 pure Life rolls 70–89; the hybrid's life line caps at 19 — the
    // floor rules the sharer out, so the spec is matchable again.
    const spec: TargetSpec = { concept: "Life", allow_hybrid: false, min_tier: 1 };
    const res = termsForSpec(spec, "prefix", POOL);
    expect(res.terms.length).toBe(1);
    const re = new RegExp(res.terms[0].pattern);
    expect(re.test("+85 to maximum life")).toBe(true);
    expect(re.test("+12 to maximum life")).toBe(false);
  });

  test("tier floor derives a roll floor from the weakest qualifying tier", () => {
    const spec: TargetSpec = { concept: "Life", min_tier: 1, allow_hybrid: false };
    const res = termsForSpec(spec, "prefix", POOL);
    expect(res.terms.length).toBe(1);
    const re = new RegExp(res.terms[0].pattern);
    // T1 min roll is 70 — a 52 roll (T2) must not match.
    expect(re.test("+85 to maximum life")).toBe(true);
    expect(re.test("+52 to maximum life")).toBe(false);
  });

  test("affix side filters the pool", () => {
    const spec: TargetSpec = { concept: "FireResistance" };
    expect(termsForSpec(spec, "prefix", POOL).terms.length).toBe(0);
    expect(termsForSpec(spec, "suffix", POOL).terms.length).toBe(1);
  });

  test("unknown concept warns instead of matching everything", () => {
    const res = termsForSpec({ concept: "Nonexistent" }, "prefix", POOL);
    expect(res.terms.length).toBe(0);
    expect(res.warnings.length).toBeGreaterThan(0);
  });
});
