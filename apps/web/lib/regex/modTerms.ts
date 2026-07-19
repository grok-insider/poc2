/// Builds in-game search terms for engine mods, driven by the eligible
/// pool the store already holds (`EligibleModsResponse` for the bare
/// item — the full corpus of lines this base can roll).
///
/// Tier constraints can't be expressed by text (all tiers of a group
/// share one template) — they become value floors: "T2+ Life" emits
/// `<atLeast(minRoll)>.*<fragment>` where minRoll is the weakest
/// qualifying tier's minimum roll. Floors are PER MOD GROUP (each
/// group's rolls live on its own scale).

import type { EligibleModView, TargetSpec } from "../types";
import { modLines } from "../format";
import { atLeastRegex } from "./numberRegex";
import { normalizeLine, selectPatterns } from "./shortestUnique";
import type { SearchTerm } from "./searchString";

/** All display lines of a mod view (hybrids have several). */
export function viewLines(view: EligibleModView): string[] {
  return modLines(view.text_template);
}

/** The full corpus of lines a pool can show (deduped, normalized). */
export function poolLines(pool: EligibleModView[]): string[] {
  const out = new Set<string>();
  for (const v of pool) for (const l of viewLines(v)) out.add(normalizeLine(l));
  return [...out];
}

export interface ModTermResult {
  /** Emitted search terms (OR-merge for "any of these mods"). */
  terms: SearchTerm[];
  /** False when a fallback (non-unique) fragment had to be used. */
  exact: boolean;
  /** Human warnings (no qualifying mods, no value data for a tier floor…). */
  warnings: string[];
}

interface LinePatterns {
  patterns: string[];
  exact: boolean;
  /** True when the requested value floor was actually applied. */
  floored: boolean;
}

/**
 * Patterns selecting `targetLines` out of `corpus`. With a value floor,
 * fragments come from the text AFTER the first roll marker and every
 * pattern is prefixed `atLeast(min).*` — the corpus drops the original
 * (untruncated) target lines so truncated fragments can still be unique.
 */
function patternsForLines(
  targetLines: string[],
  corpus: string[],
  minValue: number | null,
): LinePatterns {
  if (minValue == null || minValue <= 0) {
    return { ...selectPatterns(targetLines, corpus), floored: false };
  }

  const originals = new Set(targetLines.map(normalizeLine));
  const truncated = [...originals]
    .map((l) => {
      const idx = l.indexOf("#");
      return idx >= 0 ? l.slice(idx + 1) : null;
    })
    .filter((l): l is string => l !== null && l.trim().length > 0);
  if (truncated.length === 0) {
    // No numeric roll to anchor — plain text match.
    return { ...selectPatterns(targetLines, corpus), floored: false };
  }

  const rest = corpus.filter((l) => !originals.has(normalizeLine(l)));
  const prefix = atLeastRegex(Math.floor(minValue));
  const { patterns, exact } = selectPatterns(truncated, rest, { noStartAnchor: true });
  return { patterns: patterns.map((p) => `${prefix}.*${p}`), exact, floored: true };
}

/** The max roll a view can show on a given normalized line (its stats
 * array parallels its template lines). `-Infinity` when unknown — an
 * unknown sharer can never be ruled out by a floor. */
function maxRollForLine(view: EligibleModView, line: string): number {
  const idx = viewLines(view)
    .map(normalizeLine)
    .findIndex((l) => l === line);
  const stat = view.stats[idx] ?? null;
  return stat ? stat.max : Number.NEGATIVE_INFINITY;
}

/**
 * Normalized lines of `views` that identify them within `pool`:
 * a line qualifies when no mod outside `targetIds` shows the same text —
 * OR when a value `floor` is active and every sharer's roll on that line
 * maxes out BELOW the floor (the number disambiguates: a hybrid's
 * 10–19 life line can never display "+85").
 */
function exclusiveLines(
  views: EligibleModView[],
  pool: EligibleModView[],
  targetIds: Set<string>,
  floor: number | null = null,
): { lines: string[]; dropped: string[] } {
  const nonTargets = pool.filter((v) => !targetIds.has(v.mod_id));
  const lines = new Set<string>();
  const dropped: string[] = [];
  for (const v of views) {
    const own = viewLines(v).map(normalizeLine);
    const usable = own.filter((l) => {
      const sharers = nonTargets.filter((nt) =>
        viewLines(nt).some((ntl) => normalizeLine(ntl) === l),
      );
      if (sharers.length === 0) return true;
      if (floor == null || floor <= 0) return false;
      return sharers.every((s) => {
        const max = maxRollForLine(s, l);
        return Number.isFinite(max) && max < floor;
      });
    });
    if (usable.length === 0) {
      dropped.push(v.name ?? v.mod_id);
    } else {
      for (const l of usable) lines.add(l);
    }
  }
  return { lines: [...lines], dropped };
}

/**
 * Terms selecting "any of `targets`" out of `pool` (Item-mods tab).
 * With `minValue`, patterns are roll-floored. Mods whose every line is
 * shared with non-targets fall back to their own lines (flagged
 * approximate) — the user explicitly picked them.
 */
export function termsForMods(
  targets: EligibleModView[],
  pool: EligibleModView[],
  minValue?: number | null,
): ModTermResult {
  const warnings: string[] = [];
  if (targets.length === 0) return { terms: [], exact: true, warnings: ["no matching mods"] };

  const all = poolLines(pool);
  // Same-group tiers share their template by definition — selecting one
  // tier means "this mod family" (an optional value floor distinguishes
  // tiers), so siblings must not count as "other mods" when judging
  // line exclusivity. Without this, any pick from a multi-tier group
  // fell back to over-matching whole-line alternatives.
  const targetGroups = new Set(targets.map((v) => v.mod_group));
  const targetIds = new Set(targets.map((v) => v.mod_id));
  for (const v of pool) {
    if (targetGroups.has(v.mod_group)) targetIds.add(v.mod_id);
  }
  const { lines, dropped } = exclusiveLines(targets, pool, targetIds);

  let targetLines = lines;
  let sharedFallback = false;
  if (targetLines.length === 0) {
    // Every line is shared — match the mod's own lines anyway, flagged.
    sharedFallback = true;
    targetLines = [...new Set(targets.flatMap((v) => viewLines(v).map(normalizeLine)))];
    warnings.push("this mod shares all its text with other mods — pattern may over-match");
  } else if (dropped.length > 0) {
    warnings.push(
      `some lines are shared with other mods and were skipped (${dropped.join(", ")})`,
    );
  }

  const { patterns, exact, floored } = patternsForLines(targetLines, all, minValue ?? null);
  if (!exact) warnings.push("no fully-unique fragment for some lines — pattern may over-match");
  if (minValue != null && minValue > 0 && !floored) {
    warnings.push("selected mods have no numeric roll — value floor ignored");
  }

  return {
    terms: patterns.map((pattern) => ({ pattern })),
    exact: exact && !sharedFallback,
    warnings,
  };
}

export interface SpecTermResult extends ModTermResult {
  /** Display label for the spec ("EnergyShield ≥T2 (prefix)"). */
  label: string;
}

/** Does `view` produce any of the spec's accepted concepts? */
function viewMatchesConcept(view: EligibleModView, spec: TargetSpec): boolean {
  const wanted = new Set<string>();
  if (spec.concept) wanted.add(spec.concept);
  for (const c of spec.concept_any ?? []) wanted.add(c);
  if (wanted.size === 0) return false;
  return view.concepts.some((c) => wanted.has(c));
}

/**
 * Terms for one goal target spec against the bare-item pool.
 *
 * Qualifying views: concept match + affix side + hybrid policy + tier
 * floor (`tier_index <= min_tier`; tier 1 = strongest). Work is done
 * PER MOD GROUP so each group gets its own roll floor (derived from its
 * weakest qualifying tier) — one global floor across groups would be
 * meaningless (every group rolls on its own scale).
 *
 * Goal semantics prefer precision over recall: mods that cannot be
 * uniquely identified by text are SKIPPED (with a warning) instead of
 * emitting over-matching patterns — a stash search that constantly
 * lights up wrong items is worse than one that misses a rare hybrid.
 */
export function termsForSpec(
  spec: TargetSpec,
  affix: "prefix" | "suffix",
  pool: EligibleModView[],
): SpecTermResult {
  const conceptLabel =
    spec.concept ?? (spec.concept_any?.length ? spec.concept_any.join("/") : "?");
  const tierLabel = spec.min_tier ? ` ≥T${spec.min_tier}` : "";
  const label = `${conceptLabel}${tierLabel} (${affix})`;

  const sided = pool.filter((v) => v.affix_type === affix);
  let candidates = sided.filter((v) => viewMatchesConcept(v, spec));
  if (spec.allow_hybrid === false) candidates = candidates.filter((v) => !v.is_hybrid);

  const qualifying = spec.min_tier
    ? candidates.filter((v) => v.tier_index <= (spec.min_tier as number))
    : candidates;

  if (qualifying.length === 0) {
    return { label, terms: [], exact: true, warnings: ["no qualifying mods in this base's pool"] };
  }

  const all = poolLines(pool);
  const targetIds = new Set(qualifying.map((v) => v.mod_id));
  const warnings: string[] = [];
  const patterns: string[] = [];
  let exact = true;

  // Group qualifying tiers by mod group (they share one template).
  const groups = new Map<string, EligibleModView[]>();
  for (const v of qualifying) {
    const g = groups.get(v.mod_group) ?? [];
    g.push(v);
    groups.set(v.mod_group, g);
  }

  const skipped: string[] = [];
  for (const [groupId, views] of groups) {
    // Tier floor: only when the pool carries lower (non-qualifying)
    // tiers of this group — otherwise the text alone is tier-exact.
    let minValue: number | null = null;
    if (spec.min_tier != null) {
      const hasLowerTiers = pool.some(
        (v) => v.mod_group === groupId && v.tier_index > (spec.min_tier as number),
      );
      if (hasLowerTiers) {
        const mins = views
          .map((v) => v.stats[0]?.min)
          .filter((m): m is number => typeof m === "number" && m > 0);
        if (mins.length > 0) {
          minValue = Math.min(...mins);
        } else {
          warnings.push(
            `${views[0].name ?? groupId}: tier floor has no roll data — matches every tier`,
          );
        }
      }
    }

    // Lower tiers of THIS group share the template by definition — the
    // roll floor distinguishes them, so they don't count as "shared
    // text". Everything else outside the qualifying set does (unless the
    // floor also rules the sharer out by roll range).
    const groupIds = new Set(targetIds);
    for (const v of pool) if (v.mod_group === groupId) groupIds.add(v.mod_id);
    const { lines } = exclusiveLines(views, pool, groupIds, minValue);
    if (lines.length === 0) {
      skipped.push(views[0].name ?? groupId);
      continue;
    }

    const r = patternsForLines(lines, all, minValue);
    if (!r.exact) exact = false;
    for (const p of r.patterns) {
      if (!patterns.includes(p)) patterns.push(p); // dedupe across groups
    }
  }

  if (skipped.length > 0) {
    warnings.push(
      `skipped (text is shared with non-qualifying mods): ${skipped.join(", ")}`,
    );
  }
  if (!exact) warnings.push("no fully-unique fragment for some lines — pattern may over-match");
  if (patterns.length === 0) {
    warnings.push("no uniquely-matchable mods for this spec");
  }

  return {
    label,
    terms: patterns.map((pattern) => ({ pattern })),
    exact,
    warnings,
  };
}
