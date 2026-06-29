"use client";

import { useMemo } from "react";
import { useCraft } from "@/lib/store";
import { conceptPalette, tierCountMap, type ConceptOption } from "@/lib/concepts";
import { applicableArchetypes } from "@/lib/archetypes";
import { humanizeId } from "@/lib/format";
import type { DivEquiv, ItemPredicate, TargetSpec } from "@/lib/types";
import styles from "./TargetEditor.module.css";

type Slot = "prefixes" | "suffixes";
type Affix = "prefix" | "suffix";

const datalistId = (affix: Affix) => `target-concept-${affix}`;

/** Render the text-input value for a spec: comma-joined when it carries a
 *  `concept_any` set, otherwise the single `concept`. */
function specText(s: TargetSpec): string {
  if (s.concept_any && s.concept_any.length > 0) return s.concept_any.join(", ");
  return s.concept ?? "";
}

/** Map a free-text edit back onto a spec. A comma list becomes `concept_any`
 *  (with `concept` cleared); a single token becomes `concept` (with
 *  `concept_any` left empty). */
function applyText(s: TargetSpec, raw: string): TargetSpec {
  const parts = raw
    .split(",")
    .map((p) => p.trim())
    .filter(Boolean);
  if (raw.includes(",") || parts.length > 1) {
    return { ...s, concept: null, concept_any: parts };
  }
  return { ...s, concept: parts[0] ?? "", concept_any: [] };
}

/** Human label for an abandon-criteria predicate (read-only info chips). */
function abandonLabel(p: ItemPredicate): string | null {
  if (typeof p === "string") return null;
  if ("corrupted" in p && p.corrupted) return "Corrupted";
  if ("sanctified" in p && p.sanctified) return "Sanctified";
  if ("mirrored" in p && p.mirrored) return "Mirrored";
  return null;
}

function Stepper({
  value,
  min,
  max,
  onChange,
  label,
  placeholder,
}: {
  value: number;
  min: number;
  max: number;
  onChange: (v: number) => void;
  label: string;
  placeholder?: string;
}) {
  const clamp = (v: number) => Math.max(min, Math.min(max, v));
  return (
    <div className={styles.stepper}>
      <span className={styles.stepperLabel}>{label}</span>
      <div className={styles.stepperBody}>
        <button
          className={styles.stepBtn}
          onClick={() => onChange(clamp(value - 1))}
          disabled={value <= min}
          aria-label={`decrease ${label}`}
        >
          −
        </button>
        <input
          className={`${styles.stepInput} num`}
          type="number"
          min={min}
          max={max}
          value={value === 0 ? "" : value}
          placeholder={placeholder}
          onChange={(e) => {
            const raw = e.target.value;
            if (raw === "") return onChange(0);
            onChange(clamp(Number(raw)));
          }}
        />
        <button
          className={styles.stepBtn}
          onClick={() => onChange(clamp(value + 1))}
          disabled={value >= max}
          aria-label={`increase ${label}`}
        >
          +
        </button>
      </div>
    </div>
  );
}

export function TargetEditor() {
  const goal = useCraft((s) => s.goal);
  const setGoal = useCraft((s) => s.setGoal);
  const item = useCraft((s) => s.item);
  const eligible = useCraft((s) => s.eligible);
  const seedTargetFromItem = useCraft((s) => s.seedTargetFromItem);
  const applyArchetype = useCraft((s) => s.applyArchetype);

  const palette = useMemo(() => conceptPalette(eligible), [eligible]);
  const tierCounts = useMemo(() => tierCountMap(eligible), [eligible]);
  const archetypes = useMemo(() => applicableArchetypes(eligible), [eligible]);
  const noPool = eligible !== null && eligible.data_available === false;
  const canSeed = item.prefixes.length + item.suffixes.length > 0 && !noPool;

  const prefixes = goal.target.prefixes ?? [];
  const suffixes = goal.target.suffixes ?? [];

  /** Primary concept of a spec (for palette de-dup + tier clamping). */
  function specConcept(s: TargetSpec): string {
    return s.concept ?? s.concept_any?.[0] ?? "";
  }

  function commitSlot(slot: Slot, next: TargetSpec[]) {
    setGoal({ ...goal, target: { ...goal.target, [slot]: next } });
  }

  function updateSpec(slot: Slot, i: number, patch: Partial<TargetSpec>) {
    const list = slot === "prefixes" ? prefixes : suffixes;
    const next = list.map((s, idx) => (idx === i ? { ...s, ...patch } : s));
    commitSlot(slot, next);
  }

  function editText(slot: Slot, i: number, raw: string) {
    const list = slot === "prefixes" ? prefixes : suffixes;
    const next = list.map((s, idx) => (idx === i ? applyText(s, raw) : s));
    commitSlot(slot, next);
  }

  function addSpec(slot: Slot) {
    const list = slot === "prefixes" ? prefixes : suffixes;
    const fresh: TargetSpec = {
      concept: "",
      count: 1,
      min_tier: 1,
      allow_hybrid: true,
    };
    commitSlot(slot, [...list, fresh]);
  }

  /** Add a target for a concept the base can roll (from the palette). */
  function addConcept(slot: Slot, opt: ConceptOption) {
    const list = slot === "prefixes" ? prefixes : suffixes;
    commitSlot(slot, [
      ...list,
      { concept: opt.concept, count: 1, min_tier: opt.bestTier, allow_hybrid: true },
    ]);
  }

  function removeSpec(slot: Slot, i: number) {
    const list = slot === "prefixes" ? prefixes : suffixes;
    commitSlot(
      slot,
      list.filter((_, idx) => idx !== i),
    );
  }

  function commitBudget(patch: Partial<DivEquiv>) {
    setGoal({ ...goal, budget: { ...goal.budget, ...patch } });
  }

  const abandons = (goal.abandon_criteria ?? [])
    .map(abandonLabel)
    .filter((l): l is string => l !== null);

  // A render helper (NOT a nested component): called as `specGroup("prefixes")`
  // so it doesn't create a new component identity each render — which would
  // remount the inputs and drop focus while typing a concept.
  function specGroup(slot: Slot) {
    const list = slot === "prefixes" ? prefixes : suffixes;
    const affix: Affix = slot === "prefixes" ? "prefix" : "suffix";
    const options = palette[slot];
    const targeted = new Set(list.map(specConcept));
    const open = options.filter((o) => !targeted.has(o.concept));

    return (
      <section className={styles.group}>
        <div className={styles.groupHead}>
          <span className="eyebrow">
            <span className={styles.dot} data-affix={affix} />
            {affix === "prefix" ? "Prefixes" : "Suffixes"}
          </span>
          <button className="btn btn-ghost" onClick={() => addSpec(slot)}>
            + Add {affix}
          </button>
        </div>

        {/* Concepts this base can actually roll (attribute + ilvl aware). */}
        {open.length > 0 && (
          <div className={styles.palette}>
            {open.map((o) => (
              <button
                key={o.concept}
                className="chip"
                title={`${o.modCount} mods · best tier reachable now: T${o.bestTier} of ${o.tierCount}`}
                onClick={() => addConcept(slot, o)}
              >
                + {humanizeId(o.concept)}
                <span className="tag num">T{o.bestTier}</span>
              </button>
            ))}
          </div>
        )}
        {noPool && (
          <div className={styles.emptyRow}>
            <span className="faint">No mod data for this base — type a concept below.</span>
          </div>
        )}

        {list.length === 0 ? (
          <div className={styles.emptyRow}>
            <span className="faint">No {affix} target.</span>
          </div>
        ) : (
          <div className={styles.rows}>
            {list.map((s, i) => {
              const maxTier = tierCounts.get(`${affix}:${specConcept(s)}`) ?? 15;
              return (
                <div key={i} className={`card ${styles.row}`}>
                  <div className={styles.rowTop}>
                    <input
                      className={`field ${styles.conceptInput}`}
                      list={datalistId(affix)}
                      placeholder="Concept (e.g. EnergyShield). Comma-list = any of…"
                      value={specText(s)}
                      onChange={(e) => editText(slot, i, e.target.value)}
                    />
                    <button
                      className={`btn btn-ghost ${styles.removeBtn}`}
                      onClick={() => removeSpec(slot, i)}
                      aria-label="remove target"
                      title="Remove this target"
                    >
                      ×
                    </button>
                  </div>
                  <div className={styles.rowControls}>
                    <Stepper
                      label="count"
                      value={s.count ?? 1}
                      min={1}
                      max={3}
                      onChange={(v) => updateSpec(slot, i, { count: v })}
                    />
                    <Stepper
                      label="min tier"
                      value={s.min_tier ?? 0}
                      min={0}
                      max={maxTier}
                      placeholder="any"
                      onChange={(v) =>
                        updateSpec(slot, i, { min_tier: v === 0 ? null : v })
                      }
                    />
                    <label className={styles.hybrid}>
                      <input
                        type="checkbox"
                        checked={s.allow_hybrid ?? true}
                        onChange={(e) =>
                          updateSpec(slot, i, { allow_hybrid: e.target.checked })
                        }
                      />
                      <span className="faint">allow hybrid</span>
                    </label>
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </section>
    );
  }

  return (
    <div className="pane">
      <div className="pane-head">
        <div className="pane-title">Target</div>
        {abandons.length > 0 && (
          <div className={styles.abandon}>
            <span className="faint">abandon if</span>
            {abandons.map((a) => (
              <span key={a} className="tag" data-risk="high">
                {a}
              </span>
            ))}
          </div>
        )}
      </div>

      <div className="pane-scroll">
        {/* Suggest a full target — match the item's mods, or a build archetype
            (validated against what this base + ilvl can actually roll). */}
        <div className={`card ${styles.suggest}`}>
          <span className="eyebrow">Suggest a target</span>
          <div className={styles.suggestChips}>
            <button
              className={`chip ${styles.suggestChip}`}
              onClick={seedTargetFromItem}
              disabled={!canSeed}
              title="Target this item's current mods, each at the best tier the base can reach"
            >
              Match current mods
            </button>
            {archetypes.map((a) => (
              <button
                key={a.id}
                className={`chip ${styles.suggestChip} ${styles.archChip}`}
                onClick={() => applyArchetype(a)}
                title={a.description}
              >
                {a.name}
              </button>
            ))}
            {archetypes.length === 0 && !noPool && (
              <span className="faint">No build presets for this base yet.</span>
            )}
          </div>
        </div>

        <datalist id={datalistId("prefix")}>
          {palette.prefixes.map((o) => (
            <option key={o.concept} value={o.concept} />
          ))}
        </datalist>
        <datalist id={datalistId("suffix")}>
          {palette.suffixes.map((o) => (
            <option key={o.concept} value={o.concept} />
          ))}
        </datalist>

        {specGroup("prefixes")}
        {specGroup("suffixes")}

        <section className={styles.group}>
          <div className={styles.groupHead}>
            <span className="eyebrow">Budget · divines</span>
          </div>
          <div className={`card ${styles.budgetCard}`}>
            <div className={styles.budgetRow}>
              <label className={styles.budgetField}>
                <span className="field-label">min</span>
                <input
                  className={`field ${styles.budgetInput} num`}
                  type="number"
                  min={0}
                  step={1}
                  value={goal.budget.min}
                  onChange={(e) => commitBudget({ min: Number(e.target.value) })}
                />
              </label>
              <label className={styles.budgetField}>
                <span className="field-label">expected</span>
                <input
                  className={`field ${styles.budgetInput} num gold`}
                  type="number"
                  min={0}
                  step={1}
                  value={goal.budget.expected}
                  onChange={(e) =>
                    commitBudget({ expected: Number(e.target.value) })
                  }
                />
              </label>
              <label className={styles.budgetField}>
                <span className="field-label">max</span>
                <input
                  className={`field ${styles.budgetInput} num`}
                  type="number"
                  min={0}
                  step={1}
                  value={goal.budget.max}
                  onChange={(e) => commitBudget({ max: Number(e.target.value) })}
                />
              </label>
            </div>
            <p className={`faint ${styles.budgetHint}`}>
              The advisor weighs cost against success within this band.
            </p>
          </div>
        </section>
      </div>
    </div>
  );
}
