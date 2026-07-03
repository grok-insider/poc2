"use client";

/// Bench target card — the craft contract at a glance.
///
/// PoE2-styled per DESIGN.md: each target spec is a structured row
/// (count · mod concept(s) in magic-blue · tier badge · affix side), the
/// budget reads as labeled min/expected/max with a fill meter, and the
/// planner dials (risk / depth) are labeled sliders with qualitative
/// captions — not bare numbers (ui-ux: visible labels, hierarchy by
/// structure not color alone).

import { useCraft } from "@/lib/store";
import { useRegex } from "@/lib/regex/state";
import { humanizeId } from "@/lib/format";
import type { TargetSpec } from "@/lib/types";
import { Crosshair, Pencil, Regex } from "lucide-react";
import styles from "./TargetSummary.module.css";

function riskWord(r: number): string {
  if (r < 0.34) return "cautious";
  if (r < 0.67) return "balanced";
  return "aggressive";
}

function SpecRow({
  spec,
  affix,
  onEdit,
}: {
  spec: TargetSpec;
  affix: "P" | "S";
  onEdit: () => void;
}) {
  const concepts = spec.concept ? [spec.concept] : (spec.concept_any ?? []);
  return (
    <button className={styles.spec} onClick={onEdit} title="Edit target">
      <span className={`${styles.count} num`}>{spec.count ?? 1}×</span>
      <span className={styles.concepts}>
        {concepts.length === 0 ? (
          <span className="faint">any modifier</span>
        ) : (
          concepts.map((c, i) => (
            <span key={c}>
              {i > 0 && <span className={styles.or}> or </span>}
              <span className={styles.concept}>{humanizeId(c)}</span>
            </span>
          ))
        )}
      </span>
      {spec.min_tier ? (
        <span className={styles.tier}>T{spec.min_tier}+</span>
      ) : (
        <span className={`${styles.tier} ${styles.tierAny}`}>any T</span>
      )}
      <span className={styles.affix} data-affix={affix === "P" ? "prefix" : "suffix"}>
        {affix}
      </span>
    </button>
  );
}

export function TargetSummary() {
  const goal = useCraft((s) => s.goal);
  const risk = useCraft((s) => s.risk);
  const depth = useCraft((s) => s.depth);
  const setRisk = useCraft((s) => s.setRisk);
  const setDepth = useCraft((s) => s.setDepth);
  const setSection = useCraft((s) => s.setSection);

  const prefixes = goal.target.prefixes ?? [];
  const suffixes = goal.target.suffixes ?? [];
  const empty = prefixes.length === 0 && suffixes.length === 0;
  const b = goal.budget;
  const expectedPct = Math.min(100, (b.expected / Math.max(1, b.max)) * 100);

  return (
    <div className={`card ${styles.card}`}>
      <div className={styles.head}>
        <span className="poe-section">Target</span>
        <span>
          {!empty && (
            <button
              className="btn btn-ghost"
              onClick={() => {
                useRegex.getState().setTab("goal");
                setSection("regex");
              }}
              title="Stash-search regex for this target"
              aria-label="Stash-search regex for this target"
            >
              <Regex size={13} />
            </button>
          )}
          <button
            className="btn btn-ghost"
            onClick={() => setSection("target")}
            title="Edit the target"
            aria-label="Edit the target"
          >
            <Pencil size={13} />
          </button>
        </span>
      </div>

      {empty ? (
        <button className={styles.empty} onClick={() => setSection("target")}>
          <Crosshair size={16} className="faint" />
          <span className="muted">No target set — define what the item must become.</span>
          <span className={styles.emptyCta}>Set a target ▸</span>
        </button>
      ) : (
        <div className={styles.specs}>
          {prefixes.map((s, i) => (
            <SpecRow key={`p${i}`} spec={s} affix="P" onEdit={() => setSection("target")} />
          ))}
          {suffixes.map((s, i) => (
            <SpecRow key={`s${i}`} spec={s} affix="S" onEdit={() => setSection("target")} />
          ))}
        </div>
      )}

      <div className={styles.budget}>
        <div className={styles.budgetHead}>
          <span className="field-label">Budget</span>
          <span className="num">
            <span className="faint">{b.min}</span>
            <span className="faint"> / </span>
            <span className="gold">{b.expected}</span>
            <span className="faint"> / </span>
            <span className="faint">{b.max}</span>
            <span className="muted"> div</span>
          </span>
        </div>
        <div className={styles.meter} role="presentation">
          <div className={styles.meterFill} style={{ width: `${expectedPct}%` }} />
        </div>
      </div>

      <div className={styles.controls}>
        <label className={styles.control}>
          <span className={styles.controlHead}>
            <span className="field-label">Risk</span>
            <span className={styles.controlValue}>
              <span className="num">{risk.toFixed(2)}</span>
              <span className="faint"> · {riskWord(risk)}</span>
            </span>
          </span>
          <input
            type="range"
            min={0}
            max={1}
            step={0.05}
            value={risk}
            onChange={(e) => setRisk(Number(e.target.value))}
            aria-label={`Risk tolerance ${risk.toFixed(2)} (${riskWord(risk)})`}
          />
        </label>
        <label className={styles.control}>
          <span className={styles.controlHead}>
            <span className="field-label">Depth</span>
            <span className={styles.controlValue}>
              <span className="num">{depth}</span>
              <span className="faint"> · lookahead</span>
            </span>
          </span>
          <input
            type="range"
            min={1}
            max={5}
            step={1}
            value={depth}
            onChange={(e) => setDepth(Number(e.target.value))}
            aria-label={`Planner lookahead depth ${depth}`}
          />
        </label>
      </div>
    </div>
  );
}
