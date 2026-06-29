"use client";

import { useEffect, useMemo, useState } from "react";
import { Search } from "lucide-react";
import { useCraft } from "@/lib/store";
import { engine, type AffixSlot } from "@/lib/engine/client";
import { humanizeId, pct } from "@/lib/format";
import type { EligibleModView, EligibleModsResponse } from "@/lib/types";
import styles from "./EligibleTab.module.css";

const MAX_ROWS = 300;
const AFFIXES: AffixSlot[] = ["either", "prefix", "suffix"];

function matches(m: EligibleModView, needle: string): boolean {
  if (!needle) return true;
  return (
    (m.name ?? "").toLowerCase().includes(needle) ||
    m.mod_id.toLowerCase().includes(needle) ||
    m.concepts.some((c) => c.toLowerCase().includes(needle)) ||
    m.tags.some((t) => t.toLowerCase().includes(needle))
  );
}

function StatusCell({ m }: { m: EligibleModView }) {
  if (m.eligible_now) {
    return <span className={`${styles.status} faint`}>ok</span>;
  }
  return (
    <span className={styles.status}>
      {m.blocked_by_min_level && (
        <span className="tag" data-risk="high" title={`requires item level ${m.required_level}`}>
          ilvl
        </span>
      )}
      {m.blocked_by_group && (
        <span className="tag" data-risk="high" title={`mod group "${m.mod_group}" is full`}>
          slot full
        </span>
      )}
      {!m.blocked_by_min_level && !m.blocked_by_group && (
        <span className="tag" data-risk="high">
          blocked
        </span>
      )}
    </span>
  );
}

function Flags({ m }: { m: EligibleModView }) {
  const flags: { key: string; label: string; title: string }[] = [];
  if (m.is_hybrid) flags.push({ key: "hy", label: "hybrid", title: "rolls on multiple stats" });
  if (m.is_essence_only)
    flags.push({ key: "es", label: "essence", title: "essence-only mod" });
  if (m.is_desecrated_only)
    flags.push({ key: "ds", label: "desecrated", title: "desecrated-only mod" });
  if (m.is_local) flags.push({ key: "lo", label: "local", title: "local mod" });
  if (flags.length === 0) return null;
  return (
    <span className={styles.flags}>
      {flags.map((f) => (
        <span key={f.key} className="tag" title={f.title}>
          {f.label}
        </span>
      ))}
    </span>
  );
}

function ModRow({ m }: { m: EligibleModView }) {
  return (
    <div className={`${styles.row} ${m.eligible_now ? "" : styles.blocked}`}>
      <span className={styles.dot} data-affix={m.affix_type} title={m.affix_type} />
      <span className={styles.name} title={m.mod_id}>
        {m.name ?? humanizeId(m.mod_id)}
      </span>
      <Flags m={m} />
      <span className="tag" title={`tier ${m.tier_index} of ${m.tier_count}`}>
        T{m.tier_index}/{m.tier_count}
      </span>
      <span className={`${styles.weight} num gold`} title="weight share">
        {pct(m.weight_share)}
      </span>
      <span className={`${styles.ilvl} num faint`} title="required item level">
        iL{m.required_level}
      </span>
      <StatusCell m={m} />
    </div>
  );
}

export function EligibleTab() {
  const item = useCraft((s) => s.item);
  const [affix, setAffix] = useState<AffixSlot>("either");
  const [q, setQ] = useState("");
  const [resp, setResp] = useState<EligibleModsResponse | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let live = true;
    setLoading(true);
    setError(null);
    engine
      .eligibleMods(item, affix, 0)
      .then((r) => {
        if (!live) return;
        setResp(r);
        setLoading(false);
      })
      .catch((e: unknown) => {
        if (!live) return;
        setError(String(e));
        setLoading(false);
      });
    return () => {
      live = false;
    };
  }, [item, affix]);

  const sorted = useMemo(() => {
    if (!resp) return [];
    return [...resp.mods].sort((a, b) => b.weight - a.weight);
  }, [resp]);

  const filtered = useMemo(() => {
    const needle = q.trim().toLowerCase();
    return sorted.filter((m) => matches(m, needle));
  }, [sorted, q]);

  const total = filtered.length;
  const rows = total > MAX_ROWS ? filtered.slice(0, MAX_ROWS) : filtered;
  const truncated = total > MAX_ROWS;

  return (
    <div className="pane">
      <div className="pane-head">
        <div className="pane-title">Eligible mods</div>
        {resp && (
          <div className={styles.headMeta}>
            <span className="chip" title="item class">
              {resp.item_class}
            </span>
            <span className="tag num" title="bundle patch">
              {resp.patch}
            </span>
          </div>
        )}
      </div>

      <div className={styles.controls}>
        <div className="seg">
          {AFFIXES.map((a) => (
            <button key={a} className={affix === a ? "on" : ""} onClick={() => setAffix(a)}>
              {a}
            </button>
          ))}
        </div>
        <div className={styles.search}>
          <Search size={13} className="faint" />
          <input
            className="field"
            placeholder="Search name, id, concept or tag…"
            value={q}
            onChange={(e) => setQ(e.target.value)}
          />
        </div>
      </div>

      <div className="pane-scroll">
        {loading ? (
          <div className={styles.list}>
            {Array.from({ length: 8 }, (_, i) => (
              <div key={i} className="skeleton" style={{ height: 30 }} />
            ))}
          </div>
        ) : error ? (
          <div className="empty-state">
            <span className="eyebrow danger">Inspect error</span>
            <span className="muted mono" style={{ fontSize: 11 }}>
              {error}
            </span>
          </div>
        ) : !resp || resp.data_available === false ? (
          <div className="empty-state">
            <span className="muted">No mod data for this base class.</span>
            <span className="faint">
              The bundle has no eligible-mod table for {resp?.item_class ?? "this item"}.
            </span>
          </div>
        ) : total === 0 ? (
          <div className="empty-state">
            <span className="muted">No mods match your filter.</span>
          </div>
        ) : (
          <>
            <div className={styles.summary}>
              <span className="eyebrow">
                {truncated ? `showing ${rows.length} of ${total}` : `${total} mods`}
              </span>
              <span className="faint">sorted by weight</span>
            </div>
            <div className={styles.list}>
              {rows.map((m, i) => (
                <ModRow key={`${m.affix_type}:${m.mod_id}:${i}`} m={m} />
              ))}
            </div>
          </>
        )}
      </div>
    </div>
  );
}
