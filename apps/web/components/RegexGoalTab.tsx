"use client";

/// Goal tab — the poc2-specific trick: turn the current craft target
/// into a stash-search string. Paste it into the stash (or hold Ctrl+F
/// in game) and a finished item lights up after every roll session.

import { useMemo } from "react";
import { useCraft } from "@/lib/store";
import { termsForSpec, type SpecTermResult } from "@/lib/regex/modTerms";
import type { SearchTerm } from "@/lib/regex/searchString";
import { RegexResult } from "./RegexResult";
import styles from "./RegexPanel.module.css";

export function RegexGoalTab() {
  const goal = useCraft((s) => s.goal);
  const eligible = useCraft((s) => s.eligible);

  const specResults: SpecTermResult[] = useMemo(() => {
    if (!eligible) return [];
    const pool = eligible.mods;
    return [
      ...(goal.target.prefixes ?? []).map((spec) => termsForSpec(spec, "prefix", pool)),
      ...(goal.target.suffixes ?? []).map((spec) => termsForSpec(spec, "suffix", pool)),
    ];
  }, [goal, eligible]);

  // One quoted term per spec (its patterns OR-merged): the item matches
  // when EVERY spec is present — i.e. "the craft is done".
  const terms: SearchTerm[] = useMemo(
    () =>
      specResults
        .filter((r) => r.terms.length > 0)
        .map((r) => ({ pattern: r.terms.map((t) => t.pattern).join("|") })),
    [specResults],
  );

  const hasCounts = [
    ...(goal.target.prefixes ?? []),
    ...(goal.target.suffixes ?? []),
  ].some((s) => (s.count ?? 1) > 1);

  return (
    <div className={styles.stack}>
      <RegexResult terms={terms} mode="all" />

      <section className={`card ${styles.section}`}>
        <div className={styles.sectionHead}>
          <span className="eyebrow">Target terms</span>
          <span className="faint num">{specResults.length} specs</span>
        </div>

        {!eligible && <p className="faint">Waiting for the base&apos;s mod pool…</p>}
        {eligible && specResults.length === 0 && (
          <p className="faint">
            No target specs — define your goal in the Target panel first.
          </p>
        )}

        {specResults.map((r, i) => (
          <div key={i} className={styles.termRow}>
            <div className={styles.termHead}>
              <span className="gold">{r.label}</span>
              {!r.exact && <span className="tag" data-risk="high">approx</span>}
            </div>
            {r.terms.length > 0 && (
              <div className={styles.termPattern}>
                {r.terms.map((t) => t.pattern).join(" | ")}
              </div>
            )}
            {r.warnings.map((w, j) => (
              <div key={j} className={styles.termWarn}>
                {w}
              </div>
            ))}
          </div>
        ))}

        {hasCounts && (
          <p className={`faint ${styles.termWarn}`}>
            Note: search strings can&apos;t count occurrences — a &quot;3× Energy
            Shield&quot; spec highlights items with at least one qualifying mod.
          </p>
        )}
      </section>
    </div>
  );
}
