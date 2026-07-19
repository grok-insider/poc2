"use client";

import { useCraft } from "@/lib/store";
import {
  actionKindLabel,
  actionLabel,
  div,
  pct,
  riskBucket,
  sourceLabel,
} from "@/lib/format";
import type { Recommendation } from "@/lib/types";
import { RecoveryPanel } from "@/components/RecoveryPanel";
import styles from "./GuidePanel.module.css";

function SuccessBand({ rec }: { rec: Recommendation }) {
  const p = rec.expected_prob;
  const se = rec.prob_stderr || 0;
  const lo = Math.max(0, p - se) * 100;
  const hi = Math.min(1, p + se) * 100;
  return (
    <div className={styles.band} title={`P(reach goal) Monte-Carlo band: ${lo.toFixed(0)}–${hi.toFixed(0)}%`}>
      <div className={styles.bandFill} style={{ width: `${p * 100}%` }} />
      <div
        className={styles.bandRange}
        style={{ left: `${lo}%`, width: `${Math.max(1, hi - lo)}%` }}
      />
    </div>
  );
}

/** Structural goal completeness — how many target specs the planned item
 * satisfies. Distinct from {@link SuccessBand} (which folds in execution
 * reliability): a short band under a fuller specs bar means "you're nearly
 * there, but the plan to get there is risky". */
function SpecsProgress({ progress, total }: { progress: number; total: number }) {
  if (!total) return null;
  const clamped = Math.max(0, Math.min(1, progress ?? 0));
  const met = Math.round(clamped * total);
  return (
    <div className={styles.specs} title="Goal specs the planned item satisfies">
      <div className={styles.specsBar}>
        <div className={styles.specsFill} style={{ width: `${clamped * 100}%` }} />
      </div>
      <span className={styles.specsCaption}>
        {met}/{total} specs
      </span>
    </div>
  );
}

function Hero({ rec }: { rec: Recommendation }) {
  const bucket = riskBucket(rec.expected_prob);
  const openOutcome = useCraft((s) => s.openOutcome);
  const goal = useCraft((s) => s.goal);
  const total =
    (goal.target.prefixes?.length ?? 0) + (goal.target.suffixes?.length ?? 0);
  return (
    <div className={`card ${styles.hero}`}>
      <div className={styles.heroTop}>
        <span className="eyebrow" style={{ color: "var(--primary)" }}>
          Next best
        </span>
        <div className={styles.heroProb}>
          <span className={styles.probLabel}>P(reach goal)</span>
          <span className={`${styles.success} num`} data-risk={bucket}>
            {pct(rec.expected_prob)}
          </span>
        </div>
      </div>
      <div className={styles.action}>{actionLabel(rec.action)}</div>
      <SpecsProgress progress={rec.goal_progress} total={total} />
      <SuccessBand rec={rec} />
      <div className={styles.metaline}>
        <span className="chip">{actionKindLabel(rec.action)}</span>
        <span className="gold num">~{div(rec.expected_cost)}</span>
        <span className="faint num">depth {rec.depth}</span>
        <span className="chip" title="source">
          via {sourceLabel(rec)}
        </span>
      </div>
      {rec.rationale && <p className={styles.rationale}>{rec.rationale}</p>}
      <div className={styles.heroActions}>
        <button
          className="btn btn-gold"
          onClick={openOutcome}
          title="Record the in-game outcome of this step"
        >
          Apply ▸
        </button>
      </div>
    </div>
  );
}

function Alt({ rec, i }: { rec: Recommendation; i: number }) {
  return (
    <button className={styles.alt} title={rec.rationale ?? undefined}>
      <span className="faint num">{i + 1}</span>
      <span className={styles.altLabel}>{actionLabel(rec.action)}</span>
      <span className="num muted" data-risk={riskBucket(rec.expected_prob)}>
        {pct(rec.expected_prob)}
      </span>
      <span className="num gold">~{div(rec.expected_cost)}</span>
    </button>
  );
}

export function GuidePanel() {
  const recs = useCraft((s) => s.recommendations);
  const planning = useCraft((s) => s.planning);
  const error = useCraft((s) => s.error);

  if (error) {
    return (
      <div className={`card ${styles.state}`}>
        <span className="eyebrow danger">Planning error</span>
        <pre className="mono" style={{ fontSize: 11, whiteSpace: "pre-wrap" }}>
          {error}
        </pre>
      </div>
    );
  }

  if (recs.length === 0) {
    return planning ? (
      <div className={`card ${styles.hero}`}>
        <div className="skeleton" style={{ height: 12, width: 120 }} />
        <div className="skeleton" style={{ height: 22, width: "70%", marginTop: 10 }} />
        <div className="skeleton" style={{ height: 8, width: "100%", marginTop: 12 }} />
      </div>
    ) : (
      <div className={`card ${styles.state}`}>
        <span className="muted">No recommendation — the goal may be satisfied.</span>
      </div>
    );
  }

  const [hero, ...alts] = recs;
  return (
    <div className={styles.wrap}>
      <Hero rec={hero} />
      {alts.length > 0 && (
        <div className={styles.alts}>
          <div className="section-title" style={{ padding: "2px 2px 4px" }}>
            Alternatives
          </div>
          {alts.map((r, i) => (
            <Alt key={i} rec={r} i={i + 1} />
          ))}
        </div>
      )}
      <RecoveryPanel />
    </div>
  );
}
