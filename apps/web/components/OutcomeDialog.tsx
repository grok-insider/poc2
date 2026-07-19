"use client";

import { useEffect, useMemo, useState } from "react";
import { X, Search, ChevronDown } from "lucide-react";
import { useCraft } from "@/lib/store";
import { engine } from "@/lib/engine/client";
import { actionLabel, div, humanizeId, modLines, modText } from "@/lib/format";
import type {
  CannotApplyView,
  EligibleModView,
  Rarity,
  RecordOutcome,
  RerollableMod,
} from "@/lib/types";
import styles from "./OutcomeDialog.module.css";

type Mode = "add" | "remove" | "reroll" | "rarity";

const MODES: { id: Mode; label: string }[] = [
  { id: "add", label: "Add mod" },
  { id: "remove", label: "Remove" },
  { id: "reroll", label: "Reroll" },
  { id: "rarity", label: "Rarity" },
];

function cannotLabel(v: CannotApplyView): string {
  switch (v.kind) {
    case "ok":
      return "can apply";
    case "wrong_rarity":
      return `needs ${v.expected.join(" / ")} (is ${v.item_rarity})`;
    case "no_open_slots":
      return `no open ${v.affix} slot`;
    case "corrupted":
      return "item is corrupted";
    case "mirrored":
      return "item is mirrored";
    case "already_locked":
      return "already locked";
    case "fracture_requires_four_mods":
      return `needs 4 mods (has ${v.current})`;
    case "recombinator_input_mismatch":
      return "recombinator input mismatch";
    case "unknown_currency":
      return "unknown currency";
    default:
      return v.kind === "other" ? v.message : "cannot apply";
  }
}

/* ---------- Add mod ----------------------------------------------------- */

/** Eligible mods of one mod-group (its tier ladder), labelled by stat text. */
type ModGroupView = {
  key: string;
  label: string; // full stat text, for search + the picked row
  primary: string; // the distinguishing stat line (rarest across the pool)
  secondary: string[]; // shared / common lines, shown dimmed
  affix_type: EligibleModView["affix_type"];
  concepts: string[];
  tiers: EligibleModView[]; // sorted T1 first
};

/** The roll range(s) for a single tier, e.g. "80–91" or "60–80 / 12–18". */
function tierRange(m: EligibleModView): string {
  if (!m.stats?.length) return "—";
  return m.stats.map((s) => `${s.min}–${s.max}`).join(" / ");
}

function modGroupLabel(m: EligibleModView): string {
  return modText(m.text_template) || m.name || humanizeId(m.mod_id);
}

function AddMode() {
  const item = useCraft((s) => s.item);
  const apply = useCraft((s) => s.applyOutcome);
  const [affix, setAffix] = useState<"prefix" | "suffix" | "either">("either");
  const [q, setQ] = useState("");
  const [mods, setMods] = useState<EligibleModView[]>([]);
  const [loading, setLoading] = useState(true);
  const [expanded, setExpanded] = useState<string | null>(null);
  const [picked, setPicked] = useState<EligibleModView | null>(null);
  const [roll, setRoll] = useState<number>(0);

  useEffect(() => {
    let live = true;
    setLoading(true);
    setExpanded(null);
    setPicked(null);
    engine
      .eligibleMods(item, affix, 0)
      .then((r) => {
        if (!live) return;
        setMods(r.mods.filter((m) => m.eligible_now));
        setLoading(false);
      })
      .catch(() => live && setLoading(false));
    return () => {
      live = false;
    };
  }, [item, affix]);

  // Collapse the per-tier rows into one entry per mod-group, labelled by the
  // human stat text, with the tier ladder kept for the picker.
  const groups = useMemo<ModGroupView[]>(() => {
    const raw = new Map<
      string,
      { key: string; lines: string[]; affix_type: EligibleModView["affix_type"]; concepts: string[]; tiers: EligibleModView[] }
    >();
    for (const m of mods) {
      let g = raw.get(m.mod_group);
      if (!g) {
        g = { key: m.mod_group, lines: modLines(m.text_template), affix_type: m.affix_type, concepts: m.concepts, tiers: [] };
        raw.set(m.mod_group, g);
      }
      g.tiers.push(m);
    }
    const all = [...raw.values()];

    // How many groups each stat line appears in. A line shared across many
    // hybrids (e.g. "#% increased Armour and Evasion") is "common"; the rarest
    // line in a group is the one that actually distinguishes it.
    const lineFreq = new Map<string, number>();
    for (const g of all) {
      for (const ln of new Set(g.lines)) lineFreq.set(ln, (lineFreq.get(ln) ?? 0) + 1);
    }

    const out: ModGroupView[] = all.map((g) => {
      const fallback = g.tiers[0]?.name ?? humanizeId(g.tiers[0]?.mod_id ?? g.key);
      g.tiers.sort((a, b) => a.tier_index - b.tier_index);
      // The distinguishing lines are the rarest in this group; any line that is
      // strictly more common (a shared/generic line) drops to dimmed secondary.
      const minFreq = g.lines.length ? Math.min(...g.lines.map((ln) => lineFreq.get(ln) ?? 0)) : 0;
      const primaryLines = g.lines.filter((ln) => (lineFreq.get(ln) ?? 0) === minFreq);
      const secondary = g.lines.filter((ln) => (lineFreq.get(ln) ?? 0) > minFreq);
      return {
        key: g.key,
        label: g.lines.join(" · ") || fallback,
        primary: primaryLines.join(" · ") || fallback,
        secondary,
        affix_type: g.affix_type,
        concepts: g.concepts,
        tiers: g.tiers,
      };
    });
    // Most-likely-to-roll groups first (best tier's weight share).
    out.sort((a, b) => (b.tiers[0]?.weight_share ?? 0) - (a.tiers[0]?.weight_share ?? 0));
    return out;
  }, [mods]);

  const filtered = useMemo(() => {
    const needle = q.toLowerCase();
    if (!needle) return groups.slice(0, 120);
    return groups
      .filter(
        (g) =>
          g.label.toLowerCase().includes(needle) ||
          g.concepts.some((c) => c.toLowerCase().includes(needle)) ||
          g.tiers.some(
            (t) => (t.name ?? "").toLowerCase().includes(needle) || t.mod_id.toLowerCase().includes(needle),
          ),
      )
      .slice(0, 120);
  }, [groups, q]);

  function pickTier(m: EligibleModView) {
    setPicked(m);
    const s = m.stats?.[0];
    setRoll(s ? Math.round((s.min + s.max) / 2) : 0);
  }

  function onGroupClick(g: ModGroupView) {
    if (g.tiers.length === 1) {
      pickTier(g.tiers[0]);
      return;
    }
    setExpanded((cur) => (cur === g.key ? null : g.key));
  }

  const stat = picked?.stats?.[0];

  function submit() {
    if (!picked) return;
    const outcome: RecordOutcome = {
      kind: "add_mod",
      mod_id: picked.mod_id,
      ...(picked.stats?.length ? { roll } : {}),
    };
    void apply(outcome, { actionLabel: `Add ${modGroupLabel(picked)}` });
  }

  return (
    <div className={styles.modeBody}>
      <div className={styles.addControls}>
        <div className="seg">
          {(["either", "prefix", "suffix"] as const).map((a) => (
            <button key={a} className={affix === a ? "on" : ""} onClick={() => setAffix(a)}>
              {a}
            </button>
          ))}
        </div>
        <div className={styles.search}>
          <Search size={13} className="faint" />
          <input
            className="field"
            placeholder="Search stat or concept…"
            value={q}
            onChange={(e) => setQ(e.target.value)}
            autoFocus
          />
        </div>
      </div>

      <div className={styles.modList}>
        {loading ? (
          <div className="empty-state faint">Loading eligible mods…</div>
        ) : filtered.length === 0 ? (
          <div className="empty-state faint">No eligible mods match.</div>
        ) : (
          filtered.map((g) => {
            const open = expanded === g.key;
            const single = g.tiers.length === 1;
            const groupPicked = picked?.mod_group === g.key;
            return (
              <div key={g.key} className={styles.groupWrap}>
                <button
                  className={`${styles.modItem} ${styles.groupRow} ${groupPicked ? styles.modPicked : ""}`}
                  onClick={() => onGroupClick(g)}
                  aria-expanded={single ? undefined : open}
                  title={g.label}
                >
                  <span className={styles.modDot} data-affix={g.affix_type} />
                  <span className={styles.modName}>
                    {g.primary}
                    {g.secondary.length > 0 && (
                      <span className={styles.modSub}> · {g.secondary.join(" · ")}</span>
                    )}
                  </span>
                  <span className="tag">{single ? `T${g.tiers[0].tier_index}` : `${g.tiers.length} tiers`}</span>
                  {single ? (
                    <span className="faint num">iL{g.tiers[0].required_level}</span>
                  ) : (
                    <ChevronDown
                      size={13}
                      className="faint"
                      style={{ transform: open ? "rotate(180deg)" : "none", transition: "transform 120ms" }}
                    />
                  )}
                </button>
                {open && !single && (
                  <div className={styles.tierList}>
                    {g.tiers.map((t) => (
                      <button
                        key={t.mod_id}
                        className={`${styles.tierRow} ${picked?.mod_id === t.mod_id ? styles.tierPicked : ""}`}
                        onClick={() => pickTier(t)}
                        title={t.name ?? t.mod_id}
                      >
                        <span className="tag">T{t.tier_index}</span>
                        <span className={styles.tierRange}>{tierRange(t)}</span>
                        <span className="faint num">iL{t.required_level}</span>
                        <span className="faint num">{(t.weight_share * 100).toFixed(1)}%</span>
                        {t.name && <span className={styles.tierName}>{t.name}</span>}
                      </button>
                    ))}
                  </div>
                )}
              </div>
            );
          })
        )}
      </div>

      {picked && (
        <div className={styles.pickedRow}>
          <div className={styles.pickedInfo}>
            <span className={`r-${item.rarity}`}>{modGroupLabel(picked)}</span>
            <span className="faint num">
              T{picked.tier_index}/{picked.tier_count} · iL{picked.required_level}
              {picked.name ? <span className="faint"> · {picked.name}</span> : null}
            </span>
          </div>
          {stat && (
            <label className={styles.rollControl}>
              <span className="faint">
                roll <span className="num">{roll}</span>
                <span className="faint num"> [{stat.min}–{stat.max}]</span>
              </span>
              <input
                type="range"
                min={stat.min}
                max={stat.max}
                step={1}
                value={roll}
                onChange={(e) => setRoll(Number(e.target.value))}
              />
            </label>
          )}
          <button className="btn btn-primary" onClick={submit}>
            Add ▸
          </button>
        </div>
      )}
    </div>
  );
}

/* ---------- Remove mod -------------------------------------------------- */

function RemoveMode() {
  const item = useCraft((s) => s.item);
  const apply = useCraft((s) => s.applyOutcome);

  const rows = [
    ...item.prefixes.map((m, i) => ({ affix: "prefix" as const, index: i, mod: m })),
    ...item.suffixes.map((m, i) => ({ affix: "suffix" as const, index: i, mod: m })),
  ];

  if (rows.length === 0) {
    return <div className="empty-state">This item has no explicit modifiers to remove.</div>;
  }

  return (
    <div className={styles.modeBody}>
      <div className={styles.modList}>
        {rows.map((r) => (
          <div key={`${r.affix}${r.index}`} className={styles.modItem}>
            <span className={styles.modDot} data-affix={r.affix} />
            <span className={styles.modName}>{humanizeId(r.mod.mod_id)}</span>
            <span className="tag">{r.affix}</span>
            <button
              className="btn"
              onClick={() =>
                void apply(
                  { kind: "remove_mod", affix: r.affix, index: r.index },
                  { actionLabel: `Remove ${humanizeId(r.mod.mod_id)}` },
                )
              }
            >
              Remove
            </button>
          </div>
        ))}
      </div>
    </div>
  );
}

/* ---------- Reroll values ----------------------------------------------- */

function RerollMode() {
  const item = useCraft((s) => s.item);
  const apply = useCraft((s) => s.applyOutcome);
  const [mods, setMods] = useState<RerollableMod[]>([]);
  const [loading, setLoading] = useState(true);
  const [values, setValues] = useState<Record<string, number[]>>({});
  const [sanctify, setSanctify] = useState(false);

  useEffect(() => {
    let live = true;
    setLoading(true);
    engine
      .rerollableMods(item, sanctify ? "OmenOfSanctification" : null)
      .then((r) => {
        if (!live) return;
        setMods(r.mods);
        const seed: Record<string, number[]> = {};
        for (const m of r.mods) {
          seed[`${m.slot}${m.index}`] = m.stats.map((s) => s.current);
        }
        setValues(seed);
        setLoading(false);
      })
      .catch(() => live && setLoading(false));
    return () => {
      live = false;
    };
  }, [item, sanctify]);

  if (loading) return <div className="empty-state faint">Loading rerollable mods…</div>;
  if (mods.length === 0)
    return <div className="empty-state">No mods on this item can be Divine-rerolled.</div>;

  function setStat(key: string, si: number, v: number) {
    setValues((prev) => {
      const arr = [...(prev[key] ?? [])];
      arr[si] = v;
      return { ...prev, [key]: arr };
    });
  }

  function submit() {
    const rolls = mods.map((m) => ({
      slot: m.slot,
      index: m.index,
      values: values[`${m.slot}${m.index}`] ?? m.stats.map((s) => s.current),
    }));
    void apply(
      { kind: "reroll_values", rolls, ...(sanctify ? { sanctify: true } : {}) },
      { actionLabel: sanctify ? "Sanctify reroll" : "Divine reroll" },
    );
  }

  return (
    <div className={styles.modeBody}>
      <label className={styles.sanctRow}>
        <input
          type="checkbox"
          checked={sanctify}
          onChange={(e) => setSanctify(e.target.checked)}
        />
        <span>Sanctify (widen bounds ×0.8–1.2)</span>
      </label>
      <div className={styles.modList}>
        {mods.map((m) => {
          const key = `${m.slot}${m.index}`;
          return (
            <div key={key} className={styles.rerollCard}>
              <div className={styles.rerollHead}>
                <span className={styles.modDot} data-affix={m.slot} />
                <span className={styles.modName}>{m.name ?? humanizeId(m.mod_id)}</span>
                <span className="tag">T{m.tier_index}/{m.tier_count}</span>
              </div>
              {m.stats.map((s, si) => (
                <label key={s.stat_id} className={styles.rollControl}>
                  <span className="faint">
                    {humanizeId(s.stat_id)}{" "}
                    <span className="num">{values[key]?.[si] ?? s.current}</span>
                    <span className="faint num"> [{s.min}–{s.max}]</span>
                  </span>
                  <input
                    type="range"
                    min={s.min}
                    max={s.max}
                    step={1}
                    value={values[key]?.[si] ?? s.current}
                    onChange={(e) => setStat(key, si, Number(e.target.value))}
                  />
                </label>
              ))}
            </div>
          );
        })}
      </div>
      <div className={styles.pickedRow}>
        <span className="faint">Record the values you rolled in-game.</span>
        <button className="btn btn-primary" onClick={submit}>
          Record reroll ▸
        </button>
      </div>
    </div>
  );
}

/* ---------- Rarity ------------------------------------------------------ */

function RarityMode() {
  const item = useCraft((s) => s.item);
  const apply = useCraft((s) => s.applyOutcome);
  const options: Rarity[] = ["normal", "magic", "rare"];
  return (
    <div className={styles.modeBody}>
      <div className="empty-state" style={{ paddingBottom: 8 }}>
        Set the item rarity (e.g. after a Transmute / Regal / Scour).
      </div>
      <div className={styles.rarityRow}>
        {options.map((r) => (
          <button
            key={r}
            className={`btn ${item.rarity === r ? styles.rarityOn : ""}`}
            onClick={() =>
              void apply({ kind: "set_rarity", rarity: r }, { actionLabel: `Set ${r}` })
            }
          >
            <span className={`r-${r}`} style={{ textTransform: "capitalize" }}>
              {r}
            </span>
          </button>
        ))}
      </div>
    </div>
  );
}

/* ---------- Dialog shell ------------------------------------------------ */

export function OutcomeDialog() {
  const close = useCraft((s) => s.closeOutcome);
  const item = useCraft((s) => s.item);
  const recs = useCraft((s) => s.recommendations);
  const [mode, setMode] = useState<Mode>("add");
  const [verdict, setVerdict] = useState<CannotApplyView | null>(null);

  const heroAction = recs[0]?.action;
  const heroCurrency =
    heroAction?.kind === "apply_currency" ? heroAction.currency : null;

  useEffect(() => {
    if (!heroCurrency) {
      setVerdict(null);
      return;
    }
    let live = true;
    engine
      .checkCanApply(item, heroCurrency)
      .then((v) => live && setVerdict(v))
      .catch(() => {});
    return () => {
      live = false;
    };
  }, [item, heroCurrency]);

  // Esc to close.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && close();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [close]);

  return (
    <div className="overlay" onClick={close}>
      <div className="dialog" onClick={(e) => e.stopPropagation()} role="dialog" aria-modal>
        <header className={styles.head}>
          <div>
            <div className="eyebrow">Record outcome</div>
            <div className={styles.headTitle}>What happened in-game?</div>
          </div>
          <button className="btn btn-ghost" onClick={close} aria-label="Close">
            <X size={16} />
          </button>
        </header>

        {recs[0] && (
          <div className={styles.context}>
            <span className="faint">Suggested:</span>
            <span className={`r-${item.rarity}`}>{actionLabel(recs[0].action)}</span>
            <span className="faint num">~{div(recs[0].expected_cost)}</span>
            {verdict && (
              <span
                className="tag"
                data-risk={verdict.kind === "ok" ? "low" : "high"}
                style={{ marginLeft: "auto" }}
              >
                {cannotLabel(verdict)}
              </span>
            )}
          </div>
        )}

        <div className={styles.tabs}>
          <div className="seg">
            {MODES.map((m) => (
              <button key={m.id} className={mode === m.id ? "on" : ""} onClick={() => setMode(m.id)}>
                {m.label}
              </button>
            ))}
          </div>
        </div>

        <div className={styles.scroll}>
          {mode === "add" && <AddMode />}
          {mode === "remove" && <RemoveMode />}
          {mode === "reroll" && <RerollMode />}
          {mode === "rarity" && <RarityMode />}
        </div>
      </div>
    </div>
  );
}
