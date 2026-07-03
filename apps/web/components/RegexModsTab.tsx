"use client";

/// Pool-picker tabs for the Regex panel.
///
/// `RegexPoolCore` is the shared machinery: pick wanted/unwanted mods
/// out of an engine pool, per-mod roll floors, AND/OR modes — each
/// wanted mod becomes a no-false-positive fragment term, unwanted mods
/// one negated group. Three wrappers feed it:
///   - Item mods:  the current base's cached eligible pool
///   - Waystone:   class "Map" (the 0.5 data-gap pool)
///   - Tablet:     class "TowerAugmentation" (Precursor Tablets)

import { useEffect, useMemo, useState } from "react";
import { Ban, Search } from "lucide-react";
import { useCraft } from "@/lib/store";
import { humanizeModId, modText } from "@/lib/format";
import { termsForMods, viewLines } from "@/lib/regex/modTerms";
import type { SearchTerm } from "@/lib/regex/searchString";
import { useRegex, type PoolSlice } from "@/lib/regex/state";
import type { EligibleModView, EligibleModsResponse, Item } from "@/lib/types";
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

function RegexPoolCore({
  slice,
  pool,
  title,
  emptyHint,
}: {
  slice: PoolSlice;
  pool: EligibleModView[] | null;
  title: string;
  emptyHint: string;
}) {
  const selection = useRegex((s) => s[slice]);
  const setSelection = useRegex((s) => s.setSelection);
  const [q, setQ] = useState("");

  const mods = useMemo(() => pool ?? [], [pool]);
  const byId = useMemo(() => new Map(mods.map((m) => [m.mod_id, m])), [mods]);

  const rows = useMemo(() => {
    const needle = q.trim().toLowerCase();
    return mods
      .filter((m) => viewLines(m).length > 0)
      .filter((m) => rowMatches(m, needle))
      .slice(0, 200);
  }, [mods, q]);

  const terms: SearchTerm[] = useMemo(() => {
    const out: SearchTerm[] = [];
    for (const id of selection.selected) {
      const view = byId.get(id);
      if (!view) continue;
      const minRaw = selection.minValues[id];
      const min = minRaw !== undefined && minRaw !== "" ? Number(minRaw) : null;
      const r = termsForMods([view], mods, Number.isFinite(min as number) ? min : null);
      if (r.terms.length > 0) {
        out.push({ pattern: r.terms.map((t) => t.pattern).join("|") });
      }
    }
    for (const id of selection.unwanted) {
      const view = byId.get(id);
      if (!view) continue;
      const r = termsForMods([view], mods);
      for (const t of r.terms) out.push({ pattern: t.pattern, negate: true });
    }
    return out;
  }, [selection.selected, selection.unwanted, selection.minValues, byId, mods]);

  function cycle(id: string) {
    if (selection.selected.includes(id)) {
      setSelection(slice, { selected: selection.selected.filter((x) => x !== id) });
    } else {
      setSelection(slice, {
        selected: [...selection.selected, id],
        unwanted: selection.unwanted.filter((x) => x !== id),
      });
    }
  }

  function toggleBan(id: string) {
    if (selection.unwanted.includes(id)) {
      setSelection(slice, { unwanted: selection.unwanted.filter((x) => x !== id) });
    } else {
      setSelection(slice, {
        unwanted: [...selection.unwanted, id],
        selected: selection.selected.filter((x) => x !== id),
      });
    }
  }

  function setMin(id: string, value: string) {
    setSelection(slice, { minValues: { ...selection.minValues, [id]: value } });
  }

  return (
    <div className={styles.stack}>
      <RegexResult terms={terms} mode={selection.mode} />

      <section className={`card ${styles.section}`}>
        <div className={styles.sectionHead}>
          <span className="eyebrow">{title}</span>
          <div className="seg">
            <button
              className={selection.mode === "all" ? "on" : ""}
              onClick={() => setSelection(slice, { mode: "all" })}
              title="Item must have EVERY selected mod"
            >
              all
            </button>
            <button
              className={selection.mode === "any" ? "on" : ""}
              onClick={() => setSelection(slice, { mode: "any" })}
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

        {!pool && <p className="faint">{emptyHint}</p>}
        {pool && mods.length === 0 && <p className="faint">{emptyHint}</p>}

        <div className={styles.list}>
          {rows.map((m) => {
            const state = selection.selected.includes(m.mod_id)
              ? "wanted"
              : selection.unwanted.includes(m.mod_id)
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
                    value={selection.minValues[m.mod_id] ?? ""}
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
                  title={
                    state === "unwanted" ? "Remove from unwanted" : "Item must NOT have this mod"
                  }
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

/** Item-mods tab: the current base's cached bare-item pool. */
export function RegexModsTab() {
  const eligible = useCraft((s) => s.eligible);
  return (
    <RegexPoolCore
      slice="mods"
      pool={eligible?.mods ?? null}
      title="Mods on this base"
      emptyHint="Waiting for the base's mod pool…"
    />
  );
}

/** A bare item of the given class (placeholder base-id convention — the
 * engine resolves the class directly when the id isn't a bundle base). */
function bareItemOfClass(classId: string, ilvl: number): Item {
  return {
    base: classId,
    ilvl,
    rarity: "normal",
    corrupted: false,
    sanctified: false,
    mirrored: false,
    quality: 0,
    quality_kind: "Untagged",
    implicits: [],
    prefixes: [],
    suffixes: [],
    enchantments: [],
    hidden_desecrated: null,
    sockets: [],
    hinekora_lock: null,
  };
}

/** Fetch-once pool tab for a non-gear surface (waystones / tablets). */
function RegexSurfaceTab({
  slice,
  classId,
  ilvl,
  title,
  emptyHint,
}: {
  slice: PoolSlice;
  classId: string;
  ilvl: number;
  title: string;
  emptyHint: string;
}) {
  const engineReady = useCraft((s) => s.engineReady);
  const [resp, setResp] = useState<EligibleModsResponse | null>(null);

  useEffect(() => {
    if (!engineReady) return;
    let live = true;
    void (async () => {
      try {
        const { engine } = await import("@/lib/engine/client");
        const r = await engine.eligibleMods(bareItemOfClass(classId, ilvl), "either", 0);
        if (live) setResp(r);
      } catch {
        if (live) setResp(null);
      }
    })();
    return () => {
      live = false;
    };
  }, [engineReady, classId, ilvl]);

  return (
    <RegexPoolCore
      slice={slice}
      pool={resp?.mods ?? null}
      title={title}
      emptyHint={emptyHint}
    />
  );
}

/** Waystone tab — the 0.5 data-gap pool (class "Map", tier-16 area level). */
export function RegexWaystoneTab() {
  return (
    <RegexSurfaceTab
      slice="waystone"
      classId="Map"
      ilvl={80}
      title="Waystone modifiers"
      emptyHint="No waystone pool in this bundle — rebuild it with the current pipeline."
    />
  );
}

/** Precursor Tablet tab. */
export function RegexTabletTab() {
  return (
    <RegexSurfaceTab
      slice="tablet"
      classId="TowerAugmentation"
      ilvl={80}
      title="Tablet modifiers"
      emptyHint="No tablet pool in this bundle — rebuild it with the current pipeline."
    />
  );
}