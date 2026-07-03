"use client";

/// Shared result bar for the Regex panel tabs: the assembled in-game
/// search string, a 250-char budget meter, copy / auto-copy, and a
/// free-form custom-text suffix.

import { useEffect, useMemo, useState } from "react";
import { Copy, Eraser } from "lucide-react";
import {
  assembleSearch,
  MAX_SEARCH_LENGTH,
  type SearchTerm,
} from "@/lib/regex/searchString";
import { useRegex } from "@/lib/regex/state";
import styles from "./RegexPanel.module.css";

export function RegexResult({
  terms,
  mode,
}: {
  terms: SearchTerm[];
  mode: "all" | "any";
}) {
  const customText = useRegex((s) => s.customText);
  const setCustomText = useRegex((s) => s.setCustomText);
  const autoCopy = useRegex((s) => s.autoCopy);
  const setAutoCopy = useRegex((s) => s.setAutoCopy);
  const resetTab = useRegex((s) => s.resetTab);
  const [copied, setCopied] = useState<string | null>(null);

  const assembled = useMemo(
    () => assembleSearch(terms, mode, customText),
    [terms, mode, customText],
  );

  useEffect(() => {
    if (!autoCopy || assembled.value === "" || assembled.overBudget) return;
    navigator.clipboard
      .writeText(assembled.value)
      .then(() => setCopied(assembled.value))
      .catch(() => {});
  }, [assembled.value, assembled.overBudget, autoCopy]);

  function copy() {
    if (assembled.value === "") return;
    navigator.clipboard
      .writeText(assembled.value)
      .then(() => setCopied(assembled.value))
      .catch(() => {});
  }

  return (
    <section className={`card ${styles.section}`}>
      <div className={styles.sectionHead}>
        <span className="eyebrow">Search string</span>
        <div className={styles.resultActions}>
          <label className={styles.check} data-on={autoCopy}>
            <input
              type="checkbox"
              checked={autoCopy}
              onChange={(e) => setAutoCopy(e.target.checked)}
            />
            auto-copy
          </label>
          <button
            className="btn"
            onClick={copy}
            disabled={assembled.value === ""}
            title="Copy to clipboard — paste into the in-game search box"
          >
            <Copy size={13} /> Copy
          </button>
          <button className="btn btn-ghost" onClick={resetTab} title="Clear this tab">
            <Eraser size={13} />
          </button>
        </div>
      </div>

      <div className={styles.result}>
        <div
          className={styles.resultString}
          data-copied={copied === assembled.value && assembled.value !== ""}
        >
          {assembled.value === "" ? (
            <span className="faint">— select something below —</span>
          ) : (
            assembled.value
          )}
        </div>
        <div className={styles.resultMeta}>
          <span className={`num ${assembled.overBudget ? styles.lengthOver : "faint"}`}>
            {assembled.length} / {MAX_SEARCH_LENGTH}
          </span>
          {assembled.overBudget && (
            <span className={styles.lengthOver}>
              over the game&apos;s limit — deselect something
            </span>
          )}
          <input
            className={`field ${styles.customText}`}
            placeholder="Custom text…"
            value={customText}
            onChange={(e) => setCustomText(e.target.value)}
            spellCheck={false}
          />
        </div>
      </div>
    </section>
  );
}
