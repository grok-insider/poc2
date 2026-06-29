"use client";

import { useEffect, useState } from "react";
import { LifeBuoy } from "lucide-react";
import { useCraft } from "@/lib/store";
import { engine } from "@/lib/engine/client";
import { div } from "@/lib/format";
import type { RecoveryStepView } from "@/lib/types";
import styles from "./RecoveryPanel.module.css";

/// Recovery hints for the current step. Only the strategy-sourced
/// recommendations carry recovery flows, so this renders nothing for
/// rule/heuristic recommendations.
export function RecoveryPanel() {
  const recs = useCraft((s) => s.recommendations);
  const top = recs[0];
  const source = top?.source;
  const strategyId = source?.kind === "strategy" ? source.id : null;
  const stepId = source?.kind === "strategy" ? source.step : null;

  const [view, setView] = useState<RecoveryStepView | null>(null);
  const [open, setOpen] = useState(false);

  useEffect(() => {
    if (!strategyId || !stepId) {
      setView(null);
      return;
    }
    let live = true;
    engine
      .recoveryHints(strategyId, stepId)
      .then((v) => live && setView(v))
      .catch(() => live && setView(null));
    return () => {
      live = false;
    };
  }, [strategyId, stepId]);

  if (!view || view.hints.length === 0) return null;

  return (
    <div className={`card ${styles.wrap}`}>
      <button className={styles.head} onClick={() => setOpen((o) => !o)} aria-expanded={open}>
        <LifeBuoy size={14} className="faint" />
        <span className="section-title">If this step fails…</span>
        <span className="chip num">{view.hints.length}</span>
        <span className="faint" style={{ marginLeft: "auto" }}>
          {open ? "▾" : "▸"}
        </span>
      </button>
      {open && (
        <div className={styles.body}>
          {view.next_action_summary && (
            <div className={styles.fallback}>
              <span className="eyebrow">Default fallback</span>
              <span className="muted">{view.next_action_summary}</span>
            </div>
          )}
          {view.hints.map((h, i) => (
            <div key={i} className={styles.hint}>
              <span className={styles.dot} />
              <span className={styles.msg}>{h.message}</span>
              {h.added_cost_div != null && (
                <span className="num gold">+{div(h.added_cost_div)}</span>
              )}
              {h.goto_step_id && <span className="tag">→ {h.goto_step_id}</span>}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
