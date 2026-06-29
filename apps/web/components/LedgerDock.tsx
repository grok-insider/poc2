"use client";

import { useCraft, totalSpent } from "@/lib/store";
import { div, riskBucket } from "@/lib/format";
import styles from "./LedgerDock.module.css";

export function LedgerDock() {
  const recs = useCraft((s) => s.recommendations);
  const history = useCraft((s) => s.history);
  const openOutcome = useCraft((s) => s.openOutcome);

  const top = recs[0];
  const nextCost = top ? top.expected_cost.expected : 0;
  const spent = totalSpent(history);
  const bucket = top ? riskBucket(top.expected_prob) : "medium";

  return (
    <footer className={`${styles.dock} glass`}>
      <div className={styles.ledger}>
        <span className="faint">spent</span>
        <span className="num gold">{div(spent)}</span>
        <span className="faint">·</span>
        <span className="faint">next</span>
        <span className="num gold">~{div(nextCost)}</span>
        <span className="faint">·</span>
        <span className="faint">projected</span>
        <span className="num gold">~{div(spent + nextCost)}</span>
      </div>
      <div className={styles.right}>
        <div className={styles.risk} aria-label={`risk ${bucket}`}>
          <span className="faint">risk</span>
          <span className={styles.bars} data-risk={bucket}>
            <i /> <i /> <i />
          </span>
        </div>
        <span className="chip num">{history.length} steps</span>
        <button
          className="btn btn-gold"
          onClick={openOutcome}
          title="Record the in-game outcome of a step"
        >
          Record outcome ▾
        </button>
      </div>
    </footer>
  );
}
