/// Presentation helpers shared by the Forge components.

import type { AdvisorAction, DivEquiv, Item, ModRoll, Rarity, Recommendation } from "./types";

/** Turn an engine currency/omen id into a spaced display name. */
export function humanizeId(id: string): string {
  return id
    .replace(/([a-z0-9])([A-Z])/g, "$1 $2")
    .replace(/^Orb Of /, "Orb of ")
    .replace(/\bOf\b/g, "of")
    .trim();
}

/**
 * Display name for a mod id. Mod ids end in a group ordinal
 * (`IncreasedLife7`) that is NOT the display tier — strip it rather than
 * show a misleading number; rolled values disambiguate visually.
 */
export function humanizeModId(id: string): string {
  return humanizeId(id.replace(/\d+$/, ""));
}

/**
 * Split a mod's stat-text template into readable per-stat lines, e.g.
 * `"(80-91)% increased [EnergyShield|Energy Shield]\n+(7-10) to maximum [Life|Life]"`
 * → `["#% increased Energy Shield", "+# to maximum Life"]`.
 * Tags `[Id|Display]`/`[Id]` collapse to their display text; numeric roll ranges
 * `(min-max)` become `#` (the specific range is per-tier, shown separately).
 */
export function modLines(template: string | null | undefined): string[] {
  if (!template) return [];
  return template
    .split(/\r?\n+/)
    .map((line) =>
      line
        .replace(/\[([^\]|]+)(?:\|([^\]]+))?\]/g, (_m, id: string, disp?: string) => disp ?? id)
        .replace(/\(\s*[+-]?\d+(?:\.\d+)?\s*-\s*[+-]?\d+(?:\.\d+)?\s*\)/g, "#")
        .replace(/\s{2,}/g, " ")
        .trim(),
    )
    .filter(Boolean);
}

/** The full stat text of a mod, lines joined with " · " (hybrid = multi-stat). */
export function modText(template: string | null | undefined): string {
  return modLines(template).join(" · ");
}

/** A short, human label for an advisor action. */
export function actionLabel(action: AdvisorAction): string {
  switch (action.kind) {
    case "apply_currency": {
      const base = humanizeId(action.currency);
      return action.omens?.length
        ? `${base} + ${action.omens.map(humanizeId).join(", ")}`
        : base;
    }
    case "activate_omen":
      return `Activate ${humanizeId(action.omen)}`;
    case "apply_hinekoras_lock":
      return "Apply Hinekora's Lock";
    case "reveal":
      return "Reveal at the Well of Souls";
    case "recombine":
      return "Recombinate";
    case "stop":
      return "Stop — goal reached";
    case "abandon":
      return `Abandon — ${action.reason}`;
    case "guidance":
      return action.note;
    case "recurring":
      return `Repeat: ${action.inner.map(actionLabel).join(" → ")}`;
    default:
      return "Next step";
  }
}

export function actionKindLabel(action: AdvisorAction): string {
  const m: Record<AdvisorAction["kind"], string> = {
    apply_currency: "Currency",
    activate_omen: "Omen",
    apply_hinekoras_lock: "Lock",
    reveal: "Desecrate",
    recombine: "Recombinator",
    stop: "Done",
    abandon: "Abandon",
    guidance: "Advice",
    recurring: "Loop",
  };
  return m[action.kind] ?? "Step";
}

export function sourceLabel(rec: Recommendation): string {
  const s = rec.source;
  if (s.kind === "rule") return `rule ${s.id}`;
  if (s.kind === "strategy") return `strategy ${s.id} · ${s.step}`;
  return `heuristic ${s.name}`;
}

export function pct(p: number): string {
  return `${(p * 100).toFixed(0)}%`;
}

export function div(d: DivEquiv | number | null | undefined): string {
  if (d == null) return "—";
  const v = typeof d === "number" ? d : d.expected;
  if (v === 0) return "free";
  if (v < 1) return `${v.toFixed(2)}d`;
  if (v < 10) return `${v.toFixed(1)}d`;
  return `${Math.round(v)}d`;
}

export type RiskBucket = "low" | "medium" | "high";
/**
 * Colour bucket for the headline P(reach goal). Re-tuned for the honest
 * goal-attainment scale (reliability × goal-progress), which is inherently
 * lower than the old raw step-execution probability — reaching a full
 * multi-spec rare is genuinely hard, so ≥40% is "good" (green), not 60%.
 * "low"/"medium"/"high" name the *risk*, so high P(reach goal) ⇒ low risk.
 */
export function riskBucket(p: number): RiskBucket {
  if (p >= 0.4) return "low";
  if (p >= 0.12) return "medium";
  return "high";
}

export function rarityClass(r: Rarity): string {
  return `r-${r}`;
}

export function rarityGlyph(r: Rarity): string {
  return r === "rare" ? "◆" : r === "magic" ? "◈" : r === "unique" ? "✦" : "◇";
}

/** A compact "+value Mod" label from a ModRoll (values joined). */
export function modValue(m: ModRoll): string {
  if (!m.values?.length) return "";
  return m.values.map((v) => (Number.isInteger(v) ? `${v}` : v.toFixed(1))).join("–");
}

export function affixCounts(item: Item) {
  const max = item.rarity === "magic" ? 1 : 3;
  return {
    prefix: { used: item.prefixes.length, max },
    suffix: { used: item.suffixes.length, max },
  };
}
