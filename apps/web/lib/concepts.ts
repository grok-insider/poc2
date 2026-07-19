/// Cross-reference helpers between an item's current mods, the base's eligible
/// mod pool, and target specs. Pure + testable — used by the store's
/// `seedTargetFromItem` and the TargetEditor palette.
///
/// All of this honours the user's caps automatically: the eligible response
/// (from the fixed `eligible` command) only contains mods the item's base
/// (str/dex/int) can roll, and each view's `eligible_now` reflects whether the
/// tier is reachable at the item's level. So "what can I target" and "best
/// achievable tier" both fall out of the eligible data — no client-side base
/// or ilvl logic.

import type {
  EligibleModsResponse,
  EligibleModView,
  Item,
  ModRoll,
  TargetSpec,
} from "./types";

export type AffixSlot = "prefix" | "suffix";

/** A targetable concept the current base can roll, with its tier info. */
export interface ConceptOption {
  concept: string;
  affix: AffixSlot;
  /** Total tiers in the pool for this concept (max across its mods). */
  tierCount: number;
  /** Best (lowest = strongest) tier index reachable at the item's level. */
  bestTier: number;
  /** Lowest required level among reachable tiers. */
  bestRequiredLevel: number;
  /** How many distinct mods carry this concept. */
  modCount: number;
}

/** Index the eligible response by mod id (first match wins). */
export function indexByModId(
  resp: EligibleModsResponse | null,
): Map<string, EligibleModView> {
  const m = new Map<string, EligibleModView>();
  for (const v of resp?.mods ?? []) {
    if (!m.has(v.mod_id)) m.set(v.mod_id, v);
  }
  return m;
}

/** Unique targetable concepts per affix slot that the base can roll *now*. */
export function conceptPalette(resp: EligibleModsResponse | null): {
  prefixes: ConceptOption[];
  suffixes: ConceptOption[];
} {
  const acc = new Map<string, ConceptOption>();
  for (const v of resp?.mods ?? []) {
    if (v.affix_type !== "prefix" && v.affix_type !== "suffix") continue;
    for (const c of v.concepts) {
      const key = `${v.affix_type}:${c}`;
      let o = acc.get(key);
      if (!o) {
        o = {
          concept: c,
          affix: v.affix_type,
          tierCount: 0,
          bestTier: Infinity,
          bestRequiredLevel: Infinity,
          modCount: 0,
        };
        acc.set(key, o);
      }
      o.modCount += 1;
      o.tierCount = Math.max(o.tierCount, v.tier_count);
      if (v.eligible_now) {
        o.bestTier = Math.min(o.bestTier, v.tier_index);
        o.bestRequiredLevel = Math.min(o.bestRequiredLevel, v.required_level);
      }
    }
  }
  // Keep only concepts reachable at the current item level.
  const list = [...acc.values()].filter((o) => Number.isFinite(o.bestTier));
  const sort = (a: ConceptOption, b: ConceptOption) => a.concept.localeCompare(b.concept);
  return {
    prefixes: list.filter((o) => o.affix === "prefix").sort(sort),
    suffixes: list.filter((o) => o.affix === "suffix").sort(sort),
  };
}

/** `${affix}:${concept}` → best achievable tier, for seed/clamp lookups. */
export function bestTierMap(resp: EligibleModsResponse | null): Map<string, number> {
  const { prefixes, suffixes } = conceptPalette(resp);
  const m = new Map<string, number>();
  for (const o of [...prefixes, ...suffixes]) m.set(`${o.affix}:${o.concept}`, o.bestTier);
  return m;
}

/** `${affix}:${concept}` → tier count (for clamping a row's min-tier stepper). */
export function tierCountMap(resp: EligibleModsResponse | null): Map<string, number> {
  const { prefixes, suffixes } = conceptPalette(resp);
  const m = new Map<string, number>();
  for (const o of [...prefixes, ...suffixes]) m.set(`${o.affix}:${o.concept}`, o.tierCount);
  return m;
}

/**
 * Seed target specs from the item's current mods. Each current mod becomes a
 * target on its concept(s), at the **best achievable tier** for that concept on
 * this base + item level (the user's "best tier" choice, ilvl-clamped).
 */
export function seedSpecsFromItem(
  item: Item,
  resp: EligibleModsResponse | null,
): { prefixes: TargetSpec[]; suffixes: TargetSpec[] } {
  const byId = indexByModId(resp);
  const best = bestTierMap(resp);

  const build = (rolls: ModRoll[], affix: AffixSlot): TargetSpec[] => {
    const specs: TargetSpec[] = [];
    for (const r of rolls) {
      const view = byId.get(r.mod_id);
      const concepts = view?.concepts ?? [];
      if (concepts.length === 0) continue; // unmappable mod: skip rather than guess
      const minTier = best.get(`${affix}:${concepts[0]}`) ?? 1;
      specs.push(
        concepts.length === 1
          ? { concept: concepts[0], affix, count: 1, min_tier: minTier, allow_hybrid: true }
          : { concept_any: concepts, affix, count: 1, min_tier: minTier, allow_hybrid: true },
      );
    }
    return dedupeByConcept(specs);
  };

  return {
    prefixes: build(item.prefixes, "prefix"),
    suffixes: build(item.suffixes, "suffix"),
  };
}

/** Collapse specs sharing a concept into one (strictest tier, bumped count). */
export function dedupeByConcept(specs: TargetSpec[]): TargetSpec[] {
  const out = new Map<string, TargetSpec>();
  for (const s of specs) {
    const key = s.concept ?? (s.concept_any ?? []).join("|");
    const cur = out.get(key);
    if (!cur) {
      out.set(key, { ...s });
    } else {
      cur.count = (cur.count ?? 1) + (s.count ?? 1);
      const a = cur.min_tier ?? null;
      const b = s.min_tier ?? null;
      cur.min_tier = a == null ? b : b == null ? a : Math.min(a, b);
    }
  }
  return [...out.values()];
}

/** Best-effort attribute variant label from the base's eligible concepts. */
export function attributeVariant(
  resp: EligibleModsResponse | null,
): "str" | "dex" | "int" | "hybrid" | null {
  const cs = new Set((resp?.mods ?? []).flatMap((m) => m.concepts));
  const flags = [cs.has("Armour"), cs.has("Evasion"), cs.has("EnergyShield")];
  const n = flags.filter(Boolean).length;
  if (n === 0) return null;
  if (n > 1) return "hybrid";
  return flags[0] ? "str" : flags[1] ? "dex" : "int";
}
