"use client";

import { useCraft, totalSpent } from "@/lib/store";
import { div, humanizeId } from "@/lib/format";
import type { HistoryEntry } from "@/lib/types";
import styles from "./HistoryTab.module.css";

type RiskLevel = "low" | "medium" | "high";

/** Map a change kind to a badge label + risk-color bucket. */
function changeBadge(change: string): { label: string; risk: RiskLevel } {
  switch (change) {
    case "added":
      return { label: "added", risk: "low" };
    case "rerolled":
      return { label: "rerolled", risk: "low" };
    case "removed":
      return { label: "removed", risk: "high" };
    case "replaced":
      return { label: "replaced", risk: "medium" };
    case "rarity":
      return { label: "rarity", risk: "medium" };
    case "sanctified":
      return { label: "sanctified", risk: "medium" };
    default:
      return { label: change || "step", risk: "medium" };
  }
}

function timeLabel(ts: string): string {
  const d = new Date(ts);
  if (Number.isNaN(d.getTime())) return ts;
  return d.toLocaleTimeString();
}

function Entry({ entry, recent }: { entry: HistoryEntry; recent: boolean }) {
  const badge = changeBadge(entry.change);
  const materials = entry.materials ?? [];
  return (
    <div className={`card ${styles.entry} ${recent ? styles.recent : ""}`}>
      <div className={styles.top}>
        <span className={styles.badge} data-risk={badge.risk}>
          {badge.label}
        </span>
        <span className={styles.explanation}>{entry.explanation}</span>
        <span className={`faint num ${styles.time}`}>{timeLabel(entry.timestamp)}</span>
      </div>

      {(entry.action_label || entry.cost_div != null || materials.length > 0) && (
        <div className={styles.meta}>
          {entry.action_label && <span className="chip">{entry.action_label}</span>}
          {entry.cost_div != null && (
            <span className="num gold">{div(entry.cost_div)}</span>
          )}
          {materials.map((m, i) => (
            <span key={`${m.id}${i}`} className="tag num">
              {m.quantity}× {humanizeId(m.id)}
            </span>
          ))}
        </div>
      )}
    </div>
  );
}

export function HistoryTab() {
  const history = useCraft((s) => s.history);
  const undo = useCraft((s) => s.undo);
  const clearHistory = useCraft((s) => s.clearHistory);

  const spent = totalSpent(history);
  const empty = history.length === 0;

  function onClear() {
    if (empty) return;
    if (window.confirm("Clear the entire crafting history? This cannot be undone.")) {
      clearHistory();
    }
  }

  return (
    <div className="pane">
      <div className="pane-head">
        <div className="pane-title">History</div>
        <div className={styles.actions}>
          <span className={styles.spent}>
            <span className="faint">spent</span>
            <span className="num gold">{div(spent)}</span>
          </span>
          <button className="btn" onClick={undo} disabled={empty} title="Undo the last recorded step">
            Undo last
          </button>
          <button
            className="btn btn-ghost"
            onClick={onClear}
            disabled={empty}
            title="Clear the crafting history"
          >
            Clear
          </button>
        </div>
      </div>

      <div className="pane-scroll">
        {empty ? (
          <div className="empty-state">
            <span className="eyebrow">Ledger</span>
            <span className="muted">No steps recorded yet. Use Record outcome to log a craft.</span>
          </div>
        ) : (
          <div className={styles.list}>
            {history.map((entry, i) => (
              <Entry key={entry.id} entry={entry} recent={i === 0} />
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
