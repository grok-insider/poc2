"use client";

/// Item-mods tab — free selection from the current base's real mod pool
/// (the same eligible pool the Target palette uses). Each wanted mod
/// becomes a no-false-positive fragment term (optionally value-floored);
/// unwanted mods become one negated group.

import { useMemo, useState } from "react";
import { Ban, Search } from "lucide-react";
import { useCraft } from "@/lib/store";
import { humanizeModId, modText } from "@/lib/format";
import { termsForMods, viewLines } from "@/lib/regex/modTerms";
import type { SearchTerm } from "@/lib/regex/searchString";
import { useRegex } from "@/lib/regex/state";
import type { EligibleModView } from "@/lib/types";
import { RegexResult } from "./RegexResult";
import styles from "./RegexPanel.module.css";

function rowMatches(m: EligibleModView, needle: string): boolean {
  if (!needle) return true;
  return (
    (m.name ?? "").toLowerCase().includes(needle) ||
    m.mod_id.toLowerCase().includes(needle) ||
    modText(m.text_template).toLowerCase().includes(needle) ||
    m.concepts.some((c) => c.toLowerCase().includes(needle))
  );
}

export function RegexModsTab() {
  const eligible = useCraft((s) => s.eligible);
  const mods = useRegex((s) => s.mods);
  const setMods = useRegex((s) => s.setMods);
  const [q, setQ] = useState("");

  const pool = useMemo(() => eligible?.mods ?? [], [eligible]);
  const byId = useMemo(() => new Map(pool.map((m) => [m.mod_id, m])), [pool]);

  const rows = useMemo(() => {
    const needle = q.trim().toLowerCase();
    return pool
      .filter((m) => viewLines(m).length > 0)
      .filter((m) => rowMatches(m, needle))
      .slice(0, 200);
  }, [pool, q]);

  const terms: SearchTerm[] = useMemo(() => {
    const out: SearchTerm[] = [];
    for (const id of mods.selected) {
      const view = byId.get(id);
      if (!view) continue;
      const minRaw = mods.minValues[id];
      const min = minRaw !== undefined && minRaw !== "" ? Number(minRaw) : null;
      const r = termsForMods([view], pool, Number.isFinite(min as number) ? min : null);
      if (r.terms.length > 0) {
        out.push({ pattern: r.terms.map((t) => t.pattern).join("|") });
      }
    }
    for (const id of mods.unwanted) {
      const view = byId.get(id);
      if (!view) continue;
      const r = termsForMods([view], pool);
      for (const t of r.terms) out.push({ pattern: t.pattern, negate: true });
    }
    return out;
  }, [mods.selected, mods.unwanted, mods.minValues, byId, pool]);

  function cycle(id: string) {
    // none → wanted → none (the ban button handles unwanted).
    if (mods.selected.includes(id)) {
      setMods({ selected: mods.selected.filter((x) => x !== id) });
    } else {
      setMods({
        selected: [...mods.selected, id],
        unwanted: mods.unwanted.filter((x) => x !== id),
      });
    }
  }

  function toggleBan(id: string) {
    if (mods.unwanted.includes(id)) {
      setMods({ unwanted: mods.unwanted.filter((x) => x !== id) });
    } else {
      setMods({
        unwanted: [...mods.unwanted, id],
        selected: mods.selected.filter((x) => x !== id),
      });
    }
  }

  function setMin(id: string, value: string) {
    setMods({ minValues: { ...mods.minValues, [id]: value } });
  }

  return (
    <div className={styles.stack}>
      <RegexResult terms={terms} mode={mods.mode} />

      <section className={`card ${styles.section}`}>
        <div className={styles.sectionHead}>
          <span className="eyebrow">Mods on this base</span>
          <div className="seg">
            <button
              className={mods.mode === "all" ? "on" : ""}
              onClick={() => setMods({ mode: "all" })}
              title="Item must have EVERY selected mod"
            >
              all
            </button>
            <button
              className={mods.mode === "any" ? "on" : ""}
              onClick={() => setMods({ mode: "any" })}
              title="Item may have ANY selected mod"
            >
              any
            </button>
          </div>
        </div>

        <div className={styles.search}>
          <Search size={13} className="faint" />
          <input
            placeholder="Filter mods…"
            value={q}
            onChange={(e) => setQ(e.target.value)}
            spellCheck={false}
          />
        </div>

        {!eligible && <p className="faint">Waiting for the base&apos;s mod pool…</p>}

        <div className={styles.list}>
          {rows.map((m) => {
            const state = mods.selected.includes(m.mod_id)
              ? "wanted"
              : mods.unwanted.includes(m.mod_id)
                ? "unwanted"
                : "none";
            const hasValue = m.stats.length > 0;
            return (
              <div
                key={m.mod_id}
                className={styles.row}
                data-state={state}
                onClick={() => cycle(m.mod_id)}
                title={modText(m.text_template)}
              >
                <span className={styles.rowName}>
                  {m.name ?? humanizeModId(m.mod_id)}
                  <span className="faint"> · T{m.tier_index}</span>
                </span>
                <span className={styles.rowText}>{modText(m.text_template)}</span>
                <span className="tag">{m.affix_type}</span>
                {state === "wanted" && hasValue && (
                  <input
                    className={`field num ${styles.minInput}`}
                    placeholder="min"
                    value={mods.minValues[m.mod_id] ?? ""}
                    onClick={(e) => e.stopPropagation()}
                    onChange={(e) => setMin(m.mod_id, e.target.value)}
                    inputMode="numeric"
                  />
                )}
                <button
                  className={`btn btn-ghost ${styles.banBtn}`}
                  data-risk="high"
                  onClick={(e) => {
                    e.stopPropagation();
                    toggleBan(m.mod_id);
                  }}
                  title={state === "unwanted" ? "Remove from unwanted" : "Item must NOT have this mod"}
                >
                  <Ban size={12} />
                </button>
              </div>
            );
          })}
        </div>
      </section>
    </div>
  );
}
